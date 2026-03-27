use crate::config_types::ReasoningSummary;
use crate::openai_models::ApplyPatchToolType;
use crate::openai_models::ConfigShellToolType;
use crate::openai_models::InputModality;
use crate::openai_models::ModelInfo;
use crate::openai_models::ModelVisibility;
use crate::openai_models::TruncationPolicyConfig;
use crate::openai_models::WebSearchToolType;
use crate::provider_profiles::DEEPSEEK_PROVIDER_ID;
use crate::provider_profiles::GLM_PROVIDER_ID;
use crate::provider_profiles::KIMI_PROVIDER_ID;
use crate::provider_profiles::MINIMAX_PROVIDER_ID;

const TEXT_INPUT_MODALITIES: &[InputModality] = &[InputModality::Text];
const MULTIMODAL_INPUT_MODALITIES: &[InputModality] = &[InputModality::Text, InputModality::Image];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledModelCatalogEntry {
    pub slug: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub priority: i32,
    pub context_window: i64,
    pub input_modalities: &'static [InputModality],
    pub supports_parallel_tool_calls: bool,
    pub supports_search_tool: bool,
    pub web_search_tool_type: WebSearchToolType,
}

impl BundledModelCatalogEntry {
    pub fn to_model_info(self, base_instructions: &str) -> ModelInfo {
        let input_modalities = self.input_modalities.to_vec();
        ModelInfo {
            slug: self.slug.to_string(),
            display_name: self.display_name.to_string(),
            description: Some(self.description.to_string()),
            default_reasoning_level: None,
            supported_reasoning_levels: vec![],
            shell_type: ConfigShellToolType::ShellCommand,
            visibility: ModelVisibility::List,
            supported_in_api: true,
            priority: self.priority,
            availability_nux: None,
            upgrade: None,
            base_instructions: base_instructions.to_string(),
            model_messages: None,
            supports_reasoning_summaries: false,
            default_reasoning_summary: ReasoningSummary::Auto,
            support_verbosity: false,
            default_verbosity: None,
            apply_patch_tool_type: Some(ApplyPatchToolType::Function),
            web_search_tool_type: self.web_search_tool_type,
            truncation_policy: TruncationPolicyConfig::tokens(10_000),
            supports_parallel_tool_calls: self.supports_parallel_tool_calls,
            supports_image_detail_original: input_modalities.contains(&InputModality::Image),
            context_window: Some(self.context_window),
            auto_compact_token_limit: None,
            effective_context_window_percent: 90,
            experimental_supported_tools: vec![],
            input_modalities,
            used_fallback_model_metadata: false,
            supports_search_tool: self.supports_search_tool,
        }
    }
}

