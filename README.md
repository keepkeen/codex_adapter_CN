<p align="center"><code>npm i -g @keepkeen/codex-cn</code><br />or install the <code>codex-cn</code> binary from a release build</p>
<p align="center"><strong>codex-cn</strong> is a local coding agent forked from the upstream Codex CLI and adapted for Chinese provider backends.</p>
<p align="center">
  <img src="./.github/codex-cli-splash.png" alt="codex-cn CLI splash" width="80%" />
</p>
</br>
Upstream OpenAI Codex docs still live at <a href="https://developers.openai.com/codex">developers.openai.com/codex</a>.
</br>If you want the upstream OpenAI desktop app experience, run <code>codex-cn app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>


### Using codex-cn with your ChatGPT plan

Run `codex-cn` and select **Sign in with ChatGPT** if you want to use the upstream OpenAI account flow. We recommend signing into your ChatGPT account to use `codex-cn` as part of your Plus, Pro, Team, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use `codex-cn` with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

## 中文适配说明

这个 fork 以 `codex-cn` 作为外部命令名，额外适配了 4 家中国模型 provider，并统一走官方更稳定的 OpenAI 兼容 `chat/completions` 路线，而不是把这些 provider 伪装成 OpenAI `responses`。

- 对外发布名：
  - CLI：`codex-cn`
  - npm：`@keepkeen/codex-cn`
  - TypeScript SDK：`@keepkeen/codex-cn-sdk`
  - Responses API proxy：`@keepkeen/codex-cn-responses-api-proxy`
- 内部 Cargo 二进制目标仍叫 `codex`，这是为了尽量少改 upstream Rust workspace；对终端用户暴露的命令、安装包、README 和 release 资产统一叫 `codex-cn`
- 已适配 provider：`deepseek`、`glm`、`kimi`、`minimax`
- 内置模型：
  - DeepSeek：`deepseek-chat`、`deepseek-reasoner`
  - GLM：`glm-5`、`glm-4.6v`
  - Kimi：`kimi-k2.5`
  - MiniMax：`MiniMax-M2.7`、`MiniMax-M2.7-highspeed`
- 兼容原理：
  - 把 Codex 的工具调用和对话历史重编码成 chat-completions 兼容格式
  - 针对 DeepSeek / Kimi / MiniMax 保留各家的 reasoning 字段
  - 按 provider 载入独立模型目录，避免错误回退到 OpenAI 元数据
  - chat provider 能拿到流式 usage 时直接回写 token 计数；拿不到时回退到本地 token 估算，恢复 footer token 刷新和自动 compact 判断
  - MiniMax 的被动 Prompt cache 不需要额外开关；适配层会尽量维持缓存友好的前缀顺序，并解析 `cached_tokens`
  - 兼容旧别名 `deepseek-chat-thinking` / `deepseek-thinking`，以及旧环境变量 `KIMI_API_KEY`
- 与原仓库不同的点：
  - 上游默认围绕 OpenAI / ChatGPT 登录和 `responses` 路径；这个 fork 为 4 家中国 provider 提供了内置 provider 和内置模型目录
  - 中国 provider 统一改走 `chat/completions` + SSE 兼容层，不依赖 OpenAI 的远端 `responses` / realtime 路径
  - 非 OpenAI provider 的上下文压缩走本地 compact 流程，不走 OpenAI 远端 compact task
  - 模型元数据改为按当前 provider 加载，不再把 DeepSeek / GLM / Kimi / MiniMax 回退到 OpenAI 模型元数据
- 最小使用方式：

```toml
model_provider = "deepseek"
model = "deepseek-chat"
```

- 需要设置对应环境变量：`DEEPSEEK_API_KEY`、`GLM_API_KEY`、`MOONSHOT_API_KEY`、`MINIMAX_API_KEY`
- 如果要在这 4 家 provider 下启用联网搜索，配置或命令行里开启 `web_search="live"` / `--search` 即可；chat provider 会改走本地 `web_search` function 适配层，支持 `search`、`open_page`、`find_in_page`
- 这个本地搜索适配层已经额外做了检索增强：
  - 会对长查询做轻量关键词扩展
  - 会对结果 URL 做归一化、去重，并限制单一站点刷屏
  - `open_page` 支持传 `focus`，按问题抽取更相关的网页片段，而不是只截页头
  - 会优先按 GitHub README / release / blob 和常见文档站正文抽取内容，尽量避开导航栏、侧边栏和站点 chrome
  - 页面 fragment 会作为隐式 `focus`，例如 `#installation` 会优先抽取安装段落
