# codex-cn 中国模型适配说明

`codex-cn` 是这个 fork 的外部命令名。它保留了 Codex CLI 的核心工作流，但把面向中国模型的接入方式改成了更稳定的 `chat/completions` 兼容层，并补齐了搜索、token 计数、compact、skills、MCP 和子代理这几条主路径。

## 对外发布名

- CLI 命令名：`codex-cn`
- npm 包：`@keepkeen/codex-cn`
- TypeScript SDK：`@keepkeen/codex-cn-sdk`
- Responses API proxy：`@keepkeen/codex-cn-responses-api-proxy`

Rust workspace 里的内部二进制目标名仍然是 `codex`。这是为了减少对 upstream 构建图的侵入式修改。对外安装、命令、文档和 release 资产全部统一成 `codex-cn`。

## 这个 fork 改了什么

- 外部命令名按 `codex-cn` 来使用，不再要求你把它当成原版 `codex`。
- 内置了 4 家 provider：
  - `deepseek`
  - `glm`
  - `kimi`
  - `minimax`
- 内置模型按当前代码库的模型目录维护：
  - DeepSeek: `deepseek-chat`, `deepseek-reasoner`
  - GLM: `glm-5`, `glm-4.6v`
  - Kimi: `kimi-k2.5`
  - MiniMax: `MiniMax-M2.7`, `MiniMax-M2.7-highspeed`
- 请求和流式响应都走 provider profile，不再散落在多处 `if provider == ...` 分支里。
- chat provider 的 token usage 会优先吃厂商流式 `usage`，拿不到时回退到本地估算，所以 footer 和 auto compact 仍然会动。
- `web_search="live"` / `--search` 已改成本地 function adapter，不是 OpenAI 原生 Responses `web_search`。
- `js_repl` 和 `output_schema` 在四家内置 chat provider 下已做成 provider-aware 兼容层，不再一刀切禁用。

## 当前适配状态

### 已稳定可用

- 基础对话
- shell / apply_patch 类常规工具调用
- `skills`
- `MCP`
- 子代理
- `auto compact`
- `manual /compact` 的 PTY/TUI smoke 路径
- `web_search="live"` 的本地搜索适配层
- `js_repl`
- `output_schema`

### 分模型说明

- `deepseek-chat`
  - 文本模型
  - 支持基本对话、工具调用、skills、MCP、子代理、搜索、compact
- `deepseek-reasoner`
  - 文本模型
  - 适合需要 reasoning 的场景
- `glm-5`
  - 文本模型
  - 支持基本对话、工具调用、skills、MCP、子代理、搜索、compact
- `glm-4.6v`
  - 图像输入模型
  - 适合带截图、界面图、示意图的场景
- `kimi-k2.5`
  - 图像输入模型
  - 适合多模态任务和代码工作流
- `MiniMax-M2.7`
  - 文本模型
  - 适合常规编码与 agent workflow
- `MiniMax-M2.7-highspeed`
  - 文本模型
  - 与 `MiniMax-M2.7` 保持同一适配策略

## 重要限制

- 这 4 家 provider 都走 HTTP SSE，不走 websocket 预热。
- 当前仍不是 OpenAI 原生 Responses 路径的完整等价实现。
- `web_search` 现在是本地搜索适配层，支持 `search`、`open_page`、`find_in_page`，但不等价于 OpenAI 原生 `web_search`。
- `output_schema`、`js_repl` 虽然已经可用，但属于兼容实现，不是厂商原生协议直通。
- `artifacts`、`image_generation` 目前仍不是这 4 家 provider 的已支持路径。
- `image_input` 只对 `glm-4.6v` 和 `kimi-k2.5` 是原生可用。
- 当 provider 没返回精确 usage 时，token 计数和 auto compact 依赖本地估算。
- `MiniMax` 的 cached token 解析已做，但 footer 不会把它单独拆成独立显示项。

## 如何使用

最小配置示例：

```toml
model_provider = "deepseek"
model = "deepseek-chat"
```

常见组合：

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

对应环境变量：

```bash
export DEEPSEEK_API_KEY="..."
export GLM_API_KEY="..."
export MOONSHOT_API_KEY="..."
export MINIMAX_API_KEY="..."
```

联网搜索：

```bash
codex-cn --search
```

或者在配置里开启：

```toml
web_search = "live"
```

图像输入：

```bash
codex-cn -i ./screen.png
```

或者在程序化调用里传图片输入项。当前只有 `glm-4.6v` 和 `kimi-k2.5` 真正支持图像输入。

## 如何避免污染原来的 `codex`

不要让 `codex-cn` 和上游 `codex` 共用命令、配置和数据库。

推荐这样隔离：

```bash
alias codex-cn='/path/to/codex-cn'
export CODEX_HOME="$HOME/.codex-cn"
export CODEX_SQLITE_HOME="$CODEX_HOME/sqlite"
```

