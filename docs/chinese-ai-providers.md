# Chinese AI Providers

This fork exposes its user-facing CLI as `codex-cn`, includes built-in adapters for four Chinese model families, and routes them through the OpenAI-compatible `chat/completions` path instead of the OpenAI Responses API.

The details below reflect the implementation and the official provider docs reviewed for March 25, 2026.

## Naming And Isolation

- User-facing CLI name: `codex-cn`
- npm package: `@keepkeen/codex-cn`
- TypeScript SDK package: `@keepkeen/codex-cn-sdk`
- Responses API proxy package: `@keepkeen/codex-cn-responses-api-proxy`
- Default home for packaged `codex-cn`: `~/.codex-cn`

The internal Cargo binary target is still named `codex`, but source builds should expose it as `codex-cn` and set `CODEX_HOME="$HOME/.codex-cn"` plus `CODEX_SQLITE_HOME="$HOME/.codex-cn/sqlite"` explicitly. Do not let this fork share `~/.codex` with an upstream install.

## Built-in Providers

### DeepSeek

- Provider ID: `deepseek`
- Base URL: `https://api.deepseek.com`
- API key env: `DEEPSEEK_API_KEY`
- Built-in models:
  - `deepseek-chat`
  - `deepseek-reasoner`
- Notes:
  - Uses HTTP streaming, not websockets.
  - `deepseek-reasoner` is the official thinking-mode alias.
  - Legacy compatibility aliases `deepseek-chat-thinking` and `deepseek-thinking` are still accepted internally and mapped onto DeepSeek thinking mode.

### GLM

- Provider ID: `glm`
- Base URL: `https://open.bigmodel.cn/api/paas/v4`
- API key env: `GLM_API_KEY`
- Built-in models:
  - `glm-5`
  - `glm-4.6v`
- Notes:
  - `glm-5` is used as the default text flagship.
  - `glm-4.6v` is exposed as the built-in multimodal option.

### Kimi / Moonshot

- Provider ID: `kimi`
- Base URL: `https://api.moonshot.cn/v1`
- API key env: `MOONSHOT_API_KEY`
- Legacy env fallback: `KIMI_API_KEY`
- Built-in models:
  - `kimi-k2.5`
- Notes:
  - The built-in catalog now tracks the official `kimi-k2.5` model id directly.
  - The adapter treats Kimi as chat-completions compatible and exposes image input metadata for the built-in model.

### MiniMax

- Provider ID: `minimax`
- Base URL: `https://api.minimaxi.com/v1`
- API key env: `MINIMAX_API_KEY`
- Built-in models:
  - `MiniMax-M2.7`
  - `MiniMax-M2.7-highspeed`
- Notes:
  - The adapter enables MiniMax `reasoning_split=true` so reasoning traces are preserved across tool loops.
  - MiniMax passive prompt caching works on the OpenAI-compatible endpoint without extra request flags; the adapter keeps tools top-level, hoists late `system` / `developer` guidance back to the head of the transcript, and appends newer user/tool items after the stable prefix to preserve cache-friendly ordering.
  - `usage.prompt_tokens_details.cached_tokens` is parsed and stored as `cached_input_tokens` so turn metrics and telemetry can distinguish cache hits from fresh prompt input.

## Quick Start

```toml
model_provider = "deepseek"
model = "deepseek-chat"
```

Other built-in combinations:

```toml
model_provider = "deepseek"
model = "deepseek-reasoner"
```

```toml
model_provider = "glm"
model = "glm-5"
```

```toml
model_provider = "kimi"
model = "kimi-k2.5"
```

```toml
model_provider = "minimax"
model = "MiniMax-M2.7"
```

## Custom Endpoints

Built-in provider IDs are reserved. If you need a proxy, gateway, or custom headers, create a new provider ID and point `model_provider` at that custom entry:

```toml
model_provider = "deepseek-proxy"
model = "deepseek-chat"

[model_providers.deepseek-proxy]
name = "DeepSeek via proxy"
base_url = "https://your-proxy.example.com/v1"
env_key = "DEEPSEEK_API_KEY"
wire_api = "chat"
```

For custom providers that are not one of the built-in IDs, Codex can still talk to the endpoint, but the best built-in model metadata is only guaranteed for the reserved built-in provider IDs.

## Behavioral Notes

