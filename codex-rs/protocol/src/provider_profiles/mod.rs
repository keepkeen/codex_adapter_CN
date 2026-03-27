mod catalog;
mod dialects;
mod features;

use self::catalog::BundledModelCatalogEntry;

pub use self::catalog::bundled_model_catalog;
pub use self::catalog::resolve_bundled_model_lookup_alias;
pub use self::dialects::AssistantToolCallReasoningPolicy;
pub use self::dialects::ChatModelRewrite;
pub use self::dialects::ChatReasoningFormat;
pub use self::dialects::ChatRequestDialect;
pub use self::dialects::ChatStreamDialect;
pub use self::dialects::JsReplTransport;
pub use self::dialects::StructuredOutputStrategy;
pub use self::dialects::ToolTransportPolicy;
pub use self::features::FeatureSupport;
pub use self::features::ProviderFeature;
pub use self::features::ProviderFeatureLiveCoverage;
pub use self::features::ProviderFeatureNotes;
pub use self::features::ProviderFeatureSpec;
pub use self::features::ProviderFeatureSupport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInProviderDescriptor {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub default_base_url: &'static str,
    pub default_env_key: Option<&'static str>,
    pub uses_chat_completions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInProviderProfile {
    pub descriptor: BuiltInProviderDescriptor,
    pub request_dialect: ChatRequestDialect,
    pub stream_dialect: ChatStreamDialect,
    pub tool_transport: ToolTransportPolicy,
    pub features: ProviderFeatureSupport,
    pub bundled_model_catalog: &'static [BundledModelCatalogEntry],
}

impl BuiltInProviderProfile {
    pub fn support_for(&self, feature: ProviderFeature) -> FeatureSupport {
        self.features.support_for(feature)
    }

    pub fn supports(&self, feature: ProviderFeature) -> bool {
        self.support_for(feature) != FeatureSupport::Unsupported
    }

    pub fn compatibility_notes(&self) -> ProviderFeatureNotes {
        self.features.compatibility_notes()
    }
}

pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
pub const GLM_PROVIDER_ID: &str = "glm";
pub const KIMI_PROVIDER_ID: &str = "kimi";
pub const MINIMAX_PROVIDER_ID: &str = "minimax";

const OPENAI_PROFILE: BuiltInProviderProfile = BuiltInProviderProfile {
    descriptor: BuiltInProviderDescriptor {
        provider_id: OPENAI_PROVIDER_ID,
        display_name: "OpenAI",
        default_base_url: "https://api.openai.com/v1",
        default_env_key: None,
        uses_chat_completions: false,
    },
    request_dialect: ChatRequestDialect {
        model_rewrite: ChatModelRewrite::Identity,
        reasoning_format: Some(ChatReasoningFormat::Reasoning),
        assistant_tool_call_reasoning: AssistantToolCallReasoningPolicy::OmitWhenMissing,
        supports_developer_role: true,
        merges_system_messages: false,
        hoists_system_messages_to_front: false,
        request_stream_usage: false,
        structured_output_strategy: StructuredOutputStrategy::NativeJsonSchema,
    },
    stream_dialect: ChatStreamDialect {
        reasoning_format: Some(ChatReasoningFormat::Reasoning),
        parse_root_usage: true,
        parse_choice_usage: true,
    },
    tool_transport: ToolTransportPolicy {
        emulate_live_web_search: false,
        js_repl_transport: JsReplTransport::FreeformNative,
    },
    features: ProviderFeatureSupport {
        function_tools: FeatureSupport::Native,
        mcp: FeatureSupport::Native,
        skills: FeatureSupport::Native,
        subagents: FeatureSupport::Native,
        web_search: FeatureSupport::Native,
        image_generation: FeatureSupport::Native,
        artifacts: FeatureSupport::Native,
        js_repl: FeatureSupport::Native,
        output_schema: FeatureSupport::Native,
        resume: FeatureSupport::Native,
        manual_compact: FeatureSupport::Native,
        auto_compact: FeatureSupport::Native,
        token_usage: FeatureSupport::Native,
        image_input: FeatureSupport::Native,
    },
    bundled_model_catalog: &[],
};

const DEEPSEEK_PROFILE: BuiltInProviderProfile = BuiltInProviderProfile {
    descriptor: BuiltInProviderDescriptor {
        provider_id: DEEPSEEK_PROVIDER_ID,
        display_name: "DeepSeek",
        default_base_url: "https://api.deepseek.com",
        default_env_key: Some("DEEPSEEK_API_KEY"),
        uses_chat_completions: true,
    },
    request_dialect: ChatRequestDialect {
        model_rewrite: ChatModelRewrite::DeepSeekThinking,
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        assistant_tool_call_reasoning: AssistantToolCallReasoningPolicy::OmitWhenMissing,
        supports_developer_role: false,
        merges_system_messages: false,
        hoists_system_messages_to_front: false,
        request_stream_usage: true,
        structured_output_strategy: StructuredOutputStrategy::JsonObjectPlusLocalValidation,
    },
    stream_dialect: ChatStreamDialect {
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        parse_root_usage: true,
        parse_choice_usage: true,
    },
    tool_transport: ToolTransportPolicy {
        emulate_live_web_search: true,
        js_repl_transport: JsReplTransport::FunctionWrapper,
    },
    features: ProviderFeatureSupport {
        function_tools: FeatureSupport::Native,
        mcp: FeatureSupport::Emulated,
        skills: FeatureSupport::Emulated,
        subagents: FeatureSupport::Emulated,
        web_search: FeatureSupport::Emulated,
        image_generation: FeatureSupport::Unsupported,
        artifacts: FeatureSupport::Unsupported,
        js_repl: FeatureSupport::Emulated,
        output_schema: FeatureSupport::Emulated,
        resume: FeatureSupport::Emulated,
        manual_compact: FeatureSupport::Emulated,
        auto_compact: FeatureSupport::Emulated,
        token_usage: FeatureSupport::Emulated,
        image_input: FeatureSupport::Unsupported,
    },
    bundled_model_catalog: catalog::DEEPSEEK_CATALOG,
};

const GLM_PROFILE: BuiltInProviderProfile = BuiltInProviderProfile {
    descriptor: BuiltInProviderDescriptor {
        provider_id: GLM_PROVIDER_ID,
        display_name: "GLM (Zhipu AI)",
        default_base_url: "https://open.bigmodel.cn/api/paas/v4",
        default_env_key: Some("GLM_API_KEY"),
        uses_chat_completions: true,
    },
    request_dialect: ChatRequestDialect {
        model_rewrite: ChatModelRewrite::Identity,
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        assistant_tool_call_reasoning: AssistantToolCallReasoningPolicy::OmitWhenMissing,
        supports_developer_role: false,
        merges_system_messages: false,
        hoists_system_messages_to_front: false,
        request_stream_usage: true,
        structured_output_strategy: StructuredOutputStrategy::JsonObjectPlusLocalValidation,
    },
    stream_dialect: ChatStreamDialect {
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        parse_root_usage: true,
        parse_choice_usage: true,
    },
    tool_transport: ToolTransportPolicy {
        emulate_live_web_search: true,
        js_repl_transport: JsReplTransport::FunctionWrapper,
    },
    features: ProviderFeatureSupport {
        function_tools: FeatureSupport::Native,
        mcp: FeatureSupport::Emulated,
        skills: FeatureSupport::Emulated,
        subagents: FeatureSupport::Emulated,
        web_search: FeatureSupport::Emulated,
        image_generation: FeatureSupport::Unsupported,
        artifacts: FeatureSupport::Unsupported,
        js_repl: FeatureSupport::Emulated,
        output_schema: FeatureSupport::Emulated,
        resume: FeatureSupport::Emulated,
        manual_compact: FeatureSupport::Emulated,
        auto_compact: FeatureSupport::Emulated,
        token_usage: FeatureSupport::Emulated,
        image_input: FeatureSupport::Native,
    },
    bundled_model_catalog: catalog::GLM_CATALOG,
};

const KIMI_PROFILE: BuiltInProviderProfile = BuiltInProviderProfile {
    descriptor: BuiltInProviderDescriptor {
        provider_id: KIMI_PROVIDER_ID,
        display_name: "Kimi (Moonshot AI)",
        default_base_url: "https://api.moonshot.cn/v1",
        default_env_key: Some("MOONSHOT_API_KEY"),
        uses_chat_completions: true,
    },
    request_dialect: ChatRequestDialect {
        model_rewrite: ChatModelRewrite::Identity,
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        assistant_tool_call_reasoning: AssistantToolCallReasoningPolicy::EmitEmptyWhenMissing,
        supports_developer_role: false,
        merges_system_messages: false,
        hoists_system_messages_to_front: false,
        request_stream_usage: true,
        structured_output_strategy: StructuredOutputStrategy::JsonObjectPlusLocalValidation,
    },
    stream_dialect: ChatStreamDialect {
        reasoning_format: Some(ChatReasoningFormat::ReasoningContent),
        parse_root_usage: true,
        parse_choice_usage: true,
    },
    tool_transport: ToolTransportPolicy {
        emulate_live_web_search: true,
        js_repl_transport: JsReplTransport::FunctionWrapper,
    },
    features: ProviderFeatureSupport {
        function_tools: FeatureSupport::Native,
        mcp: FeatureSupport::Emulated,
        skills: FeatureSupport::Emulated,
        subagents: FeatureSupport::Emulated,
        web_search: FeatureSupport::Emulated,
        image_generation: FeatureSupport::Unsupported,
        artifacts: FeatureSupport::Unsupported,
        js_repl: FeatureSupport::Emulated,
        output_schema: FeatureSupport::Emulated,
        resume: FeatureSupport::Emulated,
        manual_compact: FeatureSupport::Emulated,
        auto_compact: FeatureSupport::Emulated,
        token_usage: FeatureSupport::Emulated,
        image_input: FeatureSupport::Native,
    },
    bundled_model_catalog: catalog::KIMI_CATALOG,
};

const MINIMAX_PROFILE: BuiltInProviderProfile = BuiltInProviderProfile {
    descriptor: BuiltInProviderDescriptor {
        provider_id: MINIMAX_PROVIDER_ID,
        display_name: "MiniMax",
        default_base_url: "https://api.minimaxi.com/v1",
        default_env_key: Some("MINIMAX_API_KEY"),
        uses_chat_completions: true,
    },
    request_dialect: ChatRequestDialect {
        model_rewrite: ChatModelRewrite::Identity,
        reasoning_format: Some(ChatReasoningFormat::ReasoningDetails),
        assistant_tool_call_reasoning: AssistantToolCallReasoningPolicy::OmitWhenMissing,
        supports_developer_role: false,
        merges_system_messages: true,
        hoists_system_messages_to_front: true,
        request_stream_usage: true,
        structured_output_strategy: StructuredOutputStrategy::PlainTextPlusLocalValidation,
    },
    stream_dialect: ChatStreamDialect {
        reasoning_format: Some(ChatReasoningFormat::ReasoningDetails),
        parse_root_usage: true,
        parse_choice_usage: true,
    },
    tool_transport: ToolTransportPolicy {
        emulate_live_web_search: true,
        js_repl_transport: JsReplTransport::FunctionWrapper,
    },
    features: ProviderFeatureSupport {
        function_tools: FeatureSupport::Native,
        mcp: FeatureSupport::Emulated,
        skills: FeatureSupport::Emulated,
        subagents: FeatureSupport::Emulated,
        web_search: FeatureSupport::Emulated,
        image_generation: FeatureSupport::Unsupported,
        artifacts: FeatureSupport::Unsupported,
        js_repl: FeatureSupport::Emulated,
        output_schema: FeatureSupport::Emulated,
        resume: FeatureSupport::Emulated,
        manual_compact: FeatureSupport::Emulated,
        auto_compact: FeatureSupport::Emulated,
        token_usage: FeatureSupport::Emulated,
        image_input: FeatureSupport::Unsupported,
    },
    bundled_model_catalog: catalog::MINIMAX_CATALOG,
};

const BUILT_IN_PROVIDER_PROFILES: &[BuiltInProviderProfile] = &[
    OPENAI_PROFILE,
    DEEPSEEK_PROFILE,
    GLM_PROFILE,
    KIMI_PROFILE,
    MINIMAX_PROFILE,
];

pub fn built_in_provider_profile(provider_id: &str) -> Option<&'static BuiltInProviderProfile> {
    BUILT_IN_PROVIDER_PROFILES
        .iter()
        .find(|profile| profile.descriptor.provider_id == provider_id)
}

