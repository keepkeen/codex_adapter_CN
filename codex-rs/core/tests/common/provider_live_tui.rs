use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use codex_protocol::provider_profiles::ProviderFeature;
use codex_utils_pty::TerminalSize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use tempfile::TempDir;
use tokio::select;
use tokio::time::interval;
use tokio::time::sleep;
use tokio::time::timeout;
use walkdir::WalkDir;

use crate::provider_live::LiveProviderCase;
use crate::provider_live::PROVIDER_LIVE_ARTIFACT_DIR_ENV_VAR;
use crate::provider_live::append_home_config;
use crate::provider_live::copy_live_home_artifacts;
use crate::provider_live::find_rollout_path;
use crate::provider_live::find_thread_id;
use crate::provider_live::parse_jsonl_events;
use crate::provider_live::resolve_codex_binary;
use crate::provider_live::toml_string_literal;
use crate::provider_live_tui_session::session_log_contains_app_server_startup_ready;
use crate::provider_live_tui_session::session_log_contains_compact_op;
use crate::provider_live_tui_session::session_log_contains_context_compacted_event;
use crate::provider_live_tui_session::session_log_contains_startup_ready;
use crate::provider_live_tui_session::session_log_rollout_path;

const MANUAL_COMPACT_SEED_PROMPT: &str = "You must use the shell tool exactly once to run `seq 1 64 | awk '{print \"provider-live-compact-line-\" $1}'`. Then reply with exactly `provider-live-compact-turn-one`.";
const MANUAL_COMPACT_READY_DELAY: Duration = Duration::from_secs(1);
const MANUAL_COMPACT_KEYPRESS_DELAY: Duration = Duration::from_millis(35);
const MANUAL_COMPACT_ENTER_DELAY: Duration = Duration::from_millis(250);
const MANUAL_COMPACT_EXIT_COMMAND: &[u8] = b"/exit";
pub const TUI_LIVE_COVERED_FEATURES: &[ProviderFeature] = &[ProviderFeature::ManualCompact];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveTuiImplementation {
    Legacy,
    AppServer,
}

impl LiveTuiImplementation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Legacy => "manual-compact-legacy",
            Self::AppServer => "manual-compact-app-server",
        }
    }

    fn uses_app_server(self) -> bool {
        matches!(self, Self::AppServer)
    }
}

pub struct LiveTuiCompactResult {
    pub output: String,
    pub seed_stdout: String,
    pub seed_stderr: String,
    pub session_log_path: PathBuf,
    pub rollout_path: PathBuf,
    _home: TempDir,
    _cwd: PathBuf,
}

struct PtyRunOutcome {
    output: String,
    error: Option<anyhow::Error>,
}

#[derive(Clone, Copy)]
struct LiveTuiEnvironment<'a> {
    home: &'a Path,
    cwd: &'a Path,
    sqlite_home: &'a Path,
    log_dir: &'a Path,
    session_log_path: &'a Path,
}

impl LiveTuiCompactResult {
    pub fn session_log_text(&self) -> Result<String> {
        fs::read_to_string(&self.session_log_path)
            .with_context(|| format!("read session log {}", self.session_log_path.display()))
    }

    pub fn rollout_text(&self) -> Result<String> {
        fs::read_to_string(&self.rollout_path)
            .with_context(|| format!("read rollout {}", self.rollout_path.display()))
    }

    pub fn contains_compact_op(&self) -> Result<bool> {
        Ok(session_log_contains_compact_op(&self.session_log_text()?))
    }

    pub fn contains_context_compacted_event(&self) -> Result<bool> {
        let session_log = self.session_log_text()?;
        let rollout = self.rollout_text()?;
        Ok(session_log_contains_context_compacted_event(&session_log)
            || rollout_contains_context_compacted_event(&rollout))
    }

    pub fn contains_rollout_compacted_item(&self) -> Result<bool> {
        Ok(self.rollout_text()?.contains("\"type\":\"compacted\""))
    }
}

