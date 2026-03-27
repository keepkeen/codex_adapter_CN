#![cfg(not(target_os = "windows"))]
#![expect(clippy::expect_used)]

use anyhow::Result;
use core_test_support::provider_live::DEEPSEEK_PROVIDER;
use core_test_support::provider_live::GLM_PROVIDER;
use core_test_support::provider_live::KIMI_PROVIDER;
use core_test_support::provider_live::LiveProviderCase;
use core_test_support::provider_live::LiveWorkflow;
use core_test_support::provider_live::MINIMAX_PROVIDER;
use core_test_support::provider_live::run_live_exec;
use core_test_support::provider_live::skip_unless_live_enabled;
use core_test_support::skip_if_no_network;
use serial_test::serial;

fn assert_basic_shell_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::BasicShell, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::BasicShell, &api_key)?;
    assert!(
        result.contains_completed_command_output("provider-live-smoke-ok"),
        "expected command_execution output in JSONL for {} basic shell smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("provider-live-smoke-ok"),
        "expected agent message to echo shell sentinel for {} basic shell smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    let usage = result
        .completed_usage()
        .expect("turn.completed usage should be present in live JSONL output");
    assert!(
        usage
            .get("input_tokens")
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|tokens| tokens >= 0),
        "expected non-negative input_tokens in live usage for {} basic shell smoke: {usage}",
        case.provider_id
    );

    Ok(())
}