- These providers use HTTP SSE transport. Websocket prewarm is disabled.
- `output_schema` is emulated for the four built-in chat providers, but not for generic custom chat providers:
  - DeepSeek / GLM / Kimi request `response_format = {"type":"json_object"}` and then run strict local JSON Schema validation plus bounded repair retries.
  - MiniMax keeps the provider request plain-text and relies on the same strict local JSON Schema validation plus bounded repair retries.
  - The outward contract is still “schema-compliant JSON or a hard failure”; Codex does not silently downgrade these requests to ordinary text output.
- Function tools, shell, and `apply_patch` are re-exposed through chat-completions-compatible function calling.
- `js_repl` is emulated for the four built-in chat providers through a function-tool wrapper:
  - The public tool names stay `js_repl` / `js_repl_reset`.
  - Chat providers send the full JavaScript program in the `code` field, with optional `timeout_ms`.
  - OpenAI / Responses providers continue to use the native freeform/custom-tool transport.
- Token usage prefers provider-reported streaming `usage`; when a provider omits it, Codex falls back to local token estimation so the footer and auto-compact thresholds still move.
- Kimi can emit `usage` on the final `choice` or in a trailing usage-only chunk, and MiniMax can emit intermediate `usage: null` plus a trailing usage chunk; the SSE adapter now waits for the trailing usage and ignores empty/null usage payloads.
- MiniMax requires `system` instructions to stay at the head of the conversation; late `system` / `developer` guidance is hoisted and merged into the leading system message on the chat adapter path.
- Interleaved tool use is preserved by replaying the full assistant tool-call turn back through the chat adapter, including tool-call ids and provider reasoning fields such as `reasoning_content` / `reasoning_details`.
- Subagents are still implemented as ordinary collaboration/function tools on this path; live smoke tests now pass on DeepSeek, GLM, Kimi, and MiniMax.
- `web_search="live"` / `--search` is re-exposed on chat providers through a local function-backed adapter that supports `search`, `open_page`, and `find_in_page`.
- The local web-search adapter now performs bounded query expansion, URL normalization and deduplication, per-host diversity limiting, focused passage extraction for `open_page`, and source-aware parsing for GitHub/docs pages.
- GitHub blob URLs are fetched through `?raw=1`, and GitHub README/release pages plus common docs frameworks prefer article/main markdown content over page chrome.
- The chat-provider adapter is still not identical to the native OpenAI Responses `web_search`: cached mode, image search, and native web-search event semantics are not preserved.
- The current implementation still does not provide specialized PDF parsing, code-search indexes, or cross-engine reranking, and fallback token estimates are approximate rather than billing-accurate.
- `artifacts` and `image_generation` remain unsupported on this chat-provider compatibility path.
- DeepSeek, Kimi, and MiniMax reasoning traces are preserved when the provider exposes them through `reasoning_content` or `reasoning_details`.
- Kimi / GLM / MiniMax can still be more tool-happy than the native OpenAI path in long agent loops; the transport is compatible, but Kimi in particular has shown weaker stability on long skill prompts and can hit rate limits sooner.

## Provider Live Smoke Matrix

The repo now includes a reusable ignored live-test harness for the built-in Chinese providers:

- Helper module: `codex-rs/core/tests/common/provider_live.rs`
- PTY / TUI compact helper: `codex-rs/core/tests/common/provider_live_tui.rs`
- Integration matrix: `codex-rs/core/tests/suite/provider_live_matrix.rs`
- Legacy TUI compact matrix: `codex-rs/tui/tests/suite/provider_live_compact.rs`
- App-server TUI compact matrix: `codex-rs/tui_app_server/tests/suite/provider_live_compact.rs`
- CI entrypoint: `.github/workflows/provider-live-matrix.yml`

Current automated live workflows:

