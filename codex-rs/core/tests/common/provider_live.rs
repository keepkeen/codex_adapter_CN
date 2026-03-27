use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use assert_cmd::Command as AssertCommand;
use codex_protocol::openai_models::InputModality;
use codex_protocol::provider_profiles::FeatureSupport;
use codex_protocol::provider_profiles::ProviderFeature;
use codex_protocol::provider_profiles::built_in_provider_profile;
use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::stdio_server_bin;

pub(crate) const PROVIDER_LIVE_ARTIFACT_DIR_ENV_VAR: &str = "CODEX_PROVIDER_LIVE_ARTIFACT_DIR";
const PROVIDER_LIVE_SKILL_NAME: &str = "provider-live-demo";
const PROVIDER_LIVE_SKILL_SENTINEL: &str = "provider-live-skill-ok::provider-live-secret-4e3fefdf";
const PROVIDER_LIVE_MCP_SERVER: &str = "rmcp";
const PROVIDER_LIVE_MCP_SENTINEL: &str = "provider-live-mcp-ok";
const PROVIDER_LIVE_IMAGE_FILE_NAME: &str = "provider-live-image.png";
const PROVIDER_LIVE_OUTPUT_SCHEMA_FILE_NAME: &str = "provider-live-output-schema.json";
const PROVIDER_LIVE_OUTPUT_SCHEMA_JSON: &str = r#"{
  "type": "object",
  "properties": {
    "answer": {
      "type": "string"
    }
  },
  "required": [
    "answer"
  ],
  "additionalProperties": false
}"#;
const PROVIDER_LIVE_IMAGE_PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveWorkflow {
    BasicShell,
    WebSearch,
    Skill,
    McpEcho,
    Subagent,
    JsRepl,
    OutputSchema,
    AutoCompact,
    ImageInput,
}