pub async fn run_live_manual_compact(
    case: LiveProviderCase,
    implementation: LiveTuiImplementation,
    api_key: &str,
) -> Result<LiveTuiCompactResult> {
    let home = TempDir::new().context("create temp CODEX_HOME for manual compact")?;
    let cwd = codex_utils_cargo_bin::repo_root().context("resolve repo root for manual compact")?;
    let sqlite_home = home.path().join("sqlite");
    fs::create_dir_all(&sqlite_home)
        .with_context(|| format!("create sqlite dir {}", sqlite_home.display()))?;

    write_live_tui_config(case, implementation, home.path(), &cwd)?;

    let (seed_stdout, seed_stderr, seed_rollout_path) =
        seed_live_thread(case, api_key, home.path(), &cwd, &sqlite_home)?;

    let log_dir = home.path().join("logs");
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("create log dir {}", log_dir.display()))?;
    let session_log_path = home
        .path()
        .join(format!("session-{}.jsonl", implementation.label()));
    let env = LiveTuiEnvironment {
        home: home.path(),
        cwd: &cwd,
        sqlite_home: &sqlite_home,
        log_dir: &log_dir,
        session_log_path: &session_log_path,
    };

    let pty_outcome =
        run_compact_resume_pty(case, implementation, api_key, env, &seed_rollout_path).await?;

    let rollout_path =
        resolved_manual_compact_rollout_path(env.session_log_path, env.home, &seed_rollout_path)?;

    persist_live_tui_artifacts(
        case,
        implementation,
        env,
        &seed_stdout,
        &seed_stderr,
        &pty_outcome.output,
        &rollout_path,
    )?;

    if let Some(err) = pty_outcome.error {
        return Err(err);
    }

    Ok(LiveTuiCompactResult {
        output: pty_outcome.output,
        seed_stdout,
        seed_stderr,
        session_log_path,
        rollout_path,
        _home: home,
        _cwd: cwd,
    })
}

fn write_live_tui_config(
    case: LiveProviderCase,
    implementation: LiveTuiImplementation,
    home: &Path,
    cwd: &Path,
) -> Result<()> {
    let model_provider = toml_string_literal(case.provider_id)?;
    let model = toml_string_literal(case.model)?;
    let trusted_project = toml_string_literal(cwd.to_string_lossy().as_ref())?;
    let config = format!(
        "model_provider = {model_provider}\nmodel = {model}\nfeatures.tui_app_server = {}\n\n[projects.{trusted_project}]\ntrust_level = \"trusted\"\n",
        implementation.uses_app_server()
    );
    fs::write(home.join("config.toml"), config)
        .with_context(|| format!("write config {}", home.join("config.toml").display()))?;
    append_home_config(home, "analytics.enabled = false\n")?;
    Ok(())
}

fn seed_live_thread(
    case: LiveProviderCase,
    api_key: &str,
    home: &Path,
    cwd: &Path,
    sqlite_home: &Path,
) -> Result<(String, String, PathBuf)> {
    let mut command = std::process::Command::new(resolve_codex_binary()?);
    command
        .current_dir(cwd)
        .env("CODEX_HOME", home)
        .env("CODEX_SQLITE_HOME", sqlite_home)
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
        .arg(format!("model={}", toml_string_literal(case.model)?))
        .arg("-C")
        .arg(cwd)
        .arg(MANUAL_COMPACT_SEED_PROMPT);
    for env_key in case.env_keys {
        command.env(env_key, api_key);
    }

    let output = command
        .output()
        .context("run codex exec seed for manual compact")?;
    let status = output.status;
    let stdout = String::from_utf8(output.stdout).context("seed stdout was not utf-8")?;
    let stderr = String::from_utf8(output.stderr).context("seed stderr was not utf-8")?;
    ensure!(
        status.success(),
        "manual compact seed for {} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        case.provider_id,
        status,
        stdout,
        stderr
    );

    let events = parse_jsonl_events(&stdout).with_context(|| {
        format!(
            "parse JSONL output for manual compact seed {}\nstderr:\n{}",
            case.provider_id, stderr
        )
    })?;
    let thread_id = find_thread_id(&events).with_context(|| {
        format!(
            "missing thread.started for manual compact seed {}",
            case.provider_id
        )
    })?;
    let rollout_path = find_rollout_path(home, &thread_id)?.with_context(|| {
        format!(
            "missing rollout for manual compact seed {}",
            case.provider_id
        )
    })?;
    Ok((stdout, stderr, rollout_path))
}