pub const DEEPSEEK_CATALOG: &[BundledModelCatalogEntry] = &[
    BundledModelCatalogEntry {
        slug: "deepseek-chat",
        display_name: "DeepSeek Chat",
        description: "DeepSeek-V3.2 non-thinking mode with tool calling and 128K context.",
        priority: 0,
        context_window: 128_000,
        input_modalities: TEXT_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
    BundledModelCatalogEntry {
        slug: "deepseek-reasoner",
        display_name: "DeepSeek Reasoner",
        description: "DeepSeek-V3.2 official thinking-mode alias with tool calling and 128K context.",
        priority: 1,
        context_window: 128_000,
        input_modalities: TEXT_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
];

pub const GLM_CATALOG: &[BundledModelCatalogEntry] = &[
    BundledModelCatalogEntry {
        slug: "glm-5",
        display_name: "GLM-5",
        description: "Zhipu AI flagship text model for coding, planning, and agent workflows with 200K context.",
        priority: 0,
        context_window: 200_000,
        input_modalities: TEXT_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
    BundledModelCatalogEntry {
        slug: "glm-4.6v",
        display_name: "GLM-4.6V",
        description: "Zhipu AI flagship visual reasoning model with tool calling and 128K context.",
        priority: 1,
        context_window: 128_000,
        input_modalities: MULTIMODAL_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
];

pub const KIMI_CATALOG: &[BundledModelCatalogEntry] = &[BundledModelCatalogEntry {
    slug: "kimi-k2.5",
    display_name: "Kimi K2.5",
    description: "Moonshot AI K2.5 multimodal coding model for agent workflows, skills, and subagents through the Kimi API.",
    priority: 0,
    context_window: 262_144,
    input_modalities: MULTIMODAL_INPUT_MODALITIES,
    supports_parallel_tool_calls: false,
    supports_search_tool: false,
    web_search_tool_type: WebSearchToolType::Text,
}];

pub const MINIMAX_CATALOG: &[BundledModelCatalogEntry] = &[
    BundledModelCatalogEntry {
        slug: "MiniMax-M2.7",
        display_name: "MiniMax-M2.7",
        description: "MiniMax flagship reasoning and coding model with 204,800-token context.",
        priority: 0,
        context_window: 204_800,
        input_modalities: TEXT_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
    BundledModelCatalogEntry {
        slug: "MiniMax-M2.7-highspeed",
        display_name: "MiniMax-M2.7 Highspeed",
        description: "MiniMax-M2.7 high-speed variant with the same 204,800-token context.",
        priority: 1,
        context_window: 204_800,
        input_modalities: TEXT_INPUT_MODALITIES,
        supports_parallel_tool_calls: false,
        supports_search_tool: false,
        web_search_tool_type: WebSearchToolType::Text,
    },
];

pub fn bundled_model_catalog(provider_id: &str) -> Option<&'static [BundledModelCatalogEntry]> {
    match provider_id {
        DEEPSEEK_PROVIDER_ID => Some(DEEPSEEK_CATALOG),
        GLM_PROVIDER_ID => Some(GLM_CATALOG),
        KIMI_PROVIDER_ID => Some(KIMI_CATALOG),
        MINIMAX_PROVIDER_ID => Some(MINIMAX_CATALOG),
        _ => None,
    }
}

pub fn resolve_bundled_model_lookup_alias(provider_id: &str, model: &str) -> Option<String> {
    match provider_id {
        DEEPSEEK_PROVIDER_ID => resolve_deepseek_lookup_alias(model),
        KIMI_PROVIDER_ID => match model {
            "moonshot/kimi-k2.5" => Some("kimi-k2.5".to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn resolve_deepseek_lookup_alias(model: &str) -> Option<String> {
    if let Some(alias) = deepseek_alias_suffix(model) {
        return Some(alias.to_string());
    }

    let (namespace, suffix) = model.split_once('/')?;
    if suffix.contains('/') {
        return None;
    }

    deepseek_alias_suffix(suffix).map(|alias| format!("{namespace}/{alias}"))
}

fn deepseek_alias_suffix(slug: &str) -> Option<&'static str> {
    match slug {
        "deepseek-chat-thinking" | "deepseek-thinking" => Some("deepseek-reasoner"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn deepseek_lookup_aliases_stay_provider_local() {
        assert_eq!(
            resolve_bundled_model_lookup_alias(DEEPSEEK_PROVIDER_ID, "deepseek-thinking"),
            Some("deepseek-reasoner".to_string())
        );
        assert_eq!(
            resolve_bundled_model_lookup_alias(
                DEEPSEEK_PROVIDER_ID,
                "vendor/deepseek-chat-thinking"
            ),
            Some("vendor/deepseek-reasoner".to_string())
        );
        assert_eq!(
            resolve_bundled_model_lookup_alias(GLM_PROVIDER_ID, "deepseek-thinking"),
            None
        );
    }

    #[test]
    fn kimi_catalog_exposes_k25_multimodal_model() {
        let model = KIMI_CATALOG[0].to_model_info("base instructions");
        assert_eq!(model.slug, "kimi-k2.5");
        assert_eq!(model.context_window, Some(262_144));
        assert_eq!(model.input_modalities, MULTIMODAL_INPUT_MODALITIES.to_vec());
        assert!(model.supports_image_detail_original);
    }
}
