use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::headers::build_conversation_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::LocalShellStatus;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionSource;
use codex_protocol::provider_profiles::BuiltInProviderProfile;
use codex_protocol::provider_profiles::ChatModelRewrite;
use codex_protocol::provider_profiles::ChatReasoningFormat;
use codex_protocol::provider_profiles::StructuredOutputStrategy;
use http::HeaderMap;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

/// Assembled request body plus headers for Chat Completions streaming calls.
pub struct ChatRequest {
    pub body: Value,
    pub headers: HeaderMap,
    pub provider_profile: Option<&'static BuiltInProviderProfile>,
}

pub struct ChatRequestBuilder<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a [ResponseItem],
    tools: &'a [Value],
    output_schema: Option<&'a Value>,
    conversation_id: Option<String>,
    session_source: Option<SessionSource>,
    provider_profile: Option<&'static BuiltInProviderProfile>,
}

struct ProviderBehavior {
    model: String,
    reasoning_format: Option<ChatReasoningFormat>,
    supports_developer_role: bool,
    merges_system_messages: bool,
    hoists_system_messages_to_front: bool,
    request_stream_usage: bool,
    structured_output_strategy: StructuredOutputStrategy,
    extra_body: Map<String, Value>,
}

impl<'a> ChatRequestBuilder<'a> {
    pub fn new(
        model: &'a str,
        instructions: &'a str,
        input: &'a [ResponseItem],
        tools: &'a [Value],
    ) -> Self {
        Self {
            model,
            instructions,
            input,
            tools,
            output_schema: None,
            conversation_id: None,
            session_source: None,
            provider_profile: None,
        }
    }

    pub fn conversation_id(mut self, id: Option<String>) -> Self {
        self.conversation_id = id;
        self
    }

    pub fn session_source(mut self, source: Option<SessionSource>) -> Self {
        self.session_source = source;
        self
    }

    pub fn provider_profile(mut self, profile: Option<&'static BuiltInProviderProfile>) -> Self {
        self.provider_profile = profile;
        self
    }

    pub fn output_schema(mut self, output_schema: Option<&'a Value>) -> Self {
        self.output_schema = output_schema;
        self
    }

