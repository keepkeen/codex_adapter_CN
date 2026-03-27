use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::provider_profiles::BuiltInProviderProfile;
use codex_protocol::provider_profiles::ChatReasoningFormat;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

pub(crate) fn spawn_chat_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    _turn_state: Option<Arc<OnceLock<String>>>,
    provider_profile: Option<&'static BuiltInProviderProfile>,
) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        process_chat_sse(
            stream_response.bytes,
            tx_event,
            idle_timeout,
            telemetry,
            provider_profile,
        )
        .await;
    });
    ResponseStream { rx_event }
}

pub async fn process_chat_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    provider_profile: Option<&'static BuiltInProviderProfile>,
) where
    S: Stream<Item = Result<bytes::Bytes, codex_client::TransportError>> + Unpin,
{
    let mut stream = stream.eventsource();

    #[derive(Default, Debug)]
    struct ToolCallState {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let mut tool_calls: HashMap<usize, ToolCallState> = HashMap::new();
    let mut tool_call_order: Vec<usize> = Vec::new();
    let mut tool_call_order_seen: HashSet<usize> = HashSet::new();
    let mut tool_call_index_by_id: HashMap<String, usize> = HashMap::new();
    let mut next_tool_call_index = 0usize;
    let mut last_tool_call_index: Option<usize> = None;
    let mut assistant_item: Option<ResponseItem> = None;
    let mut reasoning_item: Option<ResponseItem> = None;
    let mut token_usage: Option<TokenUsage> = None;
    let completed_sent = false;

    async fn flush_and_complete(
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
        reasoning_item: &mut Option<ResponseItem>,
        assistant_item: &mut Option<ResponseItem>,
        token_usage: Option<TokenUsage>,
    ) {
        if let Some(reasoning) = reasoning_item.take() {
            let _ = tx_event
                .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                .await;
        }

        if let Some(assistant) = assistant_item.take() {
            let _ = tx_event
                .send(Ok(ResponseEvent::OutputItemDone(assistant)))
                .await;
        }

        let _ = tx_event
            .send(Ok(ResponseEvent::Completed {
                response_id: String::new(),
                token_usage,
            }))
            .await;
    }

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(sse_telemetry) = telemetry.as_ref() {
            sse_telemetry.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(err))) => {
                let _ = tx_event.send(Err(ApiError::Stream(err.to_string()))).await;
                return;
            }
            Ok(None) => {
                if !completed_sent {
                    flush_and_complete(
                        &tx_event,
                        &mut reasoning_item,
                        &mut assistant_item,
                        token_usage.clone(),
                    )
                    .await;
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        trace!("chat SSE event: {}", sse.data);

        let data = sse.data.trim();
        if data.is_empty() {
            continue;
        }

        if data == "[DONE]" || data == "DONE" {
            if !completed_sent {
                flush_and_complete(
                    &tx_event,
                    &mut reasoning_item,
                    &mut assistant_item,
                    token_usage.clone(),
                )
                .await;
            }
            return;
        }

        let value: Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(err) => {
                debug!("failed to parse chat SSE event: {err}, data: {data}");
                continue;
            }
        };

        if let Some(parsed_usage) = parse_chat_usage(&value, provider_profile) {
            token_usage = Some(parsed_usage);
        }

        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            continue;
        };

        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                append_reasoning_from_value(
                    &tx_event,
                    &mut reasoning_item,
                    delta,
                    provider_profile,
                )
                .await;

                if let Some(content) = delta.get("content") {
                    if content.is_array() {
                        if let Some(items) = content.as_array() {
                            for item in items {
                                if let Some(text) = item
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .filter(|text| !text.is_empty())
                                {
                                    append_assistant_text(
                                        &tx_event,
                                        &mut assistant_item,
                                        text.to_string(),
                                    )
                                    .await;
                                }
                            }
                        }
                    } else if let Some(text) = content.as_str().filter(|text| !text.is_empty()) {
                        append_assistant_text(&tx_event, &mut assistant_item, text.to_string())
                            .await;
                    }
                }

                if let Some(tool_call_values) = delta.get("tool_calls").and_then(Value::as_array) {
                    for tool_call in tool_call_values {
                        let mut index = tool_call
                            .get("index")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize);

                        let mut call_id_for_lookup = None;
                        if let Some(call_id) = tool_call.get("id").and_then(Value::as_str) {
                            call_id_for_lookup = Some(call_id.to_string());
                            if let Some(existing) = tool_call_index_by_id.get(call_id) {
                                index = Some(*existing);
                            }
                        }

                        if index.is_none() && call_id_for_lookup.is_none() {
                            index = last_tool_call_index;
                        }

                        let index = index.unwrap_or_else(|| {
                            while tool_calls.contains_key(&next_tool_call_index) {
                                next_tool_call_index += 1;
                            }
                            let index = next_tool_call_index;
                            next_tool_call_index += 1;
                            index
                        });

                        let call_state = tool_calls.entry(index).or_default();
                        if tool_call_order_seen.insert(index) {
                            tool_call_order.push(index);
                        }

                        if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                            call_state.id.get_or_insert_with(|| id.to_string());
                            tool_call_index_by_id.entry(id.to_string()).or_insert(index);
                        }

                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(Value::as_str)
                                && !name.is_empty()
                            {
                                call_state.name.get_or_insert_with(|| name.to_string());
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                call_state.arguments.push_str(arguments);
                            }
                        }

                        last_tool_call_index = Some(index);
                    }
                }
            }

            if let Some(message) = choice.get("message") {
                append_reasoning_from_value(
                    &tx_event,
                    &mut reasoning_item,
                    message,
                    provider_profile,
                )
                .await;
            }

            let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
            if finish_reason == Some("stop") {
                if let Some(reasoning) = reasoning_item.take() {
                    let _ = tx_event
                        .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                        .await;
                }

                if let Some(assistant) = assistant_item.take() {
                    let _ = tx_event
                        .send(Ok(ResponseEvent::OutputItemDone(assistant)))
                        .await;
                }
                continue;
            }

            if finish_reason == Some("length") {
                let _ = tx_event.send(Err(ApiError::ContextWindowExceeded)).await;
                return;
            }

            if finish_reason == Some("tool_calls") {
                if let Some(reasoning) = reasoning_item.take() {
                    let _ = tx_event
                        .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                        .await;
                }

                for index in tool_call_order.drain(..) {
                    let Some(state) = tool_calls.remove(&index) else {
                        continue;
                    };
                    tool_call_order_seen.remove(&index);
                    let ToolCallState {
                        id,
                        name,
                        arguments,
                    } = state;
                    let Some(name) = name else {
                        debug!("skipping tool call at index {index} because name is missing");
                        continue;
                    };
                    let item = ResponseItem::FunctionCall {
                        id: None,
                        name,
                        namespace: None,
                        arguments,
                        call_id: id.unwrap_or_else(|| format!("tool-call-{index}")),
                    };
                    let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                }
            }
        }
    }
}

