use crate::api_bridge::map_api_error;
use crate::client::ModelClient;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::compact::content_items_to_text;
use crate::error::CodexErr;
use crate::error::Result;
use codex_otel::SessionTelemetry;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::TokenUsage;
use futures::StreamExt;
use jsonschema::validator_for;
use serde_json::Value;
use tokio::sync::mpsc;

const STRUCTURED_OUTPUT_MAX_ATTEMPTS: usize = 3;

#[derive(Debug)]
struct CollectedAttempt {
    response_id: String,
    passthrough_events: Vec<ResponseEvent>,
    items: Vec<ResponseItem>,
    token_usage: Option<TokenUsage>,
}

#[derive(Debug, PartialEq)]
enum ValidationDisposition {
    ToolBearing,
    Validated(Vec<ResponseItem>),
    Invalid(String),
}

pub(crate) async fn stream_chat_completions_with_output_schema(
    client: ModelClient,
    prompt: Prompt,
    model_info: ModelInfo,
    session_telemetry: SessionTelemetry,
) -> Result<ResponseStream> {
    let Some(schema) = prompt.output_schema.clone() else {
        return Err(CodexErr::InvalidRequest(
            "output_schema wrapper requires an output schema".to_string(),
        ));
    };
    validate_output_schema(&schema)?;

    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
    tokio::spawn(async move {
        let result = drive_structured_output_turn(
            client,
            prompt,
            model_info,
            session_telemetry.clone(),
            schema,
            tx_event.clone(),
        )
        .await;
        if let Err(err) = result {
            session_telemetry.see_event_completed_failed(&err);
            let _ = tx_event.send(Err(err)).await;
        }
    });

    Ok(ResponseStream { rx_event })
}

async fn drive_structured_output_turn(
    client: ModelClient,
    prompt: Prompt,
    model_info: ModelInfo,
    session_telemetry: SessionTelemetry,
    schema: Value,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    let mut aggregated_usage = TokenUsage::default();
    let mut saw_usage = false;
    let mut attempt_prompt = prompt;

    for attempt_index in 0..STRUCTURED_OUTPUT_MAX_ATTEMPTS {
        let api_stream = client
            .stream_chat_completions_raw(&attempt_prompt, &model_info, &session_telemetry)
            .await?;
        let attempt = collect_attempt(api_stream).await?;
        if let Some(usage) = &attempt.token_usage {
            aggregated_usage.add_assign(usage);
            saw_usage = true;
        }

        match validate_attempt_items(&schema, &attempt.items)? {
            ValidationDisposition::ToolBearing => {
                return emit_attempt(
                    tx_event,
                    attempt.passthrough_events,
                    attempt.items,
                    attempt.response_id,
                    saw_usage.then_some(aggregated_usage),
                    &session_telemetry,
                )
                .await;
            }
            ValidationDisposition::Validated(items) => {
                return emit_attempt(
                    tx_event,
                    attempt.passthrough_events,
                    items,
                    attempt.response_id,
                    saw_usage.then_some(aggregated_usage),
                    &session_telemetry,
                )
                .await;
            }
            ValidationDisposition::Invalid(failure) => {
                if attempt_index + 1 == STRUCTURED_OUTPUT_MAX_ATTEMPTS {
                    return Err(CodexErr::InvalidRequest(format!(
                        "chat provider failed to satisfy output_schema after {STRUCTURED_OUTPUT_MAX_ATTEMPTS} attempts: {failure}"
                    )));
                }
                tracing::warn!(
                    attempt = attempt_index + 1,
                    failure,
                    "retrying chat provider structured output repair"
                );
                attempt_prompt =
                    build_repair_prompt(attempt_prompt, &attempt.items, &schema, &failure)?;
            }
        }
    }

    Err(CodexErr::InvalidRequest(
        "structured output retry loop exhausted unexpectedly".to_string(),
    ))
}