pub fn built_in_provider_profiles() -> &'static [BuiltInProviderProfile] {
    BUILT_IN_PROVIDER_PROFILES
}

pub fn infer_built_in_provider_id(
    name: &str,
    base_url: Option<&str>,
    env_key: Option<&str>,
    uses_chat_completions: bool,
) -> Option<&'static str> {
    built_in_provider_profiles()
        .iter()
        .find(|profile| {
            let descriptor = profile.descriptor;
            descriptor.display_name == name
                && Some(descriptor.default_base_url) == base_url
                && descriptor.default_env_key == env_key
                && descriptor.uses_chat_completions == uses_chat_completions
        })
        .map(|profile| profile.descriptor.provider_id)
}

pub fn infer_built_in_profile_for_chat_transport(
    name: &str,
    base_url: Option<&str>,
) -> Option<&'static BuiltInProviderProfile> {
    built_in_provider_profiles().iter().find(|profile| {
        let descriptor = profile.descriptor;
        descriptor.uses_chat_completions
            && descriptor.display_name == name
            && Some(descriptor.default_base_url) == base_url
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn infers_built_in_provider_ids_from_exact_descriptors() {
        assert_eq!(
            infer_built_in_provider_id(
                "DeepSeek",
                Some("https://api.deepseek.com"),
                Some("DEEPSEEK_API_KEY"),
                true,
            ),
            Some(DEEPSEEK_PROVIDER_ID)
        );
        assert_eq!(
            infer_built_in_provider_id(
                "Kimi (Moonshot AI)",
                Some("https://api.moonshot.cn/v1"),
                Some("MOONSHOT_API_KEY"),
                true,
            ),
            Some(KIMI_PROVIDER_ID)
        );
        assert_eq!(
            infer_built_in_provider_id("OpenAI", Some("https://api.openai.com/v1"), None, false,),
            Some(OPENAI_PROVIDER_ID)
        );
    }

    #[test]
    fn chat_transport_lookup_skips_non_matching_names() {
        assert_eq!(
            infer_built_in_profile_for_chat_transport(
                "OpenAI compatible",
                Some("https://api.deepseek.com"),
            ),
            None
        );
    }

    #[test]
    fn profiles_expose_expected_feature_surface() {
        let kimi = built_in_provider_profile(KIMI_PROVIDER_ID).expect("kimi profile");
        assert_eq!(
            kimi.features.support_for(ProviderFeature::WebSearch),
            FeatureSupport::Emulated
        );
        assert_eq!(
            kimi.features.support_for(ProviderFeature::ImageInput),
            FeatureSupport::Native
        );

        let openai = built_in_provider_profile(OPENAI_PROVIDER_ID).expect("openai profile");
        assert_eq!(
            openai.features.support_for(ProviderFeature::OutputSchema),
            FeatureSupport::Native
        );
        assert_eq!(openai.tool_transport.emulate_live_web_search, false);
    }

    #[test]
    fn deepseek_compatibility_notes_match_profile_matrix() {
        let deepseek = built_in_provider_profile(DEEPSEEK_PROVIDER_ID).expect("deepseek profile");
        let notes = deepseek.compatibility_notes();

        assert_eq!(
            notes.emulated,
            vec![
                ProviderFeature::Mcp,
                ProviderFeature::Skills,
                ProviderFeature::Subagents,
                ProviderFeature::WebSearch,
                ProviderFeature::JsRepl,
                ProviderFeature::OutputSchema,
                ProviderFeature::Resume,
                ProviderFeature::ManualCompact,
                ProviderFeature::AutoCompact,
                ProviderFeature::TokenUsage,
            ]
        );
        assert_eq!(
            notes.unsupported,
            vec![
                ProviderFeature::ImageGeneration,
                ProviderFeature::Artifacts,
                ProviderFeature::ImageInput,
            ]
        );
    }

    #[test]
    fn chat_provider_profiles_mark_js_repl_and_output_schema_as_emulated() {
        let emulated_features = [ProviderFeature::JsRepl, ProviderFeature::OutputSchema];

        for provider_id in [
            DEEPSEEK_PROVIDER_ID,
            GLM_PROVIDER_ID,
            KIMI_PROVIDER_ID,
            MINIMAX_PROVIDER_ID,
        ] {
            let profile = built_in_provider_profile(provider_id).expect("chat profile");
            for feature in emulated_features {
                assert_eq!(profile.support_for(feature), FeatureSupport::Emulated);
                assert!(profile.supports(feature));
            }
        }
    }

    #[test]
    fn openai_profile_keeps_secondary_features_native() {
        let openai = built_in_provider_profile(OPENAI_PROVIDER_ID).expect("openai profile");

        for feature in [
            ProviderFeature::ImageGeneration,
            ProviderFeature::Artifacts,
            ProviderFeature::JsRepl,
            ProviderFeature::OutputSchema,
        ] {
            assert_eq!(openai.support_for(feature), FeatureSupport::Native);
            assert!(openai.supports(feature));
        }
    }

    #[test]
    fn built_in_profiles_match_expected_feature_support_matrix() {
        let matrix = built_in_provider_profiles()
            .iter()
            .map(|profile| {
                (
                    profile.descriptor.provider_id,
                    ProviderFeature::ALL
                        .into_iter()
                        .map(|feature| (feature, profile.support_for(feature)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            matrix,
            vec![
                (
                    OPENAI_PROVIDER_ID,
                    vec![
                        (ProviderFeature::FunctionTools, FeatureSupport::Native),
                        (ProviderFeature::Mcp, FeatureSupport::Native),
                        (ProviderFeature::Skills, FeatureSupport::Native),
                        (ProviderFeature::Subagents, FeatureSupport::Native),
                        (ProviderFeature::WebSearch, FeatureSupport::Native),
                        (ProviderFeature::ImageGeneration, FeatureSupport::Native),
                        (ProviderFeature::Artifacts, FeatureSupport::Native),
                        (ProviderFeature::JsRepl, FeatureSupport::Native),
                        (ProviderFeature::OutputSchema, FeatureSupport::Native),
                        (ProviderFeature::Resume, FeatureSupport::Native),
                        (ProviderFeature::ManualCompact, FeatureSupport::Native),
                        (ProviderFeature::AutoCompact, FeatureSupport::Native),
                        (ProviderFeature::TokenUsage, FeatureSupport::Native),
                        (ProviderFeature::ImageInput, FeatureSupport::Native),
                    ],
                ),
                (
                    DEEPSEEK_PROVIDER_ID,
                    vec![
                        (ProviderFeature::FunctionTools, FeatureSupport::Native),
                        (ProviderFeature::Mcp, FeatureSupport::Emulated),
                        (ProviderFeature::Skills, FeatureSupport::Emulated),
                        (ProviderFeature::Subagents, FeatureSupport::Emulated),
                        (ProviderFeature::WebSearch, FeatureSupport::Emulated),
                        (
                            ProviderFeature::ImageGeneration,
                            FeatureSupport::Unsupported
                        ),
                        (ProviderFeature::Artifacts, FeatureSupport::Unsupported),
                        (ProviderFeature::JsRepl, FeatureSupport::Emulated),
                        (ProviderFeature::OutputSchema, FeatureSupport::Emulated),
                        (ProviderFeature::Resume, FeatureSupport::Emulated),
                        (ProviderFeature::ManualCompact, FeatureSupport::Emulated),
                        (ProviderFeature::AutoCompact, FeatureSupport::Emulated),
                        (ProviderFeature::TokenUsage, FeatureSupport::Emulated),
                        (ProviderFeature::ImageInput, FeatureSupport::Unsupported),
                    ],
                ),
                (
                    GLM_PROVIDER_ID,
                    vec![
                        (ProviderFeature::FunctionTools, FeatureSupport::Native),
                        (ProviderFeature::Mcp, FeatureSupport::Emulated),
                        (ProviderFeature::Skills, FeatureSupport::Emulated),
                        (ProviderFeature::Subagents, FeatureSupport::Emulated),
                        (ProviderFeature::WebSearch, FeatureSupport::Emulated),
                        (
                            ProviderFeature::ImageGeneration,
                            FeatureSupport::Unsupported
                        ),
                        (ProviderFeature::Artifacts, FeatureSupport::Unsupported),
                        (ProviderFeature::JsRepl, FeatureSupport::Emulated),
                        (ProviderFeature::OutputSchema, FeatureSupport::Emulated),
                        (ProviderFeature::Resume, FeatureSupport::Emulated),
                        (ProviderFeature::ManualCompact, FeatureSupport::Emulated),
                        (ProviderFeature::AutoCompact, FeatureSupport::Emulated),
                        (ProviderFeature::TokenUsage, FeatureSupport::Emulated),
                        (ProviderFeature::ImageInput, FeatureSupport::Native),
                    ],
                ),
                (
                    KIMI_PROVIDER_ID,
                    vec![
                        (ProviderFeature::FunctionTools, FeatureSupport::Native),
                        (ProviderFeature::Mcp, FeatureSupport::Emulated),
                        (ProviderFeature::Skills, FeatureSupport::Emulated),
                        (ProviderFeature::Subagents, FeatureSupport::Emulated),
                        (ProviderFeature::WebSearch, FeatureSupport::Emulated),
                        (
                            ProviderFeature::ImageGeneration,
                            FeatureSupport::Unsupported
                        ),
                        (ProviderFeature::Artifacts, FeatureSupport::Unsupported),
                        (ProviderFeature::JsRepl, FeatureSupport::Emulated),
                        (ProviderFeature::OutputSchema, FeatureSupport::Emulated),
                        (ProviderFeature::Resume, FeatureSupport::Emulated),
                        (ProviderFeature::ManualCompact, FeatureSupport::Emulated),
                        (ProviderFeature::AutoCompact, FeatureSupport::Emulated),
                        (ProviderFeature::TokenUsage, FeatureSupport::Emulated),
                        (ProviderFeature::ImageInput, FeatureSupport::Native),
                    ],
                ),
                (
                    MINIMAX_PROVIDER_ID,
                    vec![
                        (ProviderFeature::FunctionTools, FeatureSupport::Native),
                        (ProviderFeature::Mcp, FeatureSupport::Emulated),
                        (ProviderFeature::Skills, FeatureSupport::Emulated),
                        (ProviderFeature::Subagents, FeatureSupport::Emulated),
                        (ProviderFeature::WebSearch, FeatureSupport::Emulated),
                        (
                            ProviderFeature::ImageGeneration,
                            FeatureSupport::Unsupported
                        ),
                        (ProviderFeature::Artifacts, FeatureSupport::Unsupported),
                        (ProviderFeature::JsRepl, FeatureSupport::Emulated),
                        (ProviderFeature::OutputSchema, FeatureSupport::Emulated),
                        (ProviderFeature::Resume, FeatureSupport::Emulated),
                        (ProviderFeature::ManualCompact, FeatureSupport::Emulated),
                        (ProviderFeature::AutoCompact, FeatureSupport::Emulated),
                        (ProviderFeature::TokenUsage, FeatureSupport::Emulated),
                        (ProviderFeature::ImageInput, FeatureSupport::Unsupported),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn feature_transport_contracts_match_support_matrix() {
        for profile in built_in_provider_profiles() {
            match profile.support_for(ProviderFeature::JsRepl) {
                FeatureSupport::Native => {
                    assert_eq!(
                        profile.tool_transport.js_repl_transport,
                        JsReplTransport::FreeformNative
                    );
                }
                FeatureSupport::Emulated => {
                    assert_eq!(
                        profile.tool_transport.js_repl_transport,
                        JsReplTransport::FunctionWrapper
                    );
                }
                FeatureSupport::Unsupported => {}
            }

            match profile.support_for(ProviderFeature::OutputSchema) {
                FeatureSupport::Native => {
                    assert_eq!(
                        profile.request_dialect.structured_output_strategy,
                        StructuredOutputStrategy::NativeJsonSchema
                    );
                }
                FeatureSupport::Emulated => {
                    assert_ne!(
                        profile.request_dialect.structured_output_strategy,
                        StructuredOutputStrategy::Unsupported
                    );
                    assert_ne!(
                        profile.request_dialect.structured_output_strategy,
                        StructuredOutputStrategy::NativeJsonSchema
                    );
                }
                FeatureSupport::Unsupported => {
                    assert_eq!(
                        profile.request_dialect.structured_output_strategy,
                        StructuredOutputStrategy::Unsupported
                    );
                }
            }

            match profile.support_for(ProviderFeature::WebSearch) {
                FeatureSupport::Native => {
                    assert!(!profile.tool_transport.emulate_live_web_search);
                }
                FeatureSupport::Emulated => {
                    assert!(profile.tool_transport.emulate_live_web_search);
                }
                FeatureSupport::Unsupported => {
                    assert!(!profile.tool_transport.emulate_live_web_search);
                }
            }
        }
    }
}