fn assert_web_search_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::WebSearch, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::WebSearch, &api_key)?;
    assert!(
        result.contains_completed_web_search(),
        "expected web_search item in JSONL for {} search smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("https://github.com/openai/codex"),
        "expected canonical repo URL in final agent message for {} search smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        !result.contains_completed_command_execution(),
        "search smoke should keep shell-style command execution disabled for {}\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.completed_usage().is_some(),
        "expected turn.completed usage in JSONL for {} search smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_skill_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::Skill, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::Skill, &api_key)?;
    assert!(
        result.contains_agent_message("provider-live-skill-ok::provider-live-secret-4e3fefdf"),
        "expected skill sentinel in final agent message for {} skill smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.completed_usage().is_some(),
        "expected turn.completed usage in JSONL for {} skill smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_mcp_echo_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::McpEcho, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::McpEcho, &api_key)?;
    assert!(
        result.contains_completed_mcp_tool_call("rmcp", "echo"),
        "expected completed mcp_tool_call in JSONL for {} mcp smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.mcp_tool_result_contains("provider-live-mcp-ok"),
        "expected MCP structured content to contain sentinel for {} mcp smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("provider-live-mcp-ok"),
        "expected final agent message to contain mcp sentinel for {} mcp smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_subagent_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::Subagent, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::Subagent, &api_key)?;
    assert!(
        result.contains_completed_collab_tool("spawn_agent"),
        "expected completed spawn_agent collab tool in JSONL for {} subagent smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_completed_collab_tool("wait"),
        "expected completed wait collab tool in JSONL for {} subagent smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.completed_spawn_has_receiver_thread(),
        "expected spawned child receiver thread ids for {} subagent smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("provider-live-subagent-ok"),
        "expected final agent message to contain delegated sentinel for {} subagent smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_js_repl_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::JsRepl, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::JsRepl, &api_key)?;
    assert!(
        result.contains_completed_function_call("js_repl"),
        "expected completed js_repl function call in JSONL for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_completed_function_call("js_repl_reset"),
        "expected completed js_repl_reset function call in JSONL for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_completed_command_output("provider-live-js-repl-persisted"),
        "expected persisted js_repl command output for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_completed_command_output("provider-live-js-repl-fresh"),
        "expected fresh js_repl command output for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("provider-live-js-repl-fresh"),
        "expected final agent message to contain fresh js_repl sentinel for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.thread_id.is_some(),
        "expected persistent thread id for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_path.is_some(),
        "expected rollout path for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_contains("\"name\":\"js_repl\"")?,
        "expected rollout artifact to contain js_repl tool call for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_contains("\"name\":\"js_repl_reset\"")?,
        "expected rollout artifact to contain js_repl_reset tool call for {} js_repl smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_output_schema_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::OutputSchema, test_name)
    else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::OutputSchema, &api_key)?;
    assert!(
        result.contains_agent_message("{\"answer\":\"provider-live-output-schema-ok\"}"),
        "expected schema-compliant JSON in final agent message for {} output schema smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        !result.contains_completed_command_execution(),
        "output schema smoke should not require command execution for {}\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.completed_usage().is_some(),
        "expected turn.completed usage in JSONL for {} output schema smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_auto_compact_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::AutoCompact, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::AutoCompact, &api_key)?;
    assert!(
        result.contains_completed_command_output("provider-live-compact-line-64"),
        "expected turn-one command output in JSONL for {} auto compact smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.contains_agent_message("provider-live-compact-turn-two"),
        "expected final agent message for {} auto compact smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.thread_id.is_some(),
        "expected persistent thread id for {} auto compact smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_path.is_some(),
        "expected rollout path for {} auto compact smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_contains("\"type\":\"compacted\"")?,
        "expected rollout artifact to contain compacted item for {} auto compact smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

fn assert_image_input_smoke(case: LiveProviderCase, test_name: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let Some(api_key) = skip_unless_live_enabled(case, LiveWorkflow::ImageInput, test_name) else {
        return Ok(());
    };

    let result = run_live_exec(case, LiveWorkflow::ImageInput, &api_key)?;
    assert!(
        result.contains_agent_message("provider-live-image-input-ok"),
        "expected final agent message for {} image input smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.completed_usage().is_some(),
        "expected turn.completed usage in JSONL for {} image input smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_path.is_some(),
        "expected rollout path for {} image input smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );
    assert!(
        result.rollout_contains("\"type\":\"input_image\"")?,
        "expected rollout artifact to contain input_image item for {} image input smoke\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        result.stdout,
        result.stderr
    );

    Ok(())
}

macro_rules! provider_live_smoke_tests {
    (
        $case:expr,
        $basic:ident,
        $search:ident,
        $skill:ident,
        $mcp:ident,
        $subagent:ident,
        $js_repl:ident,
        $output_schema:ident,
        $auto_compact:ident
    ) => {
        #[ignore]
        #[test]
        #[serial]
        fn $basic() -> Result<()> {
            assert_basic_shell_smoke($case, stringify!($basic))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $search() -> Result<()> {
            assert_web_search_smoke($case, stringify!($search))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $skill() -> Result<()> {
            assert_skill_smoke($case, stringify!($skill))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $mcp() -> Result<()> {
            assert_mcp_echo_smoke($case, stringify!($mcp))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $subagent() -> Result<()> {
            assert_subagent_smoke($case, stringify!($subagent))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $js_repl() -> Result<()> {
            assert_js_repl_smoke($case, stringify!($js_repl))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $output_schema() -> Result<()> {
            assert_output_schema_smoke($case, stringify!($output_schema))
        }

        #[ignore]
        #[test]
        #[serial]
        fn $auto_compact() -> Result<()> {
            assert_auto_compact_smoke($case, stringify!($auto_compact))
        }
    };
}

provider_live_smoke_tests!(
    DEEPSEEK_PROVIDER,
    deepseek_exec_live_basic_shell_smoke,
    deepseek_exec_live_web_search_smoke,
    deepseek_exec_live_skill_smoke,
    deepseek_exec_live_mcp_echo_smoke,
    deepseek_exec_live_subagent_smoke,
    deepseek_exec_live_js_repl_smoke,
    deepseek_exec_live_output_schema_smoke,
    deepseek_exec_live_auto_compact_smoke
);

provider_live_smoke_tests!(
    GLM_PROVIDER,
    glm_exec_live_basic_shell_smoke,
    glm_exec_live_web_search_smoke,
    glm_exec_live_skill_smoke,
    glm_exec_live_mcp_echo_smoke,
    glm_exec_live_subagent_smoke,
    glm_exec_live_js_repl_smoke,
    glm_exec_live_output_schema_smoke,
    glm_exec_live_auto_compact_smoke
);

provider_live_smoke_tests!(
    KIMI_PROVIDER,
    kimi_exec_live_basic_shell_smoke,
    kimi_exec_live_web_search_smoke,
    kimi_exec_live_skill_smoke,
    kimi_exec_live_mcp_echo_smoke,
    kimi_exec_live_subagent_smoke,
    kimi_exec_live_js_repl_smoke,
    kimi_exec_live_output_schema_smoke,
    kimi_exec_live_auto_compact_smoke
);

provider_live_smoke_tests!(
    MINIMAX_PROVIDER,
    minimax_exec_live_basic_shell_smoke,
    minimax_exec_live_web_search_smoke,
    minimax_exec_live_skill_smoke,
    minimax_exec_live_mcp_echo_smoke,
    minimax_exec_live_subagent_smoke,
    minimax_exec_live_js_repl_smoke,
    minimax_exec_live_output_schema_smoke,
    minimax_exec_live_auto_compact_smoke
);

#[ignore]
#[test]
#[serial]
fn glm_exec_live_image_input_smoke() -> Result<()> {
    assert_image_input_smoke(GLM_PROVIDER, "glm_exec_live_image_input_smoke")
}

#[ignore]
#[test]
#[serial]
fn kimi_exec_live_image_input_smoke() -> Result<()> {
    assert_image_input_smoke(KIMI_PROVIDER, "kimi_exec_live_image_input_smoke")
}