    pub fn build(self, provider: &Provider) -> Result<ChatRequest, ApiError> {
        let behavior = provider_behavior(provider, self.provider_profile, self.model);
        let mut messages = Vec::<Value>::new();
        messages.push(json!({"role": "system", "content": self.instructions}));

        let mut reasoning_by_anchor_index: HashMap<usize, String> = HashMap::new();
        let mut last_emitted_role: Option<&str> = None;
        for item in self.input {
            match item {
                ResponseItem::Message { role, .. } => last_emitted_role = Some(role.as_str()),
                ResponseItem::FunctionCall { .. }
                | ResponseItem::LocalShellCall { .. }
                | ResponseItem::CustomToolCall { .. }
                | ResponseItem::ToolSearchCall { .. } => last_emitted_role = Some("assistant"),
                ResponseItem::FunctionCallOutput { .. }
                | ResponseItem::CustomToolCallOutput { .. }
                | ResponseItem::ToolSearchOutput { .. } => last_emitted_role = Some("tool"),
                ResponseItem::Reasoning { .. }
                | ResponseItem::WebSearchCall { .. }
                | ResponseItem::ImageGenerationCall { .. }
                | ResponseItem::GhostSnapshot { .. }
                | ResponseItem::Compaction { .. }
                | ResponseItem::Other => {}
            }
        }

        let mut last_user_index: Option<usize> = None;
        for (idx, item) in self.input.iter().enumerate() {
            if let ResponseItem::Message { role, .. } = item
                && role == "user"
            {
                last_user_index = Some(idx);
            }
        }

        if !matches!(last_emitted_role, Some("user")) {
            for (idx, item) in self.input.iter().enumerate() {
                if let Some(user_idx) = last_user_index
                    && idx <= user_idx
                {
                    continue;
                }

                if let ResponseItem::Reasoning {
                    content: Some(items),
                    ..
                } = item
                {
                    let mut text = String::new();
                    for entry in items {
                        match entry {
                            codex_protocol::models::ReasoningItemContent::ReasoningText {
                                text: segment,
                            }
                            | codex_protocol::models::ReasoningItemContent::Text {
                                text: segment,
                            } => text.push_str(segment),
                        }
                    }
                    if text.trim().is_empty() {
                        continue;
                    }

                    let mut attached = false;
                    if idx > 0
                        && let ResponseItem::Message { role, .. } = &self.input[idx - 1]
                        && role == "assistant"
                    {
                        reasoning_by_anchor_index
                            .entry(idx - 1)
                            .and_modify(|value| value.push_str(&text))
                            .or_insert(text.clone());
                        attached = true;
                    }

                    if !attached && idx + 1 < self.input.len() {
                        match &self.input[idx + 1] {
                            ResponseItem::FunctionCall { .. }
                            | ResponseItem::LocalShellCall { .. }
                            | ResponseItem::CustomToolCall { .. }
                            | ResponseItem::ToolSearchCall { .. } => {
                                reasoning_by_anchor_index
                                    .entry(idx + 1)
                                    .and_modify(|value| value.push_str(&text))
                                    .or_insert(text.clone());
                            }
                            ResponseItem::Message { role, .. } if role == "assistant" => {
                                reasoning_by_anchor_index
                                    .entry(idx + 1)
                                    .and_modify(|value| value.push_str(&text))
                                    .or_insert(text.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let mut last_assistant_text: Option<String> = None;

        for (idx, item) in self.input.iter().enumerate() {
            match item {
                ResponseItem::Message { role, content, .. } => {
                    let mut text = String::new();
                    let mut items: Vec<Value> = Vec::new();
                    let mut saw_image = false;
                    let role = chat_message_role(role, &behavior);

                    for content_item in content {
                        match content_item {
                            ContentItem::InputText { text: value }
                            | ContentItem::OutputText { text: value } => {
                                text.push_str(value);
                                items.push(json!({"type":"text","text": value}));
                            }
                            ContentItem::InputImage { image_url } => {
                                saw_image = true;
                                items.push(
                                    json!({"type":"image_url","image_url": {"url": image_url}}),
                                );
                            }
                        }
                    }

                    if role == "assistant" {
                        if let Some(previous) = &last_assistant_text
                            && previous == &text
                        {
                            continue;
                        }
                        last_assistant_text = Some(text.clone());

                        let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                        if append_assistant_content_to_pending_tool_call(
                            &mut messages,
                            &text,
                            behavior.reasoning_format,
                            reasoning,
                        ) {
                            continue;
                        }
                    }

                    if role == "system" {
                        if behavior.hoists_system_messages_to_front
                            && let Some(Value::Object(first)) = messages.first_mut()
                            && first.get("role").and_then(Value::as_str) == Some("system")
                            && let Some(existing) = first
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        {
                            let merged = if existing.is_empty() {
                                text.clone()
                            } else if text.is_empty() {
                                existing
                            } else {
                                format!("{existing}\n\n{text}")
                            };
                            first.insert("content".to_string(), Value::String(merged));
                            continue;
                        }

                        if behavior.merges_system_messages
                            && let Some(Value::Object(previous)) = messages.last_mut()
                            && previous.get("role").and_then(Value::as_str) == Some("system")
                            && let Some(existing) = previous
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        {
                            let merged = if existing.is_empty() {
                                text.clone()
                            } else if text.is_empty() {
                                existing
                            } else {
                                format!("{existing}\n\n{text}")
                            };
                            previous.insert("content".to_string(), Value::String(merged));
                            continue;
                        }
                    }

                    let content_value = if role == "assistant" {
                        json!(text)
                    } else if saw_image {
                        json!(items)
                    } else {
                        json!(text)
                    };

                    let mut message = json!({"role": role, "content": content_value});
                    if role == "assistant"
                        && let Some(reasoning) = reasoning_by_anchor_index.get(&idx)
                    {
                        attach_reasoning_field(&mut message, behavior.reasoning_format, reasoning);
                    }
                    messages.push(message);
                }
                ResponseItem::FunctionCall {
                    name,
                    namespace,
                    arguments,
                    call_id,
                    ..
                } => {
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    let tool_call = json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": qualified_tool_name(name, namespace.as_deref()),
                            "arguments": arguments,
                        }
                    });
                    push_tool_call_message(
                        &mut messages,
                        tool_call,
                        behavior.reasoning_format,
                        reasoning,
                    );
                }
                ResponseItem::LocalShellCall {
                    id,
                    call_id,
                    status,
                    action,
                } => {
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    let tool_call = json!({
                        "id": call_id
                            .clone()
                            .or_else(|| id.clone())
                            .unwrap_or_else(|| format!("local-shell-{idx}")),
                        "type": "function",
                        "function": {
                            "name": "local_shell_call",
                            "arguments": local_shell_call_arguments(status, action)?,
                        }
                    });
                    push_tool_call_message(
                        &mut messages,
                        tool_call,
                        behavior.reasoning_format,
                        reasoning,
                    );
                }
                ResponseItem::FunctionCallOutput { call_id, output } => {
                    let content_value = if let Some(items) = output.content_items() {
                        let mapped: Vec<Value> = items
                            .iter()
                            .map(|item| match item {
                                FunctionCallOutputContentItem::InputText { text } => {
                                    json!({"type":"text","text": text})
                                }
                                FunctionCallOutputContentItem::InputImage { image_url, .. } => {
                                    json!({"type":"image_url","image_url": {"url": image_url}})
                                }
                            })
                            .collect();
                        json!(mapped)
                    } else {
                        json!(output.text_content().unwrap_or_default())
                    };

                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": content_value,
                    }));
                }
                ResponseItem::CustomToolCall {
                    id: _,
                    call_id,
                    name,
                    input,
                    status: _,
                } => {
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    let tool_call = json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": input,
                        }
                    });
                    push_tool_call_message(
                        &mut messages,
                        tool_call,
                        behavior.reasoning_format,
                        reasoning,
                    );
                }
                ResponseItem::CustomToolCallOutput {
                    call_id, output, ..
                } => {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output,
                    }));
                }
                ResponseItem::ToolSearchCall {
                    call_id,
                    execution,
                    arguments,
                    ..
                } => {
                    let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                    let tool_call = json!({
                        "id": call_id.clone().unwrap_or_else(|| format!("tool-search-{idx}")),
                        "type": "function",
                        "function": {
                            "name": execution,
                            "arguments": serde_json::to_string(arguments).map_err(|err| {
                                ApiError::Stream(format!(
                                    "failed to encode tool search arguments: {err}"
                                ))
                            })?,
                        }
                    });
                    push_tool_call_message(
                        &mut messages,
                        tool_call,
                        behavior.reasoning_format,
                        reasoning,
                    );
                }
                ResponseItem::ToolSearchOutput {
                    call_id,
                    status,
                    execution,
                    tools,
                } => {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id.clone().unwrap_or_else(|| execution.clone()),
                        "content": serde_json::to_string(&json!({
                            "status": status,
                            "execution": execution,
                            "tools": tools,
                        }))
                        .map_err(|err| {
                            ApiError::Stream(format!(
                                "failed to encode tool search output: {err}"
                            ))
                        })?,
                    }));
                }
                ResponseItem::Reasoning { .. }
                | ResponseItem::WebSearchCall { .. }
                | ResponseItem::ImageGenerationCall { .. }
                | ResponseItem::GhostSnapshot { .. }
                | ResponseItem::Compaction { .. }
                | ResponseItem::Other => continue,
            }
        }

        let mut payload = Map::from_iter([
            ("model".to_string(), Value::String(behavior.model)),
            ("messages".to_string(), Value::Array(messages)),
            ("stream".to_string(), Value::Bool(true)),
        ]);
        if behavior.request_stream_usage {
            payload.insert(
                "stream_options".to_string(),
                json!({ "include_usage": true }),
            );
        }
        if self.output_schema.is_some()
            && let Some(response_format) = chat_response_format(behavior.structured_output_strategy)
        {
            payload.insert("response_format".to_string(), response_format);
        }
        if !self.tools.is_empty() {
            payload.insert("tools".to_string(), Value::Array(self.tools.to_vec()));
            payload.insert("tool_choice".to_string(), Value::String("auto".to_string()));
        }
        payload.extend(behavior.extra_body);

        let mut headers = build_conversation_headers(self.conversation_id);
        if let Some(subagent) = subagent_header(&self.session_source) {
            insert_header(&mut headers, "x-openai-subagent", &subagent);
        }

        Ok(ChatRequest {
            body: Value::Object(payload),
            headers,
            provider_profile: self.provider_profile,
        })
    }
}

