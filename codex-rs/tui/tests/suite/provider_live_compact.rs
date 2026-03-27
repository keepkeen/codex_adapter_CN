#![cfg(not(target_os = "windows"))]

use anyhow::Result;
use codex_protocol::provider_profiles::ProviderFeature;
use core_test_support::provider_live::DEEPSEEK_PROVIDER;
use core_test_support::provider_live::GLM_PROVIDER;
use core_test_support::provider_live::KIMI_PROVIDER;
use core_test_support::provider_live::LiveProviderCase;
use core_test_support::provider_live::MINIMAX_PROVIDER;
use core_test_support::provider_live::skip_unless_configured;
use core_test_support::provider_live::skip_unless_feature_enabled;
use core_test_support::provider_live_tui::LiveTuiImplementation;
use core_test_support::provider_live_tui::run_live_manual_compact;
use core_test_support::skip_if_no_network;
use serial_test::serial;

async fn assert_manual_compact_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_configured(case, test_name).and_then(|api_key| {
        skip_unless_feature_enabled(case, ProviderFeature::ManualCompact, test_name, api_key)
    }) else {
        return Ok(());
    };

    let result = run_live_manual_compact(case, LiveTuiImplementation::Legacy, &api_key).await?;
    assert!(
        result.seed_stdout.contains("provider-live-compact-line-64"),
        "expected seeded exec thread output for {} legacy /compact smoke\nseed stdout:\n{}\nseed stderr:\n{}",
        case.provider_id,
        result.seed_stdout,
        result.seed_stderr
    );
    assert!(
        result.contains_compact_op()?,
        "expected /compact op in session log for {} legacy /compact smoke\npty output:\n{}",
        case.provider_id,
        result.output
    );
    assert!(
        result.contains_context_compacted_event()?,
        "expected context_compacted event in session log for {} legacy /compact smoke\npty output:\n{}",
        case.provider_id,
        result.output
    );
    assert!(
        result.contains_rollout_compacted_item()?,
        "expected compacted rollout item for {} legacy /compact smoke\npty output:\n{}",
        case.provider_id,
        result.output
    );

    Ok(())
}

macro_rules! provider_live_compact_tests {
    ($case:expr, $test_name:ident) => {
        #[ignore]
        #[tokio::test]
        #[serial]
        async fn $test_name() -> Result<()> {
            assert_manual_compact_smoke($case, stringify!($test_name)).await
        }
    };
}

provider_live_compact_tests!(DEEPSEEK_PROVIDER, deepseek_live_manual_compact_smoke);
provider_live_compact_tests!(GLM_PROVIDER, glm_live_manual_compact_smoke);
provider_live_compact_tests!(KIMI_PROVIDER, kimi_live_manual_compact_smoke);
provider_live_compact_tests!(MINIMAX_PROVIDER, minimax_live_manual_compact_smoke);