async fn append_assistant_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    assistant_item: &mut Option<ResponseItem>,
    text: String,
) {
    if assistant_item.is_none() {
        let item = ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![],
            end_turn: None,
            phase: None,
        };
        *assistant_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Message { content, .. }) = assistant_item {
        content.push(ContentItem::OutputText { text: text.clone() });
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputTextDelta(text)))
            .await;
    }
}

async fn append_reasoning_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    text: String,
) {
    if reasoning_item.is_none() {
        let item = ResponseItem::Reasoning {
            id: String::new(),
            summary: vec![],
            content: Some(vec![]),
            encrypted_content: None,
        };
        *reasoning_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Reasoning {
        content: Some(content),
        ..
    }) = reasoning_item
    {
        let content_index = content.len() as i64;
        content.push(ReasoningItemContent::ReasoningText { text: text.clone() });

        let _ = tx_event
            .send(Ok(ResponseEvent::ReasoningContentDelta {
                delta: text,
                content_index,
            }))
            .await;
    }
}

async fn append_reasoning_from_value(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    value: &Value,
    provider_profile: Option<&BuiltInProviderProfile>,
) {
    let reasoning_format =
        provider_profile.and_then(|profile| profile.stream_dialect.reasoning_format);
    match reasoning_format {
        Some(ChatReasoningFormat::Reasoning) => {
            if let Some(reasoning) = value.get("reasoning") {
                append_reasoning_value(tx_event, reasoning_item, reasoning).await;
            }
        }
        Some(ChatReasoningFormat::ReasoningContent) => {
            if let Some(reasoning_content) = value.get("reasoning_content") {
                append_reasoning_value(tx_event, reasoning_item, reasoning_content).await;
            }
        }
        Some(ChatReasoningFormat::ReasoningDetails) => {
            if let Some(reasoning_details) = value.get("reasoning_details") {
                append_reasoning_value(tx_event, reasoning_item, reasoning_details).await;
            }
        }
        None => {
            if let Some(reasoning) = value.get("reasoning") {
                append_reasoning_value(tx_event, reasoning_item, reasoning).await;
            }
            if let Some(reasoning_content) = value.get("reasoning_content") {
                append_reasoning_value(tx_event, reasoning_item, reasoning_content).await;
            }
            if let Some(reasoning_details) = value.get("reasoning_details") {
                append_reasoning_value(tx_event, reasoning_item, reasoning_details).await;
            }
        }
    }
}