async fn emit_attempt(
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    passthrough_events: Vec<ResponseEvent>,
    items: Vec<ResponseItem>,
    response_id: String,
    token_usage: Option<TokenUsage>,
    session_telemetry: &SessionTelemetry,
) -> Result<()> {
    if let Some(usage) = &token_usage {
        session_telemetry.sse_event_completed(
            usage.input_tokens,
            usage.output_tokens,
            Some(usage.cached_input_tokens),
            Some(usage.reasoning_output_tokens),
            usage.total_tokens,
        );
    }

    for event in passthrough_events {
        if tx_event.send(Ok(event)).await.is_err() {
            return Ok(());
        }
    }
    for item in items {
        if tx_event
            .send(Ok(ResponseEvent::OutputItemDone(item)))
            .await
            .is_err()
        {
            return Ok(());
        }
    }
    let _ = tx_event
        .send(Ok(ResponseEvent::Completed {
            response_id,
            token_usage,
        }))
        .await;
    Ok(())
}

async fn collect_attempt(mut api_stream: codex_api::ResponseStream) -> Result<CollectedAttempt> {
    let mut passthrough_events = Vec::new();
    let mut items = Vec::new();

    while let Some(event) = api_stream.next().await {
        match event {
            Ok(ResponseEvent::OutputItemDone(item)) => items.push(item),
            Ok(ResponseEvent::Completed {
                response_id,
                token_usage,
            }) => {
                return Ok(CollectedAttempt {
                    response_id,
                    passthrough_events,
                    items,
                    token_usage,
                });
            }
            Ok(
                passthrough @ (ResponseEvent::Created
                | ResponseEvent::ServerModel(_)
                | ResponseEvent::ServerReasoningIncluded(_)
                | ResponseEvent::RateLimits(_)
                | ResponseEvent::ModelsEtag(_)),
            ) => passthrough_events.push(passthrough),
            Ok(ResponseEvent::OutputItemAdded(_))
            | Ok(ResponseEvent::OutputTextDelta(_))
            | Ok(ResponseEvent::ReasoningSummaryDelta { .. })
            | Ok(ResponseEvent::ReasoningContentDelta { .. })
            | Ok(ResponseEvent::ReasoningSummaryPartAdded { .. }) => {}
            Err(err) => return Err(map_api_error(err)),
        }
    }

    Err(CodexErr::Stream(
        "stream closed before response.completed".to_string(),
        None,
    ))
}

fn validate_attempt_items(schema: &Value, items: &[ResponseItem]) -> Result<ValidationDisposition> {
    if items.iter().any(is_tool_call_item) {
        return Ok(ValidationDisposition::ToolBearing);
    }

    let Some(message_index) = items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| match item {
            ResponseItem::Message { role, .. } if role == "assistant" => Some(index),
            _ => None,
        })
    else {
        return Ok(ValidationDisposition::Invalid(
            "chat completion returned no final assistant message".to_string(),
        ));
    };

    let ResponseItem::Message { content, .. } = &items[message_index] else {
        unreachable!("message index should always point at an assistant message");
    };
    let Some(message_text) = content_items_to_text(content) else {
        return Ok(ValidationDisposition::Invalid(
            "assistant message did not contain text to validate against output_schema".to_string(),
        ));
    };
    let parsed = match serde_json::from_str::<Value>(&message_text) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Ok(ValidationDisposition::Invalid(format!(
                "assistant message was not valid JSON: {err}"
            )));
        }
    };

    let validator = validator_for(schema)
        .map_err(|err| CodexErr::InvalidRequest(format!("invalid output_schema: {err}")))?;
    if !validator.is_valid(&parsed) {
        let errors = validator
            .iter_errors(&parsed)
            .take(3)
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Ok(ValidationDisposition::Invalid(format!(
            "assistant JSON did not satisfy output_schema: {errors}"
        )));
    }

    let normalized_json = serde_json::to_string(&parsed)?;
    let mut normalized_items = items.to_vec();
    if let ResponseItem::Message { content, .. } = &mut normalized_items[message_index] {
        *content = vec![ContentItem::OutputText {
            text: normalized_json,
        }];
    }

    Ok(ValidationDisposition::Validated(normalized_items))
}