impl LiveWorkflow {
    pub const ALL: [Self; 9] = [
        Self::BasicShell,
        Self::WebSearch,
        Self::Skill,
        Self::McpEcho,
        Self::Subagent,
        Self::JsRepl,
        Self::OutputSchema,
        Self::AutoCompact,
        Self::ImageInput,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::BasicShell => "basic-shell",
            Self::WebSearch => "web-search",
            Self::Skill => "skill",
            Self::McpEcho => "mcp-echo",
            Self::Subagent => "subagent",
            Self::JsRepl => "js-repl",
            Self::OutputSchema => "output-schema",
            Self::AutoCompact => "auto-compact",
            Self::ImageInput => "image-input",
        }
    }

    fn initial_prompt(self) -> &'static str {
        match self {
            Self::BasicShell => {
                "You must use the shell tool exactly once to run `printf 'provider-live-smoke-ok\\n'`. Then reply with exactly `provider-live-smoke-ok`."
            }
            Self::WebSearch => {
                "You must call the web_search tool at least once to find the canonical GitHub repository URL for openai/codex. Reply with only the URL."
            }
            Self::Skill => {
                "Use $provider-live-demo exactly once and reply with exactly the secret phrase defined in that skill."
            }
            Self::McpEcho => {
                "Call the rmcp echo MCP tool exactly once with the message `provider-live-mcp-ok`. Then reply with exactly `provider-live-mcp-ok`."
            }
            Self::Subagent => {
                "Use spawn_agent exactly once to ask a child agent to reply with exactly `provider-live-subagent-ok`. Wait for the child to finish, then reply with exactly `provider-live-subagent-ok`."
            }
            Self::JsRepl => {
                "Call `js_repl` exactly once with JavaScript that sets `globalThis.providerLiveCounter = 41` and prints `provider-live-js-repl-seeded`. Then reply with exactly `provider-live-js-repl-seeded`."
            }
            Self::OutputSchema => {
                "Return a JSON object whose `answer` field is exactly `provider-live-output-schema-ok`. Do not call any tools."
            }
            Self::AutoCompact => {
                "You must use the shell tool exactly once to run `seq 1 64 | sed 's/.*/provider-live-compact-line-&/'`. Then reply with exactly `provider-live-compact-turn-one`."
            }
            Self::ImageInput => {
                "An image is attached to this turn. Reply with exactly `provider-live-image-input-ok` and do not call any tools."
            }
        }
    }

    fn follow_up_prompts(self) -> &'static [&'static str] {
        match self {
            Self::JsRepl => &[
                "Call `js_repl` exactly once with JavaScript that throws unless `globalThis.providerLiveCounter === 41`, then prints `provider-live-js-repl-persisted`. Then reply with exactly `provider-live-js-repl-persisted`.",
                "Call `js_repl_reset` exactly once. Then reply with exactly `provider-live-js-repl-reset-ok` and do not call any other tools.",
                "Call `js_repl` exactly once with JavaScript that throws unless `typeof globalThis.providerLiveCounter === 'undefined'`, then prints `provider-live-js-repl-fresh`. Then reply with exactly `provider-live-js-repl-fresh`.",
            ],
            Self::AutoCompact => {
                &["Reply with exactly `provider-live-compact-turn-two` and do not call any tools."]
            }
            Self::BasicShell
            | Self::WebSearch
            | Self::Skill
            | Self::McpEcho
            | Self::Subagent
            | Self::OutputSchema
            | Self::ImageInput => &[],
        }
    }

    fn uses_persistent_thread(self) -> bool {
        self == Self::ImageInput || !self.follow_up_prompts().is_empty()
    }

    fn required_feature(self) -> Option<ProviderFeature> {
        match self {
            Self::BasicShell => None,
            Self::WebSearch => Some(ProviderFeature::WebSearch),
            Self::Skill => Some(ProviderFeature::Skills),
            Self::McpEcho => Some(ProviderFeature::Mcp),
            Self::Subagent => Some(ProviderFeature::Subagents),
            Self::JsRepl => Some(ProviderFeature::JsRepl),
            Self::OutputSchema => Some(ProviderFeature::OutputSchema),
            Self::AutoCompact => Some(ProviderFeature::AutoCompact),
            Self::ImageInput => Some(ProviderFeature::ImageInput),
        }
    }

    fn config_overrides(self) -> Vec<String> {
        match self {
            Self::BasicShell
            | Self::Skill
            | Self::McpEcho
            | Self::Subagent
            | Self::JsRepl
            | Self::OutputSchema
            | Self::ImageInput => Vec::new(),
            Self::WebSearch => vec![
                "web_search=\"live\"".to_string(),
                "tools.web_search.allowed_domains=[\"github.com\"]".to_string(),
                "disabled_tools=[\"shell\",\"shell_command\",\"exec_command\",\"write_stdin\"]"
                    .to_string(),
            ],
            Self::AutoCompact => vec!["model_auto_compact_token_limit=128".to_string()],
        }
    }

    fn prepare_home(self, home: &Path) -> Result<()> {
        match self {
            Self::Skill => write_skill(
                home,
                PROVIDER_LIVE_SKILL_NAME,
                "provider live smoke skill",
                format!(
                    "Reply with exactly `{PROVIDER_LIVE_SKILL_SENTINEL}`.\nDo not add any explanation."
                )
                .as_str(),
            ),
            Self::McpEcho => write_provider_live_mcp_config(home),
            Self::Subagent => append_home_config(home, "[features]\nmulti_agent = true\n"),
            Self::JsRepl | Self::OutputSchema | Self::ImageInput => Ok(()),
            Self::BasicShell | Self::WebSearch | Self::AutoCompact => Ok(()),
        }
    }

    fn model(self, case: LiveProviderCase) -> Result<&'static str> {
        match self {
            Self::ImageInput => {
                let profile = built_in_provider_profile(case.provider_id).with_context(|| {
                    format!("resolve built-in profile for {}", case.provider_id)
                })?;
                profile
                    .bundled_model_catalog
                    .iter()
                    .find(|entry| entry.input_modalities.contains(&InputModality::Image))
                    .map(|entry| entry.slug)
                    .with_context(|| format!("find image-capable model for {}", case.provider_id))
            }
            Self::BasicShell
            | Self::WebSearch
            | Self::Skill
            | Self::McpEcho
            | Self::Subagent
            | Self::JsRepl
            | Self::OutputSchema
            | Self::AutoCompact => Ok(case.model),
        }
    }

    fn append_cli_args(self, command: &mut AssertCommand, cwd: &Path) -> Result<()> {
        if self == Self::ImageInput {
            let image_path = cwd.join(PROVIDER_LIVE_IMAGE_FILE_NAME);
            fs::write(&image_path, PROVIDER_LIVE_IMAGE_PNG_BYTES)
                .with_context(|| format!("write provider live image {}", image_path.display()))?;
            command.arg("--image").arg(image_path);
        }
        if self == Self::OutputSchema {
            let schema_path = cwd.join(PROVIDER_LIVE_OUTPUT_SCHEMA_FILE_NAME);
            fs::write(&schema_path, PROVIDER_LIVE_OUTPUT_SCHEMA_JSON)
                .with_context(|| format!("write output schema {}", schema_path.display()))?;
            command.arg("--output-schema").arg(schema_path);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveProviderCase {
    pub provider_id: &'static str,
    pub model: &'static str,
    pub env_keys: &'static [&'static str],
}

pub const DEEPSEEK_PROVIDER: LiveProviderCase = LiveProviderCase {
    provider_id: "deepseek",
    model: "deepseek-chat",
    env_keys: &["DEEPSEEK_API_KEY"],
};

pub const GLM_PROVIDER: LiveProviderCase = LiveProviderCase {
    provider_id: "glm",
    model: "glm-5",
    env_keys: &["GLM_API_KEY"],
};

pub const KIMI_PROVIDER: LiveProviderCase = LiveProviderCase {
    provider_id: "kimi",
    model: "kimi-k2.5",
    env_keys: &["MOONSHOT_API_KEY", "KIMI_API_KEY"],
};

pub const MINIMAX_PROVIDER: LiveProviderCase = LiveProviderCase {
    provider_id: "minimax",
    model: "MiniMax-M2.7",
    env_keys: &["MINIMAX_API_KEY"],
};

pub struct LiveInvocationResult {
    pub stdout: String,
    pub stderr: String,
    pub events: Vec<Value>,
}

struct LiveExecInvocationRequest<'a> {
    prompt: &'a str,
    resume_thread_id: Option<&'a str>,
    run_index: usize,
}