fn chat_message_role<'a>(role: &'a str, behavior: &ProviderBehavior) -> &'a str {
    match role {
        "developer" if !behavior.supports_developer_role => "system",
        _ => role,
    }
}

fn qualified_tool_name(name: &str, namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}/{name}"),
        None => name.to_string(),
    }
}

fn local_shell_call_arguments(
    status: &LocalShellStatus,
    action: &LocalShellAction,
) -> Result<String, ApiError> {
    serde_json::to_string(&json!({
        "status": status,
        "action": action,
    }))
    .map_err(|err| ApiError::Stream(format!("failed to encode local shell call: {err}")))
}

fn attach_reasoning_field(
    message: &mut Value,
    reasoning_format: Option<ChatReasoningFormat>,
    reasoning: &str,
) {
    let Some(object) = message.as_object_mut() else {
        return;
    };
    match reasoning_format {
        Some(ChatReasoningFormat::Reasoning) => {
            object.insert(
                "reasoning".to_string(),
                Value::String(reasoning.to_string()),
            );
        }
        Some(ChatReasoningFormat::ReasoningContent) => {
            object.insert(
                "reasoning_content".to_string(),
                Value::String(reasoning.to_string()),
            );
        }
        Some(ChatReasoningFormat::ReasoningDetails) => {
            object.insert(
                "reasoning_details".to_string(),
                json!([{ "text": reasoning }]),
            );
        }
        None => {}
    }
}