async fn run_compact_resume_pty(
    case: LiveProviderCase,
    implementation: LiveTuiImplementation,
    api_key: &str,
    env: LiveTuiEnvironment<'_>,
    rollout_path: &Path,
) -> Result<PtyRunOutcome> {
    let mut process_env = HashMap::new();
    process_env.insert("CODEX_HOME".to_string(), env.home.display().to_string());
    process_env.insert(
        "CODEX_SQLITE_HOME".to_string(),
        env.sqlite_home.display().to_string(),
    );
    process_env.insert("CODEX_TUI_RECORD_SESSION".to_string(), "1".to_string());
    process_env.insert(
        "CODEX_TUI_SESSION_LOG_PATH".to_string(),
        env.session_log_path.display().to_string(),
    );
    process_env.insert("RUST_LOG".to_string(), "trace".to_string());
    for env_key in case.env_keys {
        process_env.insert((*env_key).to_string(), api_key.to_string());
    }

    let args = vec![
        "resume".to_string(),
        "--last".to_string(),
        "--no-alt-screen".to_string(),
        "--dangerously-bypass-approvals-and-sandbox".to_string(),
        "-C".to_string(),
        env.cwd.display().to_string(),
        "-c".to_string(),
        format!(
            "log_dir={}",
            toml_string_literal(env.log_dir.to_string_lossy().as_ref())?
        ),
    ];
    let codex = resolve_codex_binary()?;
    let spawned = codex_utils_pty::spawn_pty_process(
        codex.to_string_lossy().as_ref(),
        &args,
        env.cwd,
        &process_env,
        &None,
        TerminalSize::default(),
    )
    .await
    .with_context(|| format!("spawn {} manual compact PTY", implementation.label()))?;

    let codex_utils_pty::SpawnedProcess {
        session,
        stdout_rx,
        stderr_rx,
        exit_rx,
    } = spawned;
    let mut output_rx = codex_utils_pty::combine_output_receivers(stdout_rx, stderr_rx);
    let mut exit_rx = exit_rx;
    let writer_tx = session.writer_sender();
    let mut output = Vec::new();
    let mut tick = interval(Duration::from_millis(150));
    let mut answered_cursor_query = false;
    let mut startup_ready_since = None;
    let mut compact_text_sent = false;
    let mut compact_enter_sent = false;
    let mut shutdown_requested = false;

    let exit_code_result = timeout(Duration::from_secs(120), async {
        loop {
            select! {
                result = output_rx.recv() => match result {
                    Ok(chunk) => {
                        let has_cursor_query = chunk.windows(4).any(|window| window == b"\x1b[6n");
                        if has_cursor_query {
                            let _ = writer_tx.send(b"\x1b[1;1R".to_vec()).await;
                            answered_cursor_query = true;
                        } else if answered_cursor_query
                            && startup_ready_since.is_none()
                            && session_log_contains_ready_for(
                                implementation,
                                &read_optional_file(env.session_log_path)?,
                            )
                        {
                            startup_ready_since = Some(Instant::now());
                        }
                        output.extend_from_slice(&chunk);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break exit_rx.await.map_err(anyhow::Error::from);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                },
                _ = tick.tick() => {
                    if answered_cursor_query
                        && startup_ready_since.is_none()
                        && session_log_contains_ready_for(
                            implementation,
                            &read_optional_file(env.session_log_path)?,
                        )
                    {
                        startup_ready_since = Some(Instant::now());
                    }
                    if startup_ready_since
                        .is_some_and(|ready_since| ready_since.elapsed() >= MANUAL_COMPACT_READY_DELAY)
                        && !compact_text_sent
                    {
                        for byte in b"/compact" {
                            writer_tx
                                .send(vec![*byte])
                                .await
                                .context("send /compact text")?;
                            sleep(MANUAL_COMPACT_KEYPRESS_DELAY).await;
                        }
                        compact_text_sent = true;
                        continue;
                    }
                    if compact_text_sent && !compact_enter_sent {
                        send_manual_compact_enter(&writer_tx, env.session_log_path).await?;
                        compact_enter_sent = true;
                        continue;
                    }
                    if compact_enter_sent
                        && manual_compact_completed(env.session_log_path, env.home, rollout_path)?
                        && !shutdown_requested
                    {
                        shutdown_requested = true;
                        for byte in MANUAL_COMPACT_EXIT_COMMAND {
                            writer_tx
                                .send(vec![*byte])
                                .await
                                .context("send /exit text after manual compact")?;
                            sleep(MANUAL_COMPACT_KEYPRESS_DELAY).await;
                        }
                        for (label, bytes) in [
                            ("CSI-u enter", b"\x1b[13u".as_slice()),
                            ("carriage return", b"\r".as_slice()),
                            ("newline", b"\n".as_slice()),
                        ] {
                            writer_tx
                                .send(bytes.to_vec())
                                .await
                                .with_context(|| format!("send /exit {label}"))?;
                            sleep(MANUAL_COMPACT_ENTER_DELAY).await;
                        }
                    }
                }
                result = &mut exit_rx => break result.map_err(anyhow::Error::from),
            }
        }
    })
    .await;

    while let Ok(chunk) = output_rx.try_recv() {
        output.extend_from_slice(&chunk);
    }

    let output = String::from_utf8_lossy(&output).to_string();
    let error = match exit_code_result {
        Ok(Ok(exit_code)) => validate_manual_compact_outcome(
            implementation,
            exit_code,
            &output,
            env.session_log_path,
            env.home,
            rollout_path,
        )
        .err(),
        Ok(Err(err)) => Some(err.context(format!(
            "manual compact PTY failed for {}\noutput:\n{}",
            implementation.label(),
            output
        ))),
        Err(err) => Some(anyhow::Error::new(err).context(format!(
            "manual compact PTY failed for {}\noutput:\n{}",
            implementation.label(),
            output
        ))),
    };

    Ok(PtyRunOutcome { output, error })
}

fn manual_compact_completed(
    session_log_path: &Path,
    home: &Path,
    rollout_path: &Path,
) -> Result<bool> {
    let session_log = read_optional_file(session_log_path)?;
    let current_rollout_path =
        resolved_rollout_path_from_session_log(&session_log, home, rollout_path);
    let rollout = read_optional_file(current_rollout_path.as_path())?;
    Ok(session_log_contains_compact_op(&session_log)
        && (session_log_contains_context_compacted_event(&session_log)
            || rollout_contains_context_compacted_event(&rollout))
        && rollout.contains("\"type\":\"compacted\""))
}

fn validate_manual_compact_outcome(
    implementation: LiveTuiImplementation,
    exit_code: i32,
    output: &str,
    session_log_path: &Path,
    home: &Path,
    rollout_path: &Path,
) -> Result<()> {
    let compact_completed = manual_compact_completed(session_log_path, home, rollout_path)?;
    let interrupted_after_compact = exit_code == 1 && compact_completed;
    ensure!(
        exit_code == 0 || exit_code == 130 || interrupted_after_compact,
        "unexpected exit code from {} manual compact PTY: {}\noutput:\n{}",
        implementation.label(),
        exit_code,
        output
    );
    ensure!(
        compact_completed,
        "{} manual compact did not record both /compact op and compacted rollout\noutput:\n{}",
        implementation.label(),
        output
    );
    Ok(())
}

fn read_optional_file(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).with_context(|| format!("read optional file {}", path.display()))
}