async fn append_reasoning_value(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    value: &Value,
) {
    let mut texts = Vec::new();
    collect_reasoning_texts(value, &mut texts);
    for text in texts {
        append_reasoning_text(tx_event, reasoning_item, text).await;
    }
}

fn collect_reasoning_texts(value: &Value, texts: &mut Vec<String>) {
    if let Some(text) = value.as_str() {
        texts.push(text.to_string());
        return;
    }

    if let Some(text) = value.get("text").and_then(Value::as_str) {
        texts.push(text.to_string());
        return;
    }

    if let Some(text) = value.get("content").and_then(Value::as_str) {
        texts.push(text.to_string());
        return;
    }

    if let Some(items) = value.as_array() {
        for item in items {
            collect_reasoning_texts(item, texts);
        }
    }
}

fn parse_chat_usage(
    value: &Value,
    provider_profile: Option<&BuiltInProviderProfile>,
) -> Option<TokenUsage> {
    let stream_dialect = provider_profile.map(|profile| profile.stream_dialect);
    let root_usage = stream_dialect
        .map(|dialect| dialect.parse_root_usage)
        .unwrap_or(true)
        .then(|| value.get("usage").and_then(parse_chat_usage_value))
        .flatten();
    let choice_usage = stream_dialect
        .map(|dialect| dialect.parse_choice_usage)
        .unwrap_or(true)
        .then(|| {
            value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| {
                    choices
                        .iter()
                        .find_map(|choice| choice.get("usage").and_then(parse_chat_usage_value))
                })
        })
        .flatten();

    root_usage.or(choice_usage)
}