fn push_tool_call_message(
    messages: &mut Vec<Value>,
    tool_call: Value,
    reasoning_format: Option<ChatReasoningFormat>,
    reasoning: Option<&str>,
) {
    if let Some(Value::Object(object)) = messages.last_mut()
        && object.get("role").and_then(Value::as_str) == Some("assistant")
        && object.get("content").is_some_and(Value::is_null)
        && let Some(tool_calls) = object.get_mut("tool_calls").and_then(Value::as_array_mut)
    {
        tool_calls.push(tool_call);
        if let Some(reasoning) = reasoning {
            let mut tmp = Value::Object(object.clone());
            attach_reasoning_field(&mut tmp, reasoning_format, reasoning);
            if let Some(updated) = tmp.as_object() {
                *object = updated.clone();
            }
        }
        return;
    }

    let mut message = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tool_call],
    });
    if let Some(reasoning) = reasoning {
        attach_reasoning_field(&mut message, reasoning_format, reasoning);
    }
    messages.push(message);
}

fn append_assistant_content_to_pending_tool_call(
    messages: &mut [Value],
    text: &str,
    reasoning_format: Option<ChatReasoningFormat>,
    reasoning: Option<&str>,
) -> bool {
    let Some(Value::Object(object)) = messages.last_mut() else {
        return false;
    };
    if object.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }
    if object.get("tool_calls").and_then(Value::as_array).is_none() {
        return false;
    }

    let existing_content = match object.get("content") {
        Some(Value::Null) => String::new(),
        Some(Value::String(existing)) => existing.clone(),
        _ => return false,
    };

    if !text.is_empty() {
        object.insert(
            "content".to_string(),
            Value::String(format!("{existing_content}{text}")),
        );
    }

    if let Some(reasoning) = reasoning {
        let mut tmp = Value::Object(object.clone());
        attach_reasoning_field(&mut tmp, reasoning_format, reasoning);
        if let Some(updated) = tmp.as_object() {
            *object = updated.clone();
        }
    }

    true
}

fn provider_behavior(
    provider: &Provider,
    provider_profile: Option<&BuiltInProviderProfile>,
    model: &str,
) -> ProviderBehavior {
    if let Some(profile) = provider_profile {
        return provider_behavior_from_profile(profile, model);
    }

    let provider_name = provider.name.to_ascii_lowercase();
    let provider_base = provider.base_url.to_ascii_lowercase();

    ProviderBehavior {
        model: model.to_string(),
        reasoning_format: Some(ChatReasoningFormat::Reasoning),
        supports_developer_role: provider_base.contains("openai.com")
            || provider_base.contains("chatgpt.com")
            || provider_name.contains("openai"),
        merges_system_messages: false,
        hoists_system_messages_to_front: false,
        request_stream_usage: false,
        structured_output_strategy: StructuredOutputStrategy::Unsupported,
        extra_body: Map::new(),
    }
}

