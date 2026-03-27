# codex-cn CLI (Rust Implementation)

We provide `codex-cn` as a standalone, native executable to ensure a zero-dependency install.

## Installing codex-cn

Today, the easiest way to install `codex-cn` is via `npm`:

```shell
npm i -g @keepkeen/codex-cn
codex-cn
```

You can also download a platform-specific fork release directly from [GitHub Releases](https://github.com/keepkeen/codex_adapter_CN/releases).

> The internal Cargo binary target is still named `codex`, but the user-facing command, package names, docs, and release assets in this fork are all `codex-cn`.

## Documentation quickstart

- First run with `codex-cn`? Start with [`docs/getting-started.md`](../docs/getting-started.md) (links to the walkthrough for prompts, keyboard shortcuts, and session management).
- Want deeper control? See [`docs/config.md`](../docs/config.md) and [`docs/install.md`](../docs/install.md).

## What's new in the Rust CLI

The Rust implementation is now the maintained `codex-cn` CLI and serves as the default experience. It includes a number of features that the legacy TypeScript CLI never supported.

### Config

`codex-cn` supports a rich set of configuration options. Note that the Rust CLI uses `config.toml` instead of `config.json`. See [`docs/config.md`](../docs/config.md) for details.

### Model Context Protocol Support

#### MCP client

`codex-cn` functions as an MCP client that allows the CLI and IDE extension to connect to MCP servers on startup. See the [`configuration documentation`](../docs/config.md#connecting-to-mcp-servers) for details.

#### MCP server (experimental)

`codex-cn` can be launched as an MCP _server_ by running `codex-cn mcp-server`. This allows _other_ MCP clients to use `codex-cn` as a tool for another agent.

Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector codex-cn mcp-server
```

Use `codex-cn mcp` to add/list/get/remove MCP server launchers defined in `config.toml`, and `codex-cn mcp-server` to run the MCP server directly.

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](../docs/config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS. When `codex-cn` detects that it is running under WSL 2 inside Windows Terminal (`WT_SESSION` is set), the TUI automatically falls back to native Windows toast notifications so approval prompts and completed turns surface even though Windows Terminal does not implement OSC 9.

### `codex-cn exec` to run codex-cn programmatically/non-interactively

To run `codex-cn` non-interactively, run `codex-cn exec PROMPT` (you can also pass the prompt via `stdin`) and `codex-cn` will work on your task until it decides that it is done and exits. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.
Use `codex-cn exec --ephemeral ...` to run without persisting session rollout files to disk.

### Experimenting with the Codex Sandbox

To test to see what happens when a command is run under the sandbox provided by `codex-cn`, we provide the following subcommands in the Rust CLI:

```
# macOS
codex-cn sandbox macos [--full-auto] [--log-denials] [COMMAND]...

# Linux
codex-cn sandbox linux [--full-auto] [COMMAND]...

# Windows
codex-cn sandbox windows [--full-auto] [COMMAND]...

# Legacy aliases
codex-cn debug seatbelt [--full-auto] [--log-denials] [COMMAND]...
codex-cn debug landlock [--full-auto] [COMMAND]...
```

### Selecting a sandbox policy via `--sandbox`

The Rust CLI exposes a dedicated `--sandbox` (`-s`) flag that lets you pick the sandbox policy **without** having to reach for the generic `-c/--config` option:

```shell
# Run codex-cn with the default, read-only sandbox
codex-cn --sandbox read-only

# Allow the agent to write within the current workspace while still blocking network access
codex-cn --sandbox workspace-write

# Danger! Disable sandboxing entirely (only do this if you are already running in a container or other isolated env)
codex-cn --sandbox danger-full-access
```

The same setting can be persisted in `~/.codex-cn/config.toml` via the top-level `sandbox_mode = "MODE"` key, e.g. `sandbox_mode = "workspace-write"`.
In `workspace-write`, `codex-cn` also includes `~/.codex-cn/memories` in its writable roots so memory maintenance does not require an extra approval.

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for the agent. Ultimately, we hope this to be a library crate that is generally useful for building other Rust/native applications that use the same workflows.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.

If you want to contribute or inspect behavior in detail, start by reading the module-level `README.md` files under each crate and run the project workspace from the top-level `codex-rs` directory so shared config, features, and build scripts stay aligned.