pub struct LiveRunResult {
    pub stdout: String,
    pub stderr: String,
    pub events: Vec<Value>,
    pub invocations: Vec<LiveInvocationResult>,
    pub thread_id: Option<String>,
    pub rollout_path: Option<PathBuf>,
    _home: TempDir,
    _cwd: TempDir,
}

impl LiveRunResult {
    fn all_events(&self) -> impl Iterator<Item = &Value> {
        self.invocations.iter().flat_map(|run| run.events.iter())
    }

    pub fn contains_completed_command_execution(&self) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("command_execution")
        })
    }

    pub fn contains_completed_command_output(&self, needle: &str) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("command_execution")
                && event
                    .pointer("/item/aggregated_output")
                    .and_then(Value::as_str)
                    .is_some_and(|output| output.contains(needle))
        })
    }

    pub fn contains_completed_web_search(&self) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("web_search")
        })
    }

    pub fn contains_agent_message(&self, needle: &str) -> bool {
        self.all_events().any(|event| {
            matches!(
                event.get("type").and_then(Value::as_str),
                Some("item.completed") | Some("item.updated")
            ) && event.pointer("/item/type").and_then(Value::as_str) == Some("agent_message")
                && event
                    .pointer("/item/text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains(needle))
        })
    }

    pub fn contains_completed_mcp_tool_call(&self, server: &str, tool: &str) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("mcp_tool_call")
                && event.pointer("/item/server").and_then(Value::as_str) == Some(server)
                && event.pointer("/item/tool").and_then(Value::as_str) == Some(tool)
                && event.pointer("/item/status").and_then(Value::as_str) == Some("completed")
        })
    }

    pub fn mcp_tool_result_contains(&self, needle: &str) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("mcp_tool_call")
                && event
                    .pointer("/item/result/structured_content")
                    .is_some_and(|result| result.to_string().contains(needle))
        })
    }

    pub fn contains_completed_collab_tool(&self, tool: &str) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("collab_tool_call")
                && event.pointer("/item/tool").and_then(Value::as_str) == Some(tool)
                && event.pointer("/item/status").and_then(Value::as_str) == Some("completed")
        })
    }

    pub fn contains_completed_function_call(&self, tool: &str) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("function_call")
                && event.pointer("/item/name").and_then(Value::as_str) == Some(tool)
        })
    }

    pub fn completed_spawn_has_receiver_thread(&self) -> bool {
        self.all_events().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event.pointer("/item/type").and_then(Value::as_str) == Some("collab_tool_call")
                && event.pointer("/item/tool").and_then(Value::as_str) == Some("spawn_agent")
                && event
                    .pointer("/item/receiver_thread_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| !ids.is_empty())
        })
    }

    pub fn completed_usage(&self) -> Option<&Value> {
        self.invocations.iter().rev().find_map(|run| {
            run.events.iter().rev().find_map(|event| {
                (event.get("type").and_then(Value::as_str) == Some("turn.completed"))
                    .then(|| event.get("usage"))
                    .flatten()
            })
        })
    }

    pub fn rollout_contains(&self, needle: &str) -> Result<bool> {
        let Some(rollout_path) = self.rollout_path.as_ref() else {
            return Ok(false);
        };
        let rollout = fs::read_to_string(rollout_path)
            .with_context(|| format!("read rollout {}", rollout_path.display()))?;
        Ok(rollout.contains(needle))
    }
}