fn resolved_manual_compact_rollout_path(
    session_log_path: &Path,
    home: &Path,
    fallback_rollout_path: &Path,
) -> Result<PathBuf> {
    let session_log = read_optional_file(session_log_path)?;
    Ok(resolved_rollout_path_from_session_log(
        &session_log,
        home,
        fallback_rollout_path,
    ))
}

fn resolved_rollout_path_from_session_log(
    session_log: &str,
    home: &Path,
    fallback_rollout_path: &Path,
) -> PathBuf {
    session_log_rollout_path(session_log)
        .or_else(|| latest_materialized_rollout_path(&home.join("sessions")))
        .unwrap_or_else(|| fallback_rollout_path.to_path_buf())
}

fn latest_materialized_rollout_path(sessions_root: &Path) -> Option<PathBuf> {
    WalkDir::new(sessions_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.into_path();
            let is_rollout = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"));
            is_rollout.then_some(path)
        })
        .max()
}

fn rollout_contains_context_compacted_event(rollout: &str) -> bool {
    rollout.contains("\"type\":\"event_msg\",\"payload\":{\"type\":\"context_compacted\"}")
}

fn session_log_contains_ready_for(
    implementation: LiveTuiImplementation,
    session_log: &str,
) -> bool {
    match implementation {
        LiveTuiImplementation::Legacy => session_log_contains_startup_ready(session_log),
        LiveTuiImplementation::AppServer => {
            session_log_contains_app_server_startup_ready(session_log)
        }
    }
}