fn provider_behavior_from_profile(
    profile: &BuiltInProviderProfile,
    model: &str,
) -> ProviderBehavior {
    let mut extra_body = Map::new();
    let model = apply_chat_model_rewrite(
        profile.request_dialect.model_rewrite,
        model,
        &mut extra_body,
    );
    if profile.descriptor.provider_id == codex_protocol::provider_profiles::MINIMAX_PROVIDER_ID {
        extra_body.insert("reasoning_split".to_string(), Value::Bool(true));
    }

    ProviderBehavior {
        model,
        reasoning_format: profile.request_dialect.reasoning_format,
        supports_developer_role: profile.request_dialect.supports_developer_role,
        merges_system_messages: profile.request_dialect.merges_system_messages,
        hoists_system_messages_to_front: profile.request_dialect.hoists_system_messages_to_front,
        request_stream_usage: profile.request_dialect.request_stream_usage,
        structured_output_strategy: profile.request_dialect.structured_output_strategy,
        extra_body,
    }
}

fn chat_response_format(strategy: StructuredOutputStrategy) -> Option<Value> {
    match strategy {
        StructuredOutputStrategy::JsonObjectPlusLocalValidation => {
            Some(json!({ "type": "json_object" }))
        }
        StructuredOutputStrategy::NativeJsonSchema
        | StructuredOutputStrategy::PlainTextPlusLocalValidation
        | StructuredOutputStrategy::Unsupported => None,
    }
}

