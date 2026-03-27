#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSupport {
    Native,
    Emulated,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFeatureLiveCoverage {
    None,
    Exec,
    Tui,
    ExecAndTui,
}

impl ProviderFeatureLiveCoverage {
    pub fn requires_exec_live_smoke(self) -> bool {
        matches!(self, Self::Exec | Self::ExecAndTui)
    }

    pub fn requires_tui_live_smoke(self) -> bool {
        matches!(self, Self::Tui | Self::ExecAndTui)
    }
}

macro_rules! provider_feature_surface {
    ($($variant:ident => $field:ident => $label:literal => $live_coverage:expr),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum ProviderFeature {
            $($variant),+
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct ProviderFeatureSpec {
            pub label: &'static str,
            pub live_coverage: ProviderFeatureLiveCoverage,
        }

        impl ProviderFeature {
            pub const ALL: [Self; provider_feature_surface!(@count $($variant),+)] = [
                $(Self::$variant),+
            ];

            pub fn spec(self) -> ProviderFeatureSpec {
                match self {
                    $(Self::$variant => ProviderFeatureSpec {
                        label: $label,
                        live_coverage: $live_coverage,
                    }),+
                }
            }

            pub fn label(self) -> &'static str {
                self.spec().label
            }

            pub fn live_coverage(self) -> ProviderFeatureLiveCoverage {
                self.spec().live_coverage
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct ProviderFeatureSupport {
            $(pub $field: FeatureSupport),+
        }

        impl ProviderFeatureSupport {
            pub fn support_for(&self, feature: ProviderFeature) -> FeatureSupport {
                match feature {
                    $(ProviderFeature::$variant => self.$field),+
                }
            }

            pub fn compatibility_notes(&self) -> ProviderFeatureNotes {
                let emulated = ProviderFeature::ALL
                    .into_iter()
                    .filter(|feature| self.support_for(*feature) == FeatureSupport::Emulated)
                    .collect();
                let unsupported = ProviderFeature::ALL
                    .into_iter()
                    .filter(|feature| self.support_for(*feature) == FeatureSupport::Unsupported)
                    .collect();

                ProviderFeatureNotes {
                    emulated,
                    unsupported,
                }
            }
        }
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(provider_feature_surface!(@replace $variant ())),+])
    };
    (@replace $_variant:ident $replacement:expr) => {
        $replacement
    };
}

provider_feature_surface! {
    FunctionTools => function_tools => "function tools" => ProviderFeatureLiveCoverage::None,
    Mcp => mcp => "MCP" => ProviderFeatureLiveCoverage::Exec,
    Skills => skills => "skills" => ProviderFeatureLiveCoverage::Exec,
    Subagents => subagents => "subagents" => ProviderFeatureLiveCoverage::Exec,
    WebSearch => web_search => "web search" => ProviderFeatureLiveCoverage::Exec,
    ImageGeneration => image_generation => "image generation" => ProviderFeatureLiveCoverage::None,
    Artifacts => artifacts => "artifacts" => ProviderFeatureLiveCoverage::None,
    JsRepl => js_repl => "js repl" => ProviderFeatureLiveCoverage::Exec,
    OutputSchema => output_schema => "structured output" => ProviderFeatureLiveCoverage::Exec,
    Resume => resume => "resume" => ProviderFeatureLiveCoverage::None,
    ManualCompact => manual_compact => "/compact" => ProviderFeatureLiveCoverage::Tui,
    AutoCompact => auto_compact => "auto compact" => ProviderFeatureLiveCoverage::Exec,
    TokenUsage => token_usage => "token usage" => ProviderFeatureLiveCoverage::None,
    ImageInput => image_input => "image input" => ProviderFeatureLiveCoverage::Exec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFeatureNotes {
    pub emulated: Vec<ProviderFeature>,
    pub unsupported: Vec<ProviderFeature>,
}

impl ProviderFeatureNotes {
    pub fn is_empty(&self) -> bool {
        self.emulated.is_empty() && self.unsupported.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn compatibility_notes_group_non_native_features() {
        let support = ProviderFeatureSupport {
            function_tools: FeatureSupport::Native,
            mcp: FeatureSupport::Emulated,
            skills: FeatureSupport::Native,
            subagents: FeatureSupport::Unsupported,
            web_search: FeatureSupport::Emulated,
            image_generation: FeatureSupport::Unsupported,
            artifacts: FeatureSupport::Unsupported,
            js_repl: FeatureSupport::Unsupported,
            output_schema: FeatureSupport::Unsupported,
            resume: FeatureSupport::Emulated,
            manual_compact: FeatureSupport::Emulated,
            auto_compact: FeatureSupport::Emulated,
            token_usage: FeatureSupport::Emulated,
            image_input: FeatureSupport::Native,
        };

        assert_eq!(
            support.compatibility_notes(),
            ProviderFeatureNotes {
                emulated: vec![
                    ProviderFeature::Mcp,
                    ProviderFeature::WebSearch,
                    ProviderFeature::Resume,
                    ProviderFeature::ManualCompact,
                    ProviderFeature::AutoCompact,
                    ProviderFeature::TokenUsage,
                ],
                unsupported: vec![
                    ProviderFeature::Subagents,
                    ProviderFeature::ImageGeneration,
                    ProviderFeature::Artifacts,
                    ProviderFeature::JsRepl,
                    ProviderFeature::OutputSchema,
                ],
            }
        );
    }

    #[test]
    fn labels_are_stable_for_status_surfaces() {
        let labels = ProviderFeature::ALL.map(ProviderFeature::label);

        assert_eq!(
            labels,
            [
                "function tools",
                "MCP",
                "skills",
                "subagents",
                "web search",
                "image generation",
                "artifacts",
                "js repl",
                "structured output",
                "resume",
                "/compact",
                "auto compact",
                "token usage",
                "image input",
            ]
        );
    }

    #[test]
    fn live_coverage_contracts_are_stable() {
        let coverage = ProviderFeature::ALL.map(ProviderFeature::live_coverage);

        assert_eq!(
            coverage,
            [
                ProviderFeatureLiveCoverage::None,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::None,
                ProviderFeatureLiveCoverage::None,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::None,
                ProviderFeatureLiveCoverage::Tui,
                ProviderFeatureLiveCoverage::Exec,
                ProviderFeatureLiveCoverage::None,
                ProviderFeatureLiveCoverage::Exec,
            ]
        );
    }
}