fn parse_chat_usage_value(usage: &Value) -> Option<TokenUsage> {
    if usage.is_null() {
        return None;
    }

    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cached_input_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let reasoning_output_tokens = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .or_else(|| usage.pointer("/output_tokens_details/reasoning_tokens"))
        .or_else(|| usage.get("reasoning_output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));

    let has_any_usage_field = usage.get("prompt_tokens").is_some()
        || usage.get("input_tokens").is_some()
        || usage
            .pointer("/prompt_tokens_details/cached_tokens")
            .is_some()
        || usage
            .pointer("/input_tokens_details/cached_tokens")
            .is_some()
        || usage.get("completion_tokens").is_some()
        || usage.get("output_tokens").is_some()
        || usage
            .pointer("/completion_tokens_details/reasoning_tokens")
            .is_some()
        || usage
            .pointer("/output_tokens_details/reasoning_tokens")
            .is_some()
        || usage.get("reasoning_output_tokens").is_some()
        || usage.get("total_tokens").is_some();

    if !has_any_usage_field {
        return None;
    }

    Some(TokenUsage {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use codex_protocol::provider_profiles::DEEPSEEK_PROVIDER_ID;
    use codex_protocol::provider_profiles::built_in_provider_profile;
    use futures::TryStreamExt;
    use serde_json::json;
    use tokio_util::io::ReaderStream;

    fn build_body(events: &[Value]) -> String {
        let mut body = String::new();
        for event in events {
            body.push_str(&format!("event: message\ndata: {event}\n\n"));
        }
        body
    }

    async fn collect_events(body: &str) -> Vec<ResponseEvent> {
        collect_events_with_profile(body, None).await
    }

    async fn collect_events_with_profile(
        body: &str,
        provider_profile: Option<&'static BuiltInProviderProfile>,
    ) -> Vec<ResponseEvent> {
        let reader = ReaderStream::new(std::io::Cursor::new(body.to_string()))
            .map_err(|err| codex_client::TransportError::Network(err.to_string()));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);
        tokio::spawn(process_chat_sse(
            reader,
            tx,
            Duration::from_millis(1000),
            None,
            provider_profile,
        ));

        let mut output = Vec::new();
        while let Some(event) = rx.recv().await {
            output.push(event.expect("stream error"));
        }
        output
    }

    #[tokio::test]
    async fn completes_on_done_sentinel_without_json() {
        let events = collect_events("event: message\ndata: [DONE]\n\n").await;
        assert_matches!(&events[..], [ResponseEvent::Completed { .. }]);
    }

    #[tokio::test]
    async fn parses_reasoning_content_deltas() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "reasoning_content": "need tool",
                    "content": ""
                }
            }]
        });
        let finish = json!({
            "choices": [{
                "finish_reason": "stop"
            }]
        });
        let body = build_body(&[delta, finish]);
        let events = collect_events(&body).await;
        assert_matches!(
            &events[..],
            [
                ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. }),
                ResponseEvent::ReasoningContentDelta { delta, .. },
                ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. }),
                ResponseEvent::Completed { .. }
            ] if delta == "need tool"
        );
    }

    #[tokio::test]
    async fn explicit_profile_prefers_provider_reasoning_field() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "reasoning": "generic",
                    "reasoning_content": "provider specific",
                    "content": ""
                }
            }]
        });
        let finish = json!({
            "choices": [{
                "finish_reason": "stop"
            }]
        });
        let body = build_body(&[delta, finish]);
        let deepseek_profile = built_in_provider_profile(DEEPSEEK_PROVIDER_ID).expect("profile");
        let events = collect_events_with_profile(&body, Some(deepseek_profile)).await;

        assert_matches!(
            &events[..],
            [
                ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. }),
                ResponseEvent::ReasoningContentDelta { delta, .. },
                ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. }),
                ResponseEvent::Completed { .. }
            ] if delta == "provider specific"
        );
    }

    #[tokio::test]
    async fn parses_reasoning_details_deltas() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "reasoning_details": [{"text": "step 1"}],
                    "content": ""
                }
            }]
        });
        let finish = json!({
            "choices": [{
                "finish_reason": "stop"
            }]
        });
        let body = build_body(&[delta, finish]);
        let events = collect_events(&body).await;
        assert_matches!(
            &events[..],
            [
                ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. }),
                ResponseEvent::ReasoningContentDelta { delta, .. },
                ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. }),
                ResponseEvent::Completed { .. }
            ] if delta == "step 1"
        );
    }

    #[tokio::test]
    async fn stop_events_forward_usage_to_completed() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "content": "hello"
                }
            }]
        });
        let finish = json!({
            "usage": {
                "prompt_tokens": 120,
                "prompt_tokens_details": {
                    "cached_tokens": 20
                },
                "completion_tokens": 30,
                "completion_tokens_details": {
                    "reasoning_tokens": 7
                },
                "total_tokens": 150
            },
            "choices": [{
                "finish_reason": "stop"
            }]
        });
        let body = build_body(&[delta, finish]);
        let events = collect_events(&body).await;

        assert_matches!(
            &events[..],
            [
                ResponseEvent::OutputItemAdded(ResponseItem::Message { .. }),
                ResponseEvent::OutputTextDelta(text),
                ResponseEvent::OutputItemDone(ResponseItem::Message { .. }),
                ResponseEvent::Completed { token_usage: Some(usage), .. }
            ] if text == "hello"
                && *usage == TokenUsage {
                    input_tokens: 120,
                    cached_input_tokens: 20,
                    output_tokens: 30,
                    reasoning_output_tokens: 7,
                    total_tokens: 150,
                }
        );
    }

    #[tokio::test]
    async fn usage_only_events_are_preserved_until_done() {
        let events = collect_events(&build_body(&[json!({
            "usage": {
                "input_tokens": 80,
                "output_tokens": 12,
                "total_tokens": 92
            }
        })]))
        .await;

        assert_matches!(
            &events[..],
            [ResponseEvent::Completed { token_usage: Some(usage), .. }]
            if *usage == TokenUsage {
                input_tokens: 80,
                cached_input_tokens: 0,
                output_tokens: 12,
                reasoning_output_tokens: 0,
                total_tokens: 92,
            }
        );
    }

    #[tokio::test]
    async fn choice_scoped_usage_is_forwarded_on_stop() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "content": "OK"
                }
            }]
        });
        let finish = json!({
            "choices": [{
                "finish_reason": "stop",
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 7,
                    "total_tokens": 19
                }
            }]
        });
        let body = build_body(&[delta, finish]);
        let events = collect_events(&body).await;

        assert_matches!(
            events.last(),
            Some(ResponseEvent::Completed {
                token_usage: Some(TokenUsage {
                    input_tokens: 12,
                    output_tokens: 7,
                    total_tokens: 19,
                    ..
                }),
                ..
            })
        );
    }

    #[tokio::test]
    async fn trailing_usage_chunk_after_stop_overrides_null_usage() {
        let delta = json!({
            "choices": [{
                "delta": {
                    "content": "OK"
                }
            }],
            "usage": null
        });
        let finish = json!({
            "choices": [{
                "finish_reason": "stop"
            }],
            "usage": null
        });
        let trailing_usage = json!({
            "choices": [],
            "usage": {
                "prompt_tokens": 46,
                "completion_tokens": 68,
                "completion_tokens_details": {
                    "reasoning_tokens": 66
                },
                "total_tokens": 114
            }
        });
        let body = build_body(&[delta, finish, trailing_usage]);
        let events = collect_events(&body).await;

        assert_matches!(
            events.last(),
            Some(ResponseEvent::Completed {
                token_usage: Some(TokenUsage {
                    input_tokens: 46,
                    output_tokens: 68,
                    reasoning_output_tokens: 66,
                    total_tokens: 114,
                    ..
                }),
                ..
            })
        );
    }
}