- Prompt / Agent 工作流：
  - `orchestrator`、`collaboration`、`compact`、`memories` 这些核心 prompt 基本通用，不需要为四家 provider 单独改写
  - 子代理、MCP、绝大多数 skills 仍可用，因为它们最终走 function tools；四家 provider 已用真实 key 做过子代理 smoke test
- 已验证的自动化面：
  - 单元/集成：`codex-protocol`、`codex-api`、`codex-core`
  - 真实 provider live smoke：`basic-shell`、`web-search`、`skill`、`mcp-echo`、`subagent`、`js-repl`、`output-schema`、`auto-compact`
  - 真实 PTY/TUI smoke：`manual-compact-legacy`、`manual-compact-app-server`
  - 图像输入 smoke：`glm-4.6v`、`kimi-k2.5`
- 当前限制：
  - 这 4 家都走 HTTP SSE，不走 websocket 预热
  - `output_schema` 现在在内置四家 chat provider 下已接入严格兼容层：DeepSeek / GLM / Kimi 会请求 `json_object` 再做本地 JSON Schema 校验与修复，MiniMax 走普通文本输出再做同样的本地严格校验；对外语义是“要么返回符合 schema 的 JSON，要么明确失败”。但 generic 自定义 chat provider 仍不会自动开启这条路径
  - `js_repl` 现在在内置四家 chat provider 下已接入兼容层：公开工具名保持 `js_repl` / `js_repl_reset`，但 chat transport 会改成 function-tool wrapper，完整 JavaScript 需要放进 `code` 字段；OpenAI/Responses 路径继续保留原生 freeform/custom-tool 语义
  - 当前 chat 兼容层仍不是和 OpenAI 原生路径完全等价：artifact 类工具、`image_generation`、并行 tool calls 仍有限制；`web_search="live"` 现在会通过本地 function adapter 提供文本搜索/开页/页内查找，但不是 OpenAI 原生 Responses `web_search`，也不支持 cached mode、image search、专门的 PDF / 代码索引后端
  - 当 provider 不返回精确 usage 时，token 计数和 auto compact 依赖本地估算值；这能恢复行为，但不是供应商返回的精确计费数字
  - footer 的 `context left` 百分比仍沿用上游的 baseline 算法；当 compact 后百分比仍显示 `100%` 时，现在会同时显示压缩后的已占用 tokens，避免误判为上下文被清零
  - 图像输入当前内置给 `glm-4.6v` 和 `kimi-k2.5`
  - `deepseek-chat`、`deepseek-reasoner`、`glm-5`、`MiniMax-M2.7`、`MiniMax-M2.7-highspeed` 按当前适配视为文本模型
  - 如果要走代理、自定义 header 或网关，不要覆盖内置 provider ID；新建自定义 provider，并把 `wire_api = "chat"`

如果你已经有一套上游 `codex` 安装，请把这个 fork 作为独立命令和独立状态目录来用，避免互相污染：

```bash
alias codex-cn='/path/to/codex-cn'
export CODEX_HOME="$HOME/.codex-cn"
export CODEX_SQLITE_HOME="$CODEX_HOME/sqlite"
```

The packaged `codex-cn` launcher and the renamed standalone binary now default to `~/.codex-cn` when `CODEX_HOME` is unset. Explicitly exporting the two variables above is still the safest option for source builds, wrappers, and CI.

### Release automation

This fork now publishes GitHub Release assets automatically from `.github/workflows/rust-release.yml` when you push a tag like `rust-v0.1.0`. To avoid remembering the tag format by hand, the repo also includes `.github/workflows/create-release-tag.yml`, which can be run manually from GitHub Actions and will create/push the matching `rust-v<version>` tag for you.

For fork safety, GitHub Release asset upload is now decoupled from npm publication. Releases are created normally on this fork, but npm publication only runs when the repository variable `ENABLE_NPM_PUBLISH=true` is set after you have finished the bootstrap/trusted-publishing setup for `@keepkeen/*`.

隔离原则：

- 不要用 `codex-cn` 去覆盖系统里的 `codex`
- 不要让 `codex-cn` 读写 `~/.codex`
- 如果你是从源码直接运行内部 `codex` 二进制，务必显式导出上面的两个环境变量

更完整的中文说明见 [README_CN_ADAPTER.md](./README_CN_ADAPTER.md) 和 [docs/chinese-ai-providers.md](./docs/chinese-ai-providers.md)。

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