这样这套 fork 只会读写自己的配置和会话数据库，不会碰到你原来的 `~/.codex`。

现在这套 fork 在两种场景下会默认走 `~/.codex-cn`：

- 通过 npm 安装并运行 `codex-cn`
- 直接运行重命名后的 `codex-cn` 原生二进制

如果你是从源码直接执行内部二进制 `codex`，仍建议显式设置上面的两个环境变量。

一个稳妥的本地安装方式是：

```bash
cd codex-rs
cargo build --release -p codex-cli
mkdir -p "$HOME/.local/bin"
cp target/release/codex "$HOME/.local/bin/codex-cn"
CODEX_HOME="$HOME/.codex-cn" CODEX_SQLITE_HOME="$HOME/.codex-cn/sqlite" codex-cn
```

## 测试结果

当前仓库已经把中国 provider 的能力收敛进统一的 live matrix，并补了 PTY/TUI smoke。已覆盖的自动化面包括：

- `basic-shell`
- `web-search`
- `skill`
- `mcp-echo`
- `subagent`
- `js-repl`
- `output-schema`
- `auto-compact`
- `image-input`
- `manual-compact-legacy`
- `manual-compact-app-server`

本地没有注入 provider key 时，这些测试会自动 skip；有真实 key 时可以直接按 [docs/chinese-ai-providers.md](./docs/chinese-ai-providers.md) 里的命令跑。

和这轮适配直接相关的定向验证已通过：

- `cargo test -p codex-protocol --lib`
- `cargo test -p codex-api --lib`
- `cargo test -p codex-core --lib`
- `cargo test -p core_test_support provider_live --lib`
- `cargo test -p core_test_support provider_live_tui --lib`

另外，和 fork 对外发布/命名直接相关的验证也已跑过：

- `cargo test -p codex-utils-home-dir`
- `cargo test -p codex-cli`
- `cargo test -p codex-tui`
- `cargo test -p codex-tui-app-server`
- `just argument-comment-lint -p codex-core`
- `just argument-comment-lint -p codex-cli`
- `just argument-comment-lint -p codex-tui`
- `just argument-comment-lint -p codex-tui-app-server`
- `just argument-comment-lint -p codex-utils-home-dir`
- `npm pack --dry-run`:
  - `codex-cli`
  - `sdk/typescript`
  - `codex-rs/responses-api-proxy/npm`

当前仍有一个和代码无关的外部发布阻塞：

- `npm whoami --registry=https://registry.npmjs.org/` 返回 `ENEEDAUTH`
- 这台机器目前无法直接把 `@keepkeen/*` 包发布到 npm，需要先登录 npm 或使用已配置好的 trusted publishing 流程

## Release 发布

这个 fork 现在已经有可直接使用的 GitHub Release 自动上传链路：

- 触发方式 1：push 一个符合规则的 tag，例如 `rust-v0.1.0`
- 触发方式 2：在 GitHub Actions 里手动运行 `create-release-tag`，输入和 `codex-rs/Cargo.toml` 一致的版本号
- 真正负责构建和上传资产的 workflow 是 `.github/workflows/rust-release.yml`

首个正式 release 版本现在定为 `0.1.0`。GitHub Release 会自动上传：

- `codex-cn-<target>`
- `codex-cn-<target>.tar.gz`
- `codex-cn-<target>.dmg`（macOS）
- `codex-cn-responses-api-proxy-<target>`
- `config-schema.json`
- `install.sh`
- `install.ps1`
- 对应的 npm tarballs

为了避免 fork 的 GitHub Release 被 npm 首发权限问题拖红，`rust-release.yml` 现在对 fork 默认关闭 npm 发布：

- upstream `openai/codex` 仍保持原行为
- 这个 fork 只有在仓库变量 `ENABLE_NPM_PUBLISH=true` 时才会继续执行 `publish-npm`
- 这意味着 GitHub Release 资产上传现在可以独立成功

如果以后你把 `@keepkeen/*` 的 bootstrap publish 和 trusted publishing 都配好了，再到仓库 Settings 里加：

```text
ENABLE_NPM_PUBLISH=true
```

之后新的 release tag 就会继续走自动 npm 发布。

## 补充说明

- DeepSeek 旧别名 `deepseek-chat-thinking` / `deepseek-thinking` 仍兼容。
- Kimi 仍兼容旧环境变量 `KIMI_API_KEY`，但官方主变量是 `MOONSHOT_API_KEY`。
- 如果你要走代理、网关或自定义 header，不要覆盖内置 provider ID，应该新建一个自定义 provider。
- SDK 仍然保留 `Codex` 这个 TypeScript 类名以兼容现有 API；对外包名和 CLI 名已经统一改成 `codex-cn`。

更完整的 provider、测试和工作流说明见 [docs/chinese-ai-providers.md](./docs/chinese-ai-providers.md)。