- `basic-shell`: runs `codex-cn exec --json` against a built-in provider, requires the model to use the shell tool once, and asserts both the `command_execution` item and the final agent message.
- `web-search`: runs `codex-cn exec --json` with `web_search="live"`, requires a real `web_search` item, and asserts the canonical `https://github.com/openai/codex` URL appears in the final agent message.
- `skill`: writes a temporary skill into isolated `CODEX_HOME/skills`, prompts the model to use `$provider-live-demo`, and asserts the final agent message contains a skill-only sentinel.
- `mcp-echo`: writes a temporary stdio MCP config backed by `test_stdio_server`, asserts `mcp_tool_call` events for `rmcp/echo`, and checks the echoed sentinel in both structured content and the final reply.
- `subagent`: enables `features.multi_agent`, asserts completed `collab_tool_call` items for `spawn_agent` and `wait`, and checks the delegated sentinel plus non-empty receiver thread ids.
- `js-repl`: uses a persistent `codex-cn exec` thread, proves `js_repl` state survives one `resume`, then proves `js_repl_reset` clears that state before the final `resume`.
- `output-schema`: runs `codex-cn exec --json --output-schema ...` with a strict JSON Schema, and asserts the final agent message is schema-compliant JSON rather than plain text.
- `auto-compact`: uses a persistent `codex-cn exec` turn followed by `codex-cn exec resume`, keeps `model_auto_compact_token_limit` intentionally low, and proves real compaction via the persisted rollout `type:"compacted"` entry.
- `image-input`: uses `codex-cn exec --json --image ...`, keeps the thread persistent so rollout artifacts are available, and only runs for built-in providers whose feature surface declares native image input support.
- `manual-compact-legacy`: seeds a persistent thread with `codex-cn exec`, then resumes it through a real PTY-backed legacy TUI session, submits `/compact`, and proves the slash command via session-log `Op::Compact` plus rollout `type:"compacted"`.
- `manual-compact-app-server`: the same PTY-backed `/compact` smoke, but forces `features.tui_app_server = true` and verifies the app-server TUI path emits the same compact evidence.

Local runs stay isolated by creating a temporary `CODEX_HOME` / `CODEX_SQLITE_HOME` for each test case. If you also have an upstream `codex` install, keep this fork on a separate command name and a separate home directory so the two installs never share state.

For normal end-user installs, `codex-cn` now defaults to `~/.codex-cn` when it is launched through the npm wrapper or a renamed standalone binary and `CODEX_HOME` is unset. Source builds that still invoke the internal `codex` binary should keep exporting `CODEX_HOME="$HOME/.codex-cn"` explicitly.

To run one locally:

```bash
cd codex-rs
export DEEPSEEK_API_KEY=...
cargo test -p codex-core --test all \
  'suite::provider_live_matrix::deepseek_exec_live_basic_shell_smoke' \
  -- --ignored --exact --nocapture
```

To run the interactive `/compact` smoke for the legacy TUI:

```bash
cd codex-rs
export DEEPSEEK_API_KEY=...
cargo test -p codex-tui --test all \
  'suite::provider_live_compact::deepseek_live_manual_compact_smoke' \
  -- --ignored --exact --nocapture
```

To run the app-server TUI variant:

```bash
cd codex-rs
export DEEPSEEK_API_KEY=...
cargo test -p codex-tui-app-server --test all \
  'suite::provider_live_compact::deepseek_app_server_live_manual_compact_smoke' \
  -- --ignored --exact --nocapture
```

The GitHub Actions workflow uses the same test entrypoints and accepts these secrets:

- `DEEPSEEK_API_KEY`
- `GLM_API_KEY`
- `MOONSHOT_API_KEY`
- `MINIMAX_API_KEY`

Set `CODEX_PROVIDER_LIVE_ARTIFACT_DIR` when running locally or in CI if you want the harness to copy the temporary `CODEX_HOME`, `cwd`, stdout JSONL, and stderr into a stable artifact directory.

The matrix is now wired into `.github/workflows/provider-live-matrix.yml` for manual and scheduled runs. `js_repl`, `output-schema`, and image input all stay on the `exec --json` harness, while interactive `/compact` lives on the PTY/TUI harness; future live workflows should extend one of those two families rather than reintroducing ad-hoc shell scripts.

## Current Validation Summary

- Targeted Rust tests passed for the fork-specific changes:
  - `cargo test -p codex-protocol --lib`
  - `cargo test -p codex-api --lib`
  - `cargo test -p codex-core --lib`
  - `cargo test -p codex-utils-home-dir`
  - `cargo test -p codex-cli`
  - `cargo test -p codex-tui`
  - `cargo test -p codex-tui-app-server`
- Fork-specific lint checks passed:
  - `just argument-comment-lint -p codex-core`
  - `just argument-comment-lint -p codex-cli`
  - `just argument-comment-lint -p codex-tui`
  - `just argument-comment-lint -p codex-tui-app-server`
  - `just argument-comment-lint -p codex-utils-home-dir`
- Package staging checks passed:
  - `npm pack --dry-run` in `codex-cli`
  - `npm pack --dry-run` in `sdk/typescript`
  - `npm pack --dry-run` in `codex-rs/responses-api-proxy/npm`