fn build_repair_prompt(
    mut prompt: Prompt,
    attempt_items: &[ResponseItem],
    schema: &Value,
    failure: &str,
) -> Result<Prompt> {
    let schema_json = serde_json::to_string(schema)?;

    prompt.input.extend_from_slice(attempt_items);
    prompt.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "Your previous response did not satisfy the required JSON schema.\nValidation failure: {failure}\nReturn only a JSON value that matches this schema exactly. Do not include markdown fences, explanations, or any extra text.\nSchema: {schema_json}"
            ),
        }],
        end_turn: None,
        phase: None,
    });
    prompt.tools.clear();
    prompt.parallel_tool_calls = false;

    Ok(prompt)
}

fn validate_output_schema(schema: &Value) -> Result<()> {
    validator_for(schema)
        .map(|_| ())
        .map_err(|err| CodexErr::InvalidRequest(format!("invalid output_schema: {err}")))
}

fn is_tool_call_item(item: &ResponseItem) -> bool {
    matches!(
        item,
        ResponseItem::FunctionCall { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::ToolSearchCall { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn assistant_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    #[test]
    fn validate_attempt_items_normalizes_valid_json() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });
        let items = vec![assistant_message("{\"answer\":\"ok\"}")];

        let disposition = validate_attempt_items(&schema, &items).expect("validate attempt");

        let ValidationDisposition::Validated(normalized) = disposition else {
            panic!("expected validated structured output");
        };
        assert_eq!(normalized, vec![assistant_message("{\"answer\":\"ok\"}")]);
    }

    #[test]
    fn validate_attempt_items_reports_invalid_shape() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });
        let items = vec![assistant_message("{\"wrong\":true}")];

        let disposition = validate_attempt_items(&schema, &items).expect("validate attempt");

        let ValidationDisposition::Invalid(reason) = disposition else {
            panic!("expected invalid structured output");
        };
        assert!(
            reason.contains("did not satisfy output_schema"),
            "expected schema validation error, got: {reason}"
        );
    }

    #[test]
    fn validate_attempt_items_reports_invalid_json() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });
        let items = vec![assistant_message("{\"answer\":}")];

        let disposition = validate_attempt_items(&schema, &items).expect("validate attempt");

        let ValidationDisposition::Invalid(reason) = disposition else {
            panic!("expected invalid structured output");
        };
        assert!(
            reason.contains("was not valid JSON"),
            "expected JSON parse error, got: {reason}"
        );
    }

    #[test]
    fn validate_attempt_items_skips_tool_bearing_turns() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });
        let items = vec![ResponseItem::FunctionCall {
            id: None,
            name: "read_file".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
        }];

        let disposition = validate_attempt_items(&schema, &items).expect("validate attempt");

        assert_eq!(disposition, ValidationDisposition::ToolBearing);
    }

    #[test]
    fn build_repair_prompt_appends_invalid_attempt_and_repair_user_message() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });
        let base_prompt = Prompt {
            output_schema: Some(schema.clone()),
            ..Prompt::default()
        };
        let attempt_items = vec![assistant_message("{\"wrong\":true}")];

        let repair = build_repair_prompt(
            base_prompt,
            &attempt_items,
            &schema,
            "assistant JSON did not satisfy output_schema: missing required property `answer`",
        )
        .expect("repair prompt should build");

        assert_eq!(repair.tools.len(), 0);
        assert!(!repair.parallel_tool_calls);
        assert_eq!(repair.input.len(), 2);
        let ResponseItem::Message { role, content, .. } = &repair.input[1] else {
            panic!("expected repair user message");
        };
        assert_eq!(role, "user");
        assert!(
            content_items_to_text(content)
                .expect("repair text")
                .contains("Return only a JSON value"),
            "repair prompt should enforce strict JSON output"
        );
    }
}