pub fn configured_api_key(case: LiveProviderCase) -> Option<String> {
    case.env_keys.iter().find_map(|env_key| {
        std::env::var(env_key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
}

pub fn skip_unless_configured(case: LiveProviderCase, test_name: &str) -> Option<String> {
    match configured_api_key(case) {
        Some(api_key) => Some(api_key),
        None => {
            eprintln!(
                "skipping {test_name} – none of [{}] are configured",
                case.env_keys.join(", ")
            );
            None
        }
    }
}

pub fn skip_unless_live_enabled(
    case: LiveProviderCase,
    workflow: LiveWorkflow,
    test_name: &str,
) -> Option<String> {
    let api_key = skip_unless_configured(case, test_name)?;
    let Some(feature) = workflow.required_feature() else {
        return Some(api_key);
    };
    skip_unless_feature_enabled(case, feature, test_name, api_key)
}

pub fn skip_unless_feature_enabled(
    case: LiveProviderCase,
    feature: ProviderFeature,
    test_name: &str,
    api_key: String,
) -> Option<String> {
    let Some(profile) = built_in_provider_profile(case.provider_id) else {
        return Some(api_key);
    };
    if profile.features.support_for(feature) == FeatureSupport::Unsupported {
        eprintln!(
            "skipping {test_name} – {} is unsupported for {}",
            feature.label(),
            case.provider_id
        );
        return None;
    }
    Some(api_key)
}

fn write_skill(home: &Path, name: &str, description: &str, body: &str) -> Result<()> {
    let skill_dir = home.join("skills").join(name);
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("create provider live skill dir {}", skill_dir.display()))?;
    let contents = format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n");
    fs::write(skill_dir.join("SKILL.md"), contents)
        .with_context(|| format!("write provider live skill {name}"))?;
    Ok(())
}

pub(crate) fn append_home_config(home: &Path, snippet: &str) -> Result<()> {
    let config_path = home.join("config.toml");
    let mut contents = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("read provider live config {}", config_path.display()))?
    } else {
        String::new()
    };
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(snippet);
    fs::write(&config_path, contents)
        .with_context(|| format!("write provider live config {}", config_path.display()))?;
    Ok(())
}

fn write_provider_live_mcp_config(home: &Path) -> Result<()> {
    let command = toml_string_literal(&stdio_server_bin().context("resolve test_stdio_server")?)?;
    append_home_config(
        home,
        format!(
            "[mcp_servers.{PROVIDER_LIVE_MCP_SERVER}]\ncommand = {command}\nstartup_timeout_sec = 10.0\n\n[mcp_servers.{PROVIDER_LIVE_MCP_SERVER}.env]\nMCP_TEST_VALUE = \"{PROVIDER_LIVE_MCP_SENTINEL}\"\n"
        )
        .as_str(),
    )
}