async fn send_manual_compact_enter(
    writer_tx: &tokio::sync::mpsc::Sender<Vec<u8>>,
    session_log_path: &Path,
) -> Result<()> {
    for (label, bytes) in [
        ("CSI-u enter", b"\x1b[13u".as_slice()),
        ("carriage return", b"\r".as_slice()),
        ("newline", b"\n".as_slice()),
    ] {
        writer_tx
            .send(bytes.to_vec())
            .await
            .with_context(|| format!("send /compact {label}"))?;
        sleep(MANUAL_COMPACT_ENTER_DELAY).await;
        if session_log_contains_compact_op(&read_optional_file(session_log_path)?) {
            break;
        }
    }
    Ok(())
}

fn persist_live_tui_artifacts(
    case: LiveProviderCase,
    implementation: LiveTuiImplementation,
    env: LiveTuiEnvironment<'_>,
    seed_stdout: &str,
    seed_stderr: &str,
    output: &str,
    rollout_path: &Path,
) -> Result<()> {
    let Some(root) = std::env::var_os(PROVIDER_LIVE_ARTIFACT_DIR_ENV_VAR) else {
        return Ok(());
    };

    let artifact_dir = PathBuf::from(root)
        .join(case.provider_id)
        .join(implementation.label());
    fs::create_dir_all(&artifact_dir).with_context(|| {
        format!(
            "create provider live TUI artifact dir {}",
            artifact_dir.display()
        )
    })?;
    copy_live_home_artifacts(env.home, &artifact_dir.join("home"))?;
    fs::write(
        artifact_dir.join("cwd-path.txt"),
        env.cwd.display().to_string(),
    )
    .context("write provider live TUI cwd artifact")?;
    fs::write(artifact_dir.join("seed-stdout.jsonl"), seed_stdout)
        .context("write provider live TUI seed stdout artifact")?;
    fs::write(artifact_dir.join("seed-stderr.txt"), seed_stderr)
        .context("write provider live TUI seed stderr artifact")?;
    fs::write(artifact_dir.join("pty-output.txt"), output)
        .context("write provider live TUI PTY output artifact")?;
    if env.session_log_path.exists() {
        fs::copy(env.session_log_path, artifact_dir.join("session-log.jsonl"))
            .context("copy provider live TUI session log artifact")?;
    }
    if rollout_path.exists() {
        fs::copy(rollout_path, artifact_dir.join("active-rollout.jsonl"))
            .context("copy provider live TUI rollout artifact")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn app_server_label_is_stable() {
        assert_eq!(
            LiveTuiImplementation::AppServer.label(),
            "manual-compact-app-server"
        );
    }

    #[test]
    fn tui_live_workflows_cover_all_tui_required_provider_features() {
        let required = ProviderFeature::ALL
            .into_iter()
            .filter(|feature| feature.live_coverage().requires_tui_live_smoke())
            .collect::<std::collections::BTreeSet<_>>();
        let covered = TUI_LIVE_COVERED_FEATURES
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(covered, required);
    }
}