fn apply_chat_model_rewrite(
    model_rewrite: ChatModelRewrite,
    model: &str,
    extra_body: &mut Map<String, Value>,
) -> String {
    match model_rewrite {
        ChatModelRewrite::Identity => model.to_string(),
        ChatModelRewrite::DeepSeekThinking => match model {
            "deepseek-chat-thinking" | "deepseek-thinking" => {
                extra_body.insert("thinking".to_string(), json!({ "type": "enabled" }));
                "deepseek-chat".to_string()
            }
            other => other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::RetryConfig;
    use codex_protocol::models::FunctionCallOutputPayload;
    use codex_protocol::models::LocalShellExecAction;
    use codex_protocol::protocol::SessionSource;
    use codex_protocol::protocol::SubAgentSource;
    use codex_protocol::provider_profiles::DEEPSEEK_PROVIDER_ID;
    use codex_protocol::provider_profiles::GLM_PROVIDER_ID;
    use codex_protocol::provider_profiles::MINIMAX_PROVIDER_ID;
    use codex_protocol::provider_profiles::built_in_provider_profile;
    use http::HeaderValue;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    fn provider(base_url: &str, name: &str) -> Provider {
        Provider {
            name: name.to_string(),
            base_url: base_url.to_string(),
            query_params: None,
            headers: HeaderMap::new(),
            retry: RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(10),
                retry_429: false,
                retry_5xx: true,
                retry_transport: true,
            },
            stream_idle_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn attaches_conversation_and_subagent_headers() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hi".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];
        let req = ChatRequestBuilder::new("gpt-test", "inst", &prompt_input, &[])
            .conversation_id(Some("conv-1".into()))
            .session_source(Some(SessionSource::SubAgent(SubAgentSource::Review)))
            .build(&provider("https://api.openai.com/v1", "OpenAI"))
            .expect("request");

        assert_eq!(
            req.headers.get("session_id"),
            Some(&HeaderValue::from_static("conv-1"))
        );
        assert_eq!(
            req.headers.get("x-openai-subagent"),
            Some(&HeaderValue::from_static("review"))
        );
    }

    #[test]
    fn groups_consecutive_tool_calls_into_a_single_assistant_message() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "read these".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "read_file".to_string(),
                namespace: None,
                arguments: r#"{"path":"a.txt"}"#.to_string(),
                call_id: "call-a".to_string(),
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "read_file".to_string(),
                namespace: None,
                arguments: r#"{"path":"b.txt"}"#.to_string(),
                call_id: "call-b".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-a".to_string(),
                output: FunctionCallOutputPayload::from_text("A".to_string()),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-b".to_string(),
                output: FunctionCallOutputPayload::from_text("B".to_string()),
            },
        ];

        let req = ChatRequestBuilder::new("gpt-test", "inst", &prompt_input, &[])
            .build(&provider("https://api.openai.com/v1", "OpenAI"))
            .expect("request");

        let messages = req
            .body
            .get("messages")
            .and_then(Value::as_array)
            .expect("messages array");
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(
            messages[2]["tool_calls"]
                .as_array()
                .expect("tool calls")
                .len(),
            2
        );
    }

    #[test]
    fn deepseek_thinking_alias_adds_extra_body_and_reasoning_content() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "weather".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Reasoning {
                id: String::new(),
                summary: vec![],
                content: Some(vec![
                    codex_protocol::models::ReasoningItemContent::ReasoningText {
                        text: "need tool".to_string(),
                    },
                ]),
                encrypted_content: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "get_weather".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call-weather".to_string(),
            },
        ];

        let req = ChatRequestBuilder::new("deepseek-chat-thinking", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(DEEPSEEK_PROVIDER_ID))
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        assert_eq!(req.body["model"], "deepseek-chat");
        assert_eq!(req.body["thinking"]["type"], "enabled");
        assert_eq!(req.body["stream_options"]["include_usage"], true);
        let assistant = &req.body["messages"].as_array().expect("messages")[2];
        assert_eq!(assistant["reasoning_content"], "need tool");
    }

    #[test]
    fn explicit_provider_profile_overrides_chat_transport_heuristics() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "Follow repo policy.".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];

        let deepseek_profile = built_in_provider_profile(DEEPSEEK_PROVIDER_ID).expect("profile");
        let req = ChatRequestBuilder::new("deepseek-chat-thinking", "inst", &prompt_input, &[])
            .provider_profile(Some(deepseek_profile))
            .build(&provider("https://example.com/v1", "Compatible Gateway"))
            .expect("request");

        assert_eq!(req.body["model"], "deepseek-chat");
        assert_eq!(req.body["thinking"]["type"], "enabled");
        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages[1]["role"], "system");
    }

    #[test]
    fn glm_requests_stream_usage() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];

        let req = ChatRequestBuilder::new("glm-5", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(GLM_PROVIDER_ID))
            .build(&provider(
                "https://open.bigmodel.cn/api/paas/v4",
                "GLM (Zhipu AI)",
            ))
            .expect("request");

        assert_eq!(req.body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn deepseek_uses_json_object_response_format_for_output_schema() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];
        let output_schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });

        let req = ChatRequestBuilder::new("deepseek-chat", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(DEEPSEEK_PROVIDER_ID))
            .output_schema(Some(&output_schema))
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        assert_eq!(req.body["response_format"]["type"], "json_object");
    }

    #[test]
    fn glm_uses_reasoning_content_for_prior_reasoning() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Reasoning {
                id: String::new(),
                summary: vec![],
                content: Some(vec![
                    codex_protocol::models::ReasoningItemContent::ReasoningText {
                        text: "inspect context".to_string(),
                    },
                ]),
                encrypted_content: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "read_file".to_string(),
                namespace: None,
                arguments: r#"{"path":"README.md"}"#.to_string(),
                call_id: "call-readme".to_string(),
            },
        ];

        let req = ChatRequestBuilder::new("glm-5", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(GLM_PROVIDER_ID))
            .build(&provider(
                "https://open.bigmodel.cn/api/paas/v4",
                "GLM (Zhipu AI)",
            ))
            .expect("request");

        let assistant = &req.body["messages"].as_array().expect("messages")[2];
        assert_eq!(assistant["reasoning_content"], "inspect context");
        assert_eq!(assistant.get("reasoning"), None);
    }

    #[test]
    fn deepseek_maps_developer_messages_to_system() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "Follow repo policy.".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];

        let req = ChatRequestBuilder::new("deepseek-chat", "inst", &prompt_input, &[])
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages[1]["role"], "system");
        assert_eq!(messages[1]["content"], "Follow repo policy.");
    }

    #[test]
    fn openai_chat_preserves_developer_messages() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "Follow repo policy.".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];

        let req = ChatRequestBuilder::new("gpt-test", "inst", &prompt_input, &[])
            .build(&provider("https://api.openai.com/v1", "OpenAI"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages[1]["role"], "developer");
        assert_eq!(messages[1]["content"], "Follow repo policy.");
        assert_eq!(req.body.get("stream_options"), None);
    }

    #[test]
    fn minimax_requests_reasoning_split() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];

        let req = ChatRequestBuilder::new("MiniMax-M2.7", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(MINIMAX_PROVIDER_ID))
            .build(&provider("https://api.minimaxi.com/v1", "MiniMax"))
            .expect("request");

        assert_eq!(req.body["reasoning_split"], true);
    }

    #[test]
    fn minimax_keeps_output_schema_local_without_response_format() {
        let prompt_input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];
        let output_schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        });

        let req = ChatRequestBuilder::new("MiniMax-M2.7", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(MINIMAX_PROVIDER_ID))
            .output_schema(Some(&output_schema))
            .build(&provider("https://api.minimaxi.com/v1", "MiniMax"))
            .expect("request");

        assert_eq!(req.body.get("response_format"), None);
    }

    #[test]
    fn minimax_merges_adjacent_system_messages() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "Follow repo policy.".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];

        let req = ChatRequestBuilder::new("MiniMax-M2.7", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(MINIMAX_PROVIDER_ID))
            .build(&provider("https://api.minimaxi.com/v1", "MiniMax"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "inst\n\nFollow repo policy.");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn minimax_hoists_late_system_messages_to_front() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "first user".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "system".to_string(),
                content: vec![ContentItem::InputText {
                    text: "late system".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "second user".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ];

        let req = ChatRequestBuilder::new("MiniMax-M2.7", "inst", &prompt_input, &[])
            .provider_profile(built_in_provider_profile(MINIMAX_PROVIDER_ID))
            .build(&provider("https://api.minimaxi.com/v1", "MiniMax"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[0]["content"], "inst\n\nlate system");
    }

    #[test]
    fn local_shell_calls_are_reencoded_as_function_tool_calls() {
        let prompt_input = vec![ResponseItem::LocalShellCall {
            id: Some("shell-1".to_string()),
            call_id: None,
            status: LocalShellStatus::Completed,
            action: LocalShellAction::Exec(LocalShellExecAction {
                command: vec!["pwd".to_string()],
                timeout_ms: None,
                working_directory: None,
                env: None,
                user: None,
            }),
        }];

        let req = ChatRequestBuilder::new("gpt-test", "inst", &prompt_input, &[])
            .build(&provider("https://api.openai.com/v1", "OpenAI"))
            .expect("request");

        let assistant = &req.body["messages"].as_array().expect("messages")[1];
        assert_eq!(assistant["tool_calls"][0]["type"], "function");
        assert_eq!(
            assistant["tool_calls"][0]["function"]["name"],
            "local_shell_call"
        );
    }

    #[test]
    fn local_shell_calls_prefer_call_id_for_tool_messages() {
        let prompt_input = vec![
            ResponseItem::LocalShellCall {
                id: Some("shell-item-1".to_string()),
                call_id: Some("shell-call-1".to_string()),
                status: LocalShellStatus::Completed,
                action: LocalShellAction::Exec(LocalShellExecAction {
                    command: vec!["pwd".to_string()],
                    timeout_ms: None,
                    working_directory: None,
                    env: None,
                    user: None,
                }),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "shell-call-1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ];

        let req = ChatRequestBuilder::new("deepseek-chat", "inst", &prompt_input, &[])
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "shell-call-1");
        assert_eq!(messages[2]["tool_call_id"], "shell-call-1");
    }

    #[test]
    fn custom_tool_calls_use_call_id_for_tool_messages() {
        let prompt_input = vec![
            ResponseItem::CustomToolCall {
                id: Some("custom-item-1".to_string()),
                status: Some("completed".to_string()),
                call_id: "custom-call-1".to_string(),
                name: "custom_tool".to_string(),
                input: "{}".to_string(),
            },
            ResponseItem::CustomToolCallOutput {
                call_id: "custom-call-1".to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ];

        let req = ChatRequestBuilder::new("deepseek-chat", "inst", &prompt_input, &[])
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "custom-call-1");
        assert_eq!(messages[2]["tool_call_id"], "custom-call-1");
    }

    #[test]
    fn assistant_text_after_tool_call_merges_into_tool_call_message() {
        let prompt_input = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "scan".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                name: "exec_command".to_string(),
                namespace: None,
                arguments: r#"{"cmd":"pwd"}"#.to_string(),
                call_id: "call-exec".to_string(),
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "我来扫描这个项目。".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-exec".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ];

        let req = ChatRequestBuilder::new("deepseek-chat", "inst", &prompt_input, &[])
            .build(&provider("https://api.deepseek.com", "DeepSeek"))
            .expect("request");

        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "我来扫描这个项目。");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call-exec");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call-exec");
    }
}