pub(crate) fn find_thread_id(events: &[Value]) -> Option<String> {
    events.iter().find_map(|event| {
        (event.get("type").and_then(Value::as_str) == Some("thread.started"))
            .then(|| event.get("thread_id"))
            .flatten()
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

pub(crate) fn find_rollout_path(home: &Path, thread_id: &str) -> Result<Option<PathBuf>> {
    let sessions_dir = home.join("sessions");
    if !sessions_dir.exists() {
        return Ok(None);
    }

    for entry in WalkDir::new(&sessions_dir) {
        let entry = entry
            .with_context(|| format!("walk provider live sessions {}", sessions_dir.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension() != Some(OsStr::new("jsonl")) {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if file_name.contains(thread_id) {
            return Ok(Some(path.to_path_buf()));
        }
    }

    Ok(None)
}

pub(crate) fn resolve_codex_binary() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_PROVIDER_LIVE_BIN") {
        let path = PathBuf::from(path);
        ensure!(
            path.is_file(),
            "CODEX_PROVIDER_LIVE_BIN does not point to a file: {}",
            path.display()
        );
        return Ok(path);
    }

    if let Ok(path) = codex_utils_cargo_bin::cargo_bin("codex") {
        return Ok(path);
    }

    let fallback = codex_utils_cargo_bin::repo_root()
        .context("resolve repo root for codex fallback binary")?
        .join("codex-rs/target/debug/codex");
    ensure!(
        fallback.is_file(),
        "could not locate binary \"codex\"; tried cargo_bin and {}",
        fallback.display()
    );
    Ok(fallback)
}

fn run_live_exec_invocation(
    case: LiveProviderCase,
    workflow: LiveWorkflow,
    api_key: &str,
    home: &Path,
    cwd: &Path,
    request: LiveExecInvocationRequest<'_>,
) -> Result<LiveInvocationResult> {
    let sqlite_home = home.join("sqlite");
    fs::create_dir_all(&sqlite_home)
        .with_context(|| format!("create sqlite dir {}", sqlite_home.display()))?;

    let mut command = AssertCommand::new(resolve_codex_binary()?);
    command.timeout(Duration::from_secs(180));
    command
        .current_dir(cwd)
        .env("CODEX_HOME", home)
        .env("CODEX_SQLITE_HOME", &sqlite_home)
        .arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("-c")
        .arg(format!(
            "model_provider={}",
            toml_string_literal(case.provider_id)?
        ))
        .arg("-c")
        .arg(format!(
            "model={}",
            toml_string_literal(workflow.model(case)?)?
        ))
        .arg("-C")
        .arg(cwd);
    if request.resume_thread_id.is_none() && !workflow.uses_persistent_thread() {
        command.arg("--ephemeral");
    }
    for env_key in case.env_keys {
        command.env(env_key, api_key);
    }
    for override_value in workflow.config_overrides() {
        command.arg("-c").arg(override_value);
    }
    workflow.append_cli_args(&mut command, cwd)?;
    if let Some(thread_id) = request.resume_thread_id {
        command.arg("resume").arg(thread_id);
    }
    command.arg(request.prompt);

    let output = command.output().context("run codex exec")?;
    let status = output.status;
    let stdout = String::from_utf8(output.stdout).context("stdout was not utf-8")?;
    let stderr = String::from_utf8(output.stderr).context("stderr was not utf-8")?;
    persist_live_artifacts(
        case,
        workflow,
        request.run_index,
        home,
        cwd,
        &stdout,
        &stderr,
    )?;

    ensure!(
        status.success(),
        "live {} smoke for {} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        workflow.label(),
        case.provider_id,
        status,
        stdout,
        stderr
    );

    let events = parse_jsonl_events(&stdout).with_context(|| {
        format!(
            "parse JSONL output for {} {}\nstderr:\n{}",
            case.provider_id,
            workflow.label(),
            stderr
        )
    })?;

    Ok(LiveInvocationResult {
        stdout,
        stderr,
        events,
    })
}

pub fn run_live_exec(
    case: LiveProviderCase,
    workflow: LiveWorkflow,
    api_key: &str,
) -> Result<LiveRunResult> {
    let home = TempDir::new().context("create temp CODEX_HOME")?;
    let cwd = TempDir::new().context("create temp cwd")?;
    workflow.prepare_home(home.path())?;

    let mut invocations = vec![run_live_exec_invocation(
        case,
        workflow,
        api_key,
        home.path(),
        cwd.path(),
        LiveExecInvocationRequest {
            prompt: workflow.initial_prompt(),
            resume_thread_id: None,
            run_index: 1,
        },
    )?];
    let thread_id = find_thread_id(&invocations[0].events);

    if !workflow.follow_up_prompts().is_empty() {
        let thread_id = thread_id
            .clone()
            .with_context(|| format!("missing thread.started event for {}", workflow.label()))?;
        for (index, prompt) in workflow.follow_up_prompts().iter().enumerate() {
            invocations.push(run_live_exec_invocation(
                case,
                workflow,
                api_key,
                home.path(),
                cwd.path(),
                LiveExecInvocationRequest {
                    prompt,
                    resume_thread_id: Some(thread_id.as_str()),
                    run_index: index + 2,
                },
            )?);
        }
    }

    let last_invocation = invocations
        .last()
        .with_context(|| format!("missing live invocation for {}", workflow.label()))?;
    let rollout_path = match thread_id.as_deref() {
        Some(thread_id) => find_rollout_path(home.path(), thread_id)?,
        None => None,
    };

    Ok(LiveRunResult {
        stdout: last_invocation.stdout.clone(),
        stderr: last_invocation.stderr.clone(),
        events: last_invocation.events.clone(),
        invocations,
        thread_id,
        rollout_path,
        _home: home,
        _cwd: cwd,
    })
}

pub(crate) fn parse_jsonl_events(stdout: &str) -> Result<Vec<Value>> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .with_context(|| format!("invalid JSONL event: {line}"))
        })
        .collect()
}

pub(crate) fn toml_string_literal(value: &str) -> Result<String> {
    serde_json::to_string(value).context("serialize TOML string literal")
}

fn persist_live_artifacts(
    case: LiveProviderCase,
    workflow: LiveWorkflow,
    run_index: usize,
    home: &Path,
    cwd: &Path,
    stdout: &str,
    stderr: &str,
) -> Result<()> {
    let Some(root) = std::env::var_os(PROVIDER_LIVE_ARTIFACT_DIR_ENV_VAR) else {
        return Ok(());
    };

    let artifact_dir = PathBuf::from(root)
        .join(case.provider_id)
        .join(workflow.label());
    fs::create_dir_all(&artifact_dir).with_context(|| {
        format!(
            "create provider live artifact dir {}",
            artifact_dir.display()
        )
    })?;
    copy_live_home_artifacts(home, &artifact_dir.join("home"))?;
    copy_tree(cwd, &artifact_dir.join("cwd"))?;
    fs::write(
        artifact_dir.join(format!("run-{run_index}-stdout.jsonl")),
        stdout,
    )
    .context("write provider live stdout artifact")?;
    fs::write(
        artifact_dir.join(format!("run-{run_index}-stderr.txt")),
        stderr,
    )
    .context("write provider live stderr artifact")?;
    fs::write(artifact_dir.join("stdout.jsonl"), stdout).context("write latest stdout artifact")?;
    fs::write(artifact_dir.join("stderr.txt"), stderr).context("write latest stderr artifact")?;
    Ok(())
}

pub(crate) fn copy_live_home_artifacts(home: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("create provider live home dir {}", destination.display()))?;
    for relative in [
        "config.toml",
        "logs",
        "sessions",
        "sqlite",
        "shell_snapshots",
        "memories",
        "skills",
    ] {
        let source_path = home.join(relative);
        if !source_path.exists() {
            continue;
        }
        let destination_path = destination.join(relative);
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("create provider live artifact parent {}", parent.display())
                })?;
            }
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "copy provider live artifact {} -> {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    for entry in fs::read_dir(home)
        .with_context(|| format!("read provider live home dir {}", home.display()))?
    {
        let entry =
            entry.with_context(|| format!("read provider live home entry {}", home.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("session-") || !name.ends_with(".jsonl") {
            continue;
        }
        fs::copy(&path, destination.join(name)).with_context(|| {
            format!(
                "copy provider live session log {} -> {}",
                path.display(),
                destination.join(name).display()
            )
        })?;
    }
    Ok(())
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry
            .with_context(|| format!("walk provider live artifact tree {}", source.display()))?;
        let relative = entry.path().strip_prefix(source).with_context(|| {
            format!(
                "strip provider live artifact prefix {} from {}",
                source.display(),
                entry.path().display()
            )
        })?;
        let destination_path = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination_path).with_context(|| {
                format!(
                    "create provider live artifact dir {}",
                    destination_path.display()
                )
            })?;
            continue;
        }
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create provider live artifact parent {}", parent.display())
            })?;
        }
        fs::copy(entry.path(), &destination_path).with_context(|| {
            format!(
                "copy provider live artifact {} -> {}",
                entry.path().display(),
                destination_path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn kimi_provider_keeps_primary_and_legacy_env_keys() {
        assert_eq!(
            KIMI_PROVIDER.env_keys,
            &["MOONSHOT_API_KEY", "KIMI_API_KEY"]
        );
    }

    #[test]
    fn web_search_workflow_label_is_stable() {
        assert_eq!(LiveWorkflow::WebSearch.label(), "web-search");
    }

    #[test]
    fn auto_compact_workflow_uses_persistent_thread() {
        assert!(LiveWorkflow::AutoCompact.uses_persistent_thread());
        assert_eq!(
            LiveWorkflow::AutoCompact.follow_up_prompts(),
            &["Reply with exactly `provider-live-compact-turn-two` and do not call any tools."]
        );
    }

    #[test]
    fn image_input_workflow_uses_persistent_thread_and_feature_gate() {
        assert!(LiveWorkflow::ImageInput.uses_persistent_thread());
        assert_eq!(
            LiveWorkflow::ImageInput.required_feature(),
            Some(ProviderFeature::ImageInput)
        );
        assert_eq!(LiveWorkflow::ImageInput.label(), "image-input");
    }

    #[test]
    fn js_repl_workflow_uses_persistent_thread_and_feature_gate() {
        assert!(LiveWorkflow::JsRepl.uses_persistent_thread());
        assert_eq!(
            LiveWorkflow::JsRepl.required_feature(),
            Some(ProviderFeature::JsRepl)
        );
        assert_eq!(LiveWorkflow::JsRepl.label(), "js-repl");
        assert_eq!(LiveWorkflow::JsRepl.follow_up_prompts().len(), 3);
    }

    #[test]
    fn output_schema_workflow_uses_schema_feature_gate() {
        assert_eq!(
            LiveWorkflow::OutputSchema.required_feature(),
            Some(ProviderFeature::OutputSchema)
        );
        assert_eq!(LiveWorkflow::OutputSchema.label(), "output-schema");
    }

    #[test]
    fn exec_live_workflows_cover_all_exec_required_provider_features() {
        let covered = LiveWorkflow::ALL
            .into_iter()
            .filter_map(LiveWorkflow::required_feature)
            .collect::<std::collections::BTreeSet<_>>();
        let required = ProviderFeature::ALL
            .into_iter()
            .filter(|feature| feature.live_coverage().requires_exec_live_smoke())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(covered, required);
    }

    #[test]
    fn toml_string_literal_quotes_provider_ids() {
        assert_eq!(
            toml_string_literal("deepseek").expect("serialize string"),
            "\"deepseek\"".to_string()
        );
    }

    #[test]
    fn live_run_result_detects_web_search_completion() {
        let invocation = LiveInvocationResult {
            stdout: String::new(),
            stderr: String::new(),
            events: vec![serde_json::json!({
                "type": "item.completed",
                "item": {
                    "type": "web_search",
                    "query": "openai/codex"
                }
            })],
        };
        let result = LiveRunResult {
            stdout: String::new(),
            stderr: String::new(),
            events: invocation.events.clone(),
            invocations: vec![invocation],
            thread_id: None,
            rollout_path: None,
            _home: TempDir::new().expect("temp dir"),
            _cwd: TempDir::new().expect("temp dir"),
        };

        assert!(result.contains_completed_web_search());
    }

    #[test]
    fn live_run_result_detects_completed_function_call() {
        let invocation = LiveInvocationResult {
            stdout: String::new(),
            stderr: String::new(),
            events: vec![serde_json::json!({
                "type": "item.completed",
                "item": {
                    "type": "function_call",
                    "name": "js_repl"
                }
            })],
        };
        let result = LiveRunResult {
            stdout: String::new(),
            stderr: String::new(),
            events: invocation.events.clone(),
            invocations: vec![invocation],
            thread_id: None,
            rollout_path: None,
            _home: TempDir::new().expect("temp dir"),
            _cwd: TempDir::new().expect("temp dir"),
        };

        assert!(result.contains_completed_function_call("js_repl"));
    }
}
