#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatReasoningFormat {
    Reasoning,
    ReasoningContent,
    ReasoningDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatModelRewrite {
    Identity,
    DeepSeekThinking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsReplTransport {
    FreeformNative,
    FunctionWrapper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredOutputStrategy {
    NativeJsonSchema,
    JsonObjectPlusLocalValidation,
    PlainTextPlusLocalValidation,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatRequestDialect {
    pub model_rewrite: ChatModelRewrite,
    pub reasoning_format: Option<ChatReasoningFormat>,
    pub supports_developer_role: bool,
    pub merges_system_messages: bool,
    pub hoists_system_messages_to_front: bool,
    pub request_stream_usage: bool,
    pub structured_output_strategy: StructuredOutputStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStreamDialect {
    pub reasoning_format: Option<ChatReasoningFormat>,
    pub parse_root_usage: bool,
    pub parse_choice_usage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolTransportPolicy {
    pub emulate_live_web_search: bool,
    pub js_repl_transport: JsReplTransport,
}
