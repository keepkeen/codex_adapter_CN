# Todo

## Publishing Readiness Review (2026-03-27)

- [x] Inspect `.github/workflows/rust-release.yml` for npm trusted publishing permissions, triggers, and publish gating.
- [x] Inspect related npm staging/packaging scripts and package metadata for `@keepkeen/codex-cn` publish readiness.
- [x] Summarize whether GitHub Actions can publish without local npm auth, what triggers are required, and likely blockers.

### Publishing readiness review

- Repo side is already wired for GitHub Actions OIDC publish: `.github/workflows/rust-release.yml` publishes from a GitHub-hosted `ubuntu-latest` runner with `id-token: write`, upgrades npm, and runs `npm publish` without `NODE_AUTH_TOKEN`.
- npm publish is only reached from tag pushes matching `rust-v*.*.*`, and the tag must match `codex-rs/Cargo.toml` exactly. The publish gate only allows stable `x.y.z` tags and numeric alpha tags `x.y.z-alpha.N`; beta tags or bare `-alpha` tags build releases but skip npm publish.
- Package metadata is aligned with the fork namespace: `@keepkeen/codex-cn`, `@keepkeen/codex-cn-sdk`, and `@keepkeen/codex-cn-responses-api-proxy` all set `publishConfig.access = public` and point their repository metadata at `keepkeen/codex_adapter_CN`.
- The main CLI package publishes one top-level package plus platform-specific tarballs that all resolve to the same npm package name `@keepkeen/codex-cn` under different versions/dist-tags, so trusted publisher setup is needed for three package names total: main CLI, SDK, and responses proxy.
- External blocker remains npm-side trust bootstrap: current registry lookups for all three package names return 404, and npm’s `npm trust` docs say the package must already exist before configuring trusted publishing. That means first publish is likely not achievable via pure trusted publishing alone; plan for one bootstrap publish with npm auth, then switch subsequent publishes to trusted publishing.

## Fork Branding + Publishing (2026-03-27)

- [x] 安装 `dotslash` 和 argument-comment-lint 所需 nightly 组件，并在仓库根目录验证 lint 可运行。
- [x] 盘点所有对外发布面的 upstream 命名：npm 包名、bin 名、README 安装命令、发布脚本、release workflow、仓库 URL。
- [x] 将对外发布身份统一改成 fork 专用名称，避免继续与 upstream `codex` 冲突或污染现有安装。
- [x] 补充详实中文文档，写清这次具体改动、四家模型适配、测试结果、限制、注意事项、使用方式，以及如何与原 `codex` 隔离。
- [x] 验证文档、包元数据和相关测试/lint。
- [ ] 推送到用户 GitHub 仓库并尝试 npm 发布；若外部账号或 registry 权限阻塞，记录精确阻塞点。

### Fork branding + publishing review

- 本机已安装 `dotslash 0.5.8`，并安装 `nightly-2025-09-18` + `llvm-tools-preview` / `rustc-dev` / `rust-src`，`just argument-comment-lint` 现在可以从仓库根目录正常运行。
- 对外发布面已经统一为 fork 命名：
  - CLI: `codex-cn`
  - npm: `@keepkeen/codex-cn`
  - SDK: `@keepkeen/codex-cn-sdk`
  - proxy: `@keepkeen/codex-cn-responses-api-proxy`
  - release/workflow/install 脚本/repo URL 都已同步到 `keepkeen/codex_adapter_CN`
- fork 现在默认通过两层机制避免污染上游：
  - npm wrapper 和 SDK wrapper 在未设置 `CODEX_HOME` 时默认写入 `~/.codex-cn`
  - 通过 `argv0 = codex-cn` 和 `current_cli_name()` 让帮助文本、resume 提示、状态页、更新提示都改成 `codex-cn`
- 文档已补齐：
  - 根 README 写清了外部命令名、适配模型、主要兼容层、已验证 workflow、当前限制、与上游 `codex` 的隔离方式
  - `README_CN_ADAPTER.md` 详细写了模型适配、工作流、测试结果、隔离安装、npm 包名和已知发布阻塞
  - `docs/chinese-ai-providers.md` 写清了 built-in provider、行为差异、live matrix、命名与隔离方式
- fork 相关验证已通过：
  - Rust: `cargo test -p codex-utils-home-dir`、`cargo test -p codex-cli`、`cargo test -p codex-tui`、`cargo test -p codex-tui-app-server`
  - Provider adapter: `cargo test -p codex-protocol --lib`、`cargo test -p codex-api --lib`、`cargo test -p codex-core --lib`
  - Live harness: `cargo test -p core_test_support provider_live --lib`、`cargo test -p core_test_support provider_live_tui --lib`
  - lint: `just argument-comment-lint -p codex-core`、`-p codex-cli`、`-p codex-tui`、`-p codex-tui-app-server`、`-p codex-utils-home-dir`
  - npm staging: `npm pack --dry-run` in `codex-cli` / `sdk/typescript` / `codex-rs/responses-api-proxy/npm`
- 已知外部阻塞：
  - `npm whoami --registry=https://registry.npmjs.org/` 返回 `ENEEDAUTH`
  - 本机还配置过 `https://registry.npmmirror.com`，直接本地 `npm publish` 也会因为未登录失败
  - GitHub Actions 已具备 trusted publishing 所需的 OIDC 配置，但 `@keepkeen/*` 三个包当前都还是首发状态；按 npm 的 trusted publishing 约束，首发通常仍需要先做一次带认证的 bootstrap publish

## Provider Feature Onboarding Guard (2026-03-27)

- [x] 把 provider feature surface 从“只有 label 的枚举”扩成带维护契约的中心元数据，至少显式声明每个 feature 的 live smoke 覆盖要求。
- [x] 为 built-in provider profile 增加全矩阵断言，避免以后新增 feature 或调整支持级别时只改局部 spot-check。
- [x] 为 `js_repl`、`output_schema`、`web_search` 这类需要实际 adapter 的 feature 增加“support matrix 与 transport/dialect 必须一致”的中心测试。
- [x] 让 exec live 和 TUI live 的 feature 覆盖直接从 provider feature metadata 反推，避免以后新增 feature 只加 profile 不加 smoke workflow。
- [x] 修复 built-in OpenAI provider 在默认 `base_url = None` 时拿不到 profile 的静默绕过问题。
- [x] 更新 review，写清这轮守卫现在能强制什么、还不能自动发现什么。

### Provider feature onboarding guard review

- `codex-rs/protocol/src/provider_profiles/features.rs` 现在不只定义 `ProviderFeature` 和 label，还集中声明了每个 feature 的 `ProviderFeatureLiveCoverage`。以后新增 feature，除了 support matrix 本身，必须同时决定它是否需要 `exec` live smoke、TUI smoke，还是无需 dedicated smoke。
- `codex-rs/core/tests/common/provider_live.rs` 和 `codex-rs/core/tests/common/provider_live_tui.rs` 现在分别断言：所有要求 `exec` live smoke 的 feature 都必须有对应 `LiveWorkflow`，所有要求 TUI smoke 的 feature 都必须落在 `TUI_LIVE_COVERED_FEATURES`。这让“加了 feature 但没补 smoke workflow”的情况直接变成测试失败。
- `codex-rs/protocol/src/provider_profiles/mod.rs` 现在有一条 built-in provider × `ProviderFeature::ALL` 的完整支持矩阵断言，不再只靠零散 provider/feature spot-check；新增 feature 或调整支持等级时，review 面会集中在一处。
- 同一文件里也增加了 transport/dialect 合约测试：`JsRepl` 的 support matrix 必须和 `js_repl_transport` 一致，`OutputSchema` 的 support matrix 必须和 `structured_output_strategy` 一致，`WebSearch` 的 support matrix 必须和 `emulate_live_web_search` 一致，避免“矩阵写成 Emulated 但底层 adapter 没接上”。
- `codex-rs/core/src/model_provider_info.rs` 现在会为默认 OpenAI provider 回填官方默认 base URL，因此 built-in OpenAI 配置不再因为 `base_url = None` 而静默失去 provider profile；这补掉了一个会绕过 feature surface 的 identity 漏洞。
- 这轮守卫能强制的是：“一旦某个能力被纳入 `ProviderFeature`，built-in provider matrix、adapter 合约、status labels、live smoke 入口都要同步更新。” 还不能自动发现的是：upstream 新特性如果完全没有被纳入 `ProviderFeature`，仓库仍然不会凭空知道它存在；这仍然需要 sync upstream 时把新能力先登记进 feature surface。

## js_repl + output_schema Compatibility (2026-03-27)

- [x] 把 `js_repl` 与 `output_schema` 从“chat provider 下显式不支持”切换成 provider-aware 兼容实现，并明确只做这两项，不顺带放开 `artifacts` / `image_generation`。
- [x] 为 provider profile 增加 `js_repl` transport 与 `output_schema` strategy 的中心化声明，避免后续继续散落在 tool serialization / client 分支里。
- [x] 让 chat provider 下的 `js_repl` 保持公开工具名不变，但通过 function-tool wrapper 接入现有 runtime；OpenAI/Responses 路径继续保留 freeform 语义。
- [x] 让 chat provider 下的 `output_schema` 通过 request strategy + 本地严格校验/修复闭环实现“结构化输出成功或明确失败”，不允许静默退化成普通文本或弱 JSON。
- [x] 补齐单元/集成测试，覆盖 OpenAI 与 chat provider 两条路径的 `js_repl`、`js_repl_reset`、`output_schema` 成功/失败/修复行为。
- [x] 更新 review，写清当前四家 provider 对 `js_repl` / `output_schema` 的支持方式、限制和后续 `artifacts` / `image_generation` 的保留边界。

### js_repl + output_schema review

- `codex-rs/protocol/src/provider_profiles/dialects.rs` 现在集中声明了 `JsReplTransport` 与 `StructuredOutputStrategy`；四家内置 chat provider 改为 `js_repl = Emulated`、`output_schema = Emulated`，OpenAI 继续保留 `Native`。
- `codex-rs/core/src/tools/spec.rs` 现在会根据 provider profile 把 chat provider 下的 `js_repl` 转成同名 function-tool wrapper，参数固定为 `code` 和可选 `timeout_ms`；OpenAI/Responses 路径仍保留 freeform/custom-tool 语义。
- `codex-rs/core/src/tools/handlers/js_repl.rs` 已统一接受 custom payload 和 function payload，两条路径都收敛到同一套 `JsReplArgs` 解析与 runtime 执行逻辑；`js_repl_reset` 保持原有行为。
- `codex-rs/core/src/project_doc.rs` 现在会按 wire API 给出 provider-aware 指令：chat provider 明确要求把完整 JS 放进 `code` 字段，Responses/OpenAI 继续提示 raw JS freeform 输入。
- `codex-rs/core/src/client.rs` 不再对内置 chat provider 一刀切拒绝 `output_schema`；新逻辑会读取 provider profile 的结构化输出策略，并在 chat path 上接入 `codex-rs/core/src/structured_output.rs` 的严格本地校验/修复闭环。
- `codex-rs/codex-api/src/requests/chat.rs` 现在会为 DeepSeek / GLM / Kimi 注入 `response_format = {"type":"json_object"}`，而 MiniMax 继续走普通文本输出再做本地严格校验；generic chat provider 仍保持显式拒绝，避免误判为已支持。
- `codex-rs/core/src/structured_output.rs` 会在最终 assistant 消息上执行 JSON Schema 校验；不合法时最多自动修复重试 3 次，成功则只向上游暴露 schema-compliant JSON，失败则返回明确 structured-output error，不静默降级成普通文本。
- `artifacts` 与 `image_generation` 本轮仍保持 `Unsupported`；这次只把 `js_repl` 与 `output_schema` 补到可维护的 provider-aware 兼容层，没有顺带放开其他 secondary features。

## Secondary Feature Coverage (2026-03-27)

- [x] 把 `artifacts`、`js_repl`、`image_generation`、`output_schema` 纳入 provider support matrix review，确认 OpenAI 基线与四家 chat provider 的目标状态。
- [x] 把仍然 unsupported 的 secondary features gating 前移到统一入口，并为可 emulated 的 feature 预留 provider-aware transport hook，避免 chat provider 继续出现“上层可见、下层静默丢弃”的错配。
- [x] 为 `artifacts`、`js_repl`、`image_generation` 补 provider-aware 工具暴露测试，确认 unsupported 与 emulated 两类 feature 都按 profile 正确暴露。
- [x] 为 generic chat provider 保留 `output_schema` 显式拒绝测试，确认只有内置四家 chat provider 会进入 emulated structured-output 路径。
- [x] 把这四类 secondary features 的覆盖结论写回 review，作为后续 upstream feature onboarding 的基线。

## Provider Live Image Smoke Review (2026-03-26)

- [x] Inspect the existing provider live runner and matrix hooks for the cleanest image-input insertion point.
- [x] Inspect current core exec/image tests for the smallest stable assertion pattern to reuse.
- [x] Summarize the recommended live image-input smoke shape, exact files, and caveats without editing code.

### Provider live image smoke review

- `codex exec` already supports `--image` on both fresh runs and `resume`, and the current provider live runner is the right layer to reuse; there is no need to route image smoke through TUI or ad-hoc shell scripts.
- The cleanest positive smoke should target only image-capable provider/model cases (currently Kimi K2.5 and GLM via `glm-4.6v`), because the default DeepSeek / GLM-5 / MiniMax matrix models are text-only in the bundled catalog.
- The smallest stable assertion shape is: attach one deterministic local PNG via `--image`, assert the persisted rollout/session contains one `input_image`, assert the final agent message identifies one trivial visual property, and keep the existing `turn.completed usage` assertion.
- The main harness gap is not transport but fixture staging: `provider_live.rs` currently has a `prepare_home(home)` hook but no workflow hook that can write an image into the temp `cwd` and pass the resulting path into `codex exec`.

## Provider Abstraction Hotspot Review (2026-03-26)

- [x] Inspect current branch diffs in provider catalog, chat request/SSE, tool spec, session token handling, and TUI status/footer.
- [x] Map the provider/model-specific branching points that currently require code edits for Chinese-provider adaptation.
- [x] Summarize clean extraction seams so a new provider mostly needs metadata/handler registration instead of core switch edits.

### Provider abstraction hotspot review

- Current provider identity is split between `known_provider_id()` in `model_provider_info.rs` and ad-hoc `provider.name` / `provider.base_url` matching in `codex-api/src/requests/chat.rs`; that should collapse into a single provider profile registry.
- `provider_catalog.rs` is already the natural place for bundled-model metadata and alias resolution, but request shaping, SSE dialect differences, and chat-tool adaptation are still encoded elsewhere as switches/heuristics.
- `tools/spec.rs` currently mixes generic tool inventory with chat-specific wire adaptation (`web_search` emulation + handler registration); that should move behind a transport/provider tool adapter.
- `codex.rs` and the TUI status/footer files are consuming normalized usage data, but the fallback accounting policy and provider-facing usage presentation are still duplicated across layers and should be centralized.

## Provider Profile Refactor (2026-03-26)

- [x] 在共享模块里建立 built-in provider profile registry，集中声明 provider 身份、feature surface、request dialect、stream dialect、tool transport policy。
- [x] 把中国 provider 的 bundled model catalog / lookup alias 从 `core` 迁移到共享数据层，让新增模型主要变成数据改动。
- [x] 让 chat request builder 读取 profile，而不是继续按 provider name/base URL 分支。
- [x] 让 chat SSE parser 读取 profile 的 stream dialect / usage policy，而不是继续散落 provider-specific 解析规则。
- [x] 让 chat tool transport 读取 profile 的 tool policy，为后续按 feature surface 接入 upstream 新工具做准备。
- [x] 补 profile 层的单元测试，以及 request / stream / tool 的回归测试，确保现有 live 行为不回退。
- [x] 汇总这轮重构的剩余缺口，尤其是 TUI/app-server 状态层、CI live matrix 和 upstream feature onboarding 流程。

### Provider profile refactor review

- 新增共享模块 `codex-rs/protocol/src/provider_profiles/`，把 built-in provider 身份、feature surface、request/stream dialect、tool transport policy，以及中国 provider 的 bundled model catalog / lookup alias 收敛到单一 registry。
- `codex-rs/core/src/model_provider_info.rs` 现在通过共享 registry 推断 built-in provider id，并用 registry 驱动四家 chat provider 的默认定义，减少 name/base_url/env_key 在多处重复维护。
- `codex-rs/core/src/models_manager/provider_catalog.rs` 不再自己维护四家模型列表，改为消费共享 catalog 数据并保留 OpenAI catalog 作为独立基线。
- `codex-rs/codex-api/src/requests/chat.rs` 已把 DeepSeek/GLM/Kimi/MiniMax 的请求方言从 provider-name heuristics 收敛到 profile；当前仍保留 generic chat provider fallback heuristics，供非内置 chat provider 使用。
- `codex-rs/codex-api/src/sse/chat.rs` 已通过 profile stream dialect 选择 reasoning 字段和 usage 解析来源；usage 归一化逻辑仍是统一实现，而不是每家 provider 各写一份 parser。
- `codex-rs/core/src/tools/spec.rs` 的 chat web-search 适配现在开始读取 profile tool transport policy，而不是默认只要是 chat provider 就无条件走同一条分支。
- `codex-rs/protocol/src/provider_profiles/features.rs` 现在把 provider feature surface 枚举化为单一宏定义，统一生成 `ProviderFeature`、`ProviderFeatureSupport`、标签和 compatibility notes；后续新增 upstream feature 时，不再需要手工维护多份 feature 列表。
- `codex-rs/core/src/model_provider_info.rs` 已公开 `known_provider_id()` / `built_in_profile()`，让 TUI 和其他上层可以直接读取 provider profile，而不是重新推断 provider 身份。
- `codex-rs/tui/src/status/card.rs` 和 `codex-rs/tui_app_server/src/status/card.rs` 现在会直接消费 profile feature surface，并在 `/status` 里显示 `Emulated` / `Unsupported` 兼容说明；对应 snapshot 测试已补齐。
- 当前主要剩余缺口还有两块：
  - CI/live smoke 还没有收敛成 provider × workflow 的统一 matrix；目前仍主要依赖手工 live 回归和任务记录。
  - upstream 新 feature 的 onboarding 虽然已经有单一 feature surface，但还没有进一步接到 CI 守卫上，例如“新增 feature 时自动要求 live matrix / 文档矩阵同步更新”。

## Provider Live Matrix Harness (2026-03-26)

- [x] 盘点现有 ignored live tests、CLI spawn helper、隔离 `CODEX_HOME` 模式和 GitHub Actions 布局，确定最小冲突接入点。
- [x] 在 `core/tests/common` 提供可复用的 provider live exec harness，统一处理 provider/model/env-key 矩阵、隔离状态目录和 JSONL 事件采集。
- [x] 在 `core/tests/suite` 增加 ignored 的 provider live smoke matrix，先覆盖基础 shell workflow 和 `web_search` workflow。
- [x] 增加手动/定时的 GitHub Actions 入口，按 provider 维度运行 live smoke matrix。
- [x] 更新中文 provider 文档和 review，写清本地运行方式、CI 入口，以及当前 live matrix 已覆盖/未覆盖的 workflow。

### Provider live matrix harness review

- 现有 live matrix 接入点已经落在 `codex-rs/core/tests/common/provider_live.rs` 和 `codex-rs/core/tests/suite/provider_live_matrix.rs`，没有继续扩散到 request/SSE/core 主路径；以后新增 provider 或 workflow，优先扩这里的矩阵数据和断言。
- harness 统一封装了 provider/model/env-key 矩阵、隔离的 `CODEX_HOME` / `CODEX_SQLITE_HOME`、`codex exec --json` 启动方式，以及 JSONL 事件采集与基础断言。
- 当前 live matrix 先覆盖两个最稳定的 workflow：
  - `basic-shell`：验证 provider 能完成非交互 `exec --json`、实际调用 shell 并回传 `turn.completed` usage。
  - `web-search`：验证 provider 能完成开启 `web_search="live"` 的非交互 `exec --json` 回合，并在禁用 shell 风格工具后返回预期搜索结果。
- `web-search` 这一项目前没有在 live exec JSONL 中强断言 `item.type == "web_search"`。原因不是适配层没接，而是 chat-provider 的本地 `web_search` 兼容层在 exec JSON 事件面上还没有稳定暴露成独立 `ThreadItem::WebSearch`。这个协议面已经由现有单元/集成测试覆盖，live smoke 现阶段只断言：
  - search-enabled turn 正常完成
  - shell 风格工具被禁用后没有退回 `command_execution`
  - 最终答案正确
- GitHub Actions 入口已经加到 `.github/workflows/provider-live-matrix.yml`，按 provider 维度跑 ignored live smoke tests，触发方式是 `workflow_dispatch` + `schedule`；夜间定时目前只在 `keepkeen/codex` 上启用，避免 forks 空跑。
- 中文 provider 文档已补 `Live Smoke Matrix` 一节，写清了本地命令和当前覆盖边界。
- 验证结论：
  - `cargo test -p core_test_support provider_live --lib` 通过。
  - `cargo test -p codex-core provider_live_matrix -- --ignored --nocapture --test-threads=1` 通过；在当前 shell 没注入 provider keys 的情况下，8 个 ignored live tests 都按预期走了 skip path。
  - 另外用隔离安装的 `codex-cn` + DeepSeek 实际跑了与 harness 等价的两条 JSON smoke：
    - `basic-shell`：输出包含 `command_execution`，`aggregated_output = "provider-live-smoke-ok\n"`，并有 `turn.completed usage`。
    - `web-search`：在禁用 shell 风格工具后返回 `https://github.com/openai/codex`，并有 `turn.completed usage`。
- 当时尚未编码进 live matrix 的 workflow 是：skills、MCP、子代理、image input、auto compact、交互式 `/compact`。本轮已把前四类补进同一套 harness；剩余 `image input` 和交互式 `/compact` 仍应继续沿这条分层路线推进，而不是回到 ad-hoc shell 脚本。

## Provider Live Matrix Expansion (2026-03-26)

- [x] 把 `provider_live` helper 从“单回合 exec helper”重构成可声明场景的 runner，统一承载 `CODEX_HOME` 预置、config 注入、持久线程恢复和 artifact 导出。
- [x] 在同一套 runner 里补 `skills` live smoke，使用可预测的临时 skill 和唯一 sentinel，避免只靠模型口头遵守断言。
- [x] 在同一套 runner 里补 `MCP` live smoke，复用 `test_stdio_server` 并在 JSONL 中强断言 `mcp_tool_call` 结果。
- [x] 在同一套 runner 里补 `子代理` live smoke，强断言 `collab_tool_call` 的 `spawn_agent` 路径和 receiver thread。
- [x] 在同一套 runner 里补 `auto compact` live smoke，走持久 session + `exec resume`，并通过 rollout `type:"compacted"` 证明真实压缩发生。
- [x] 把新增 workflow 接进 ignored provider matrix、GitHub Actions live workflow 和中文文档。
- [x] 汇总本轮完成后的剩余缺口，明确 `image input` 和交互式 `/compact` 是否继续留在独立 harness。

### Provider live matrix expansion review

- 目标不是再堆更多 ad-hoc ignored tests，而是把 live workflow 的公共需求抽成单一场景层：一次实现 `home setup / config / first turn / resume turn / artifact capture`，后续新增 workflow 只加 declarative case。
- `skills / MCP / 子代理` 三类都可以继续走 `codex exec --json`，因此优先复用现有 JSONL 事件断言路径，而不是过早切到 TUI PTY 自动化。
- `auto compact` 需要持久线程，因此 runner 必须支持 `exec` 首回合 + `exec resume` 后续回合，并把 thread id / rollout 路径暴露给断言层。
- 交互式 `/compact` 仍然和 TUI slash workflow 强绑定；如果本轮先把 `auto compact` 纳入 live matrix，就可以把 `/compact` 留在后续单独的 PTY/TUI harness，而不是污染当前 exec runner。
- `codex-rs/core/tests/common/provider_live.rs` 现在已经不是固定 `--ephemeral` 的单回合 helper，而是统一 runner：支持 workflow-specific home setup、config snippets、首次 `exec`、持久 `resume`、rollout 定位，以及按 run 索引导出 artifacts。
- `codex-rs/core/tests/suite/provider_live_matrix.rs` 已扩成 34 个 ignored live smoke tests：`deepseek/glm/kimi/minimax × basic-shell/web-search/skill/mcp-echo/subagent/js-repl/output-schema/auto-compact` 共 32 条，再加 `glm/kimi` 两条 `image-input` smoke，并用宏收敛重复定义。
- `skill` live smoke 通过临时 `CODEX_HOME/skills/provider-live-demo/SKILL.md` 和唯一 secret sentinel 证明技能注入真正生效；`MCP` live smoke 复用 `test_stdio_server` 并断言 `mcp_tool_call`; `subagent` live smoke 断言 `collab_tool_call` 的 `spawn_agent`/`wait` 和 receiver thread ids。
- `auto compact` live smoke 通过持久 `exec` + `resume` 低阈值线程触发，并通过 rollout `type:"compacted"` 取证，而不是继续误用 stdout JSONL 去推断 compaction。
- `.github/workflows/provider-live-matrix.yml` 现在是唯一 live CI 入口，按 provider × workflow 维度运行 ignored tests 并上传 workflow 级 artifact；之前重复的 `.github/workflows/provider-live.yml` 已删除，避免维护两套矩阵。
- 当前剩余的 live 自动化缺口已经收敛到两类：`image input` 以及交互式 `/compact`。前者仍适合继续挂在 exec runner，后者应单独走 TUI/PTy harness。

## Remaining Provider Sweep (2026-03-26)

- [x] 对 `glm`、`kimi`、`minimax` 做真实 `--search` 回归，确认搜索工具在三家 chat provider 下都能稳定工作。
- [x] 对 DeepSeek 做独立的 MCP / skill smoke，补齐四家 provider 的专项验证矩阵。
- [x] 补做稳定的交互式 `/compact` 验证，至少拿到一条可复用、可证明 compact 发生的自动化或半自动化路径。
- [x] 评估并实现 MiniMax `cached_tokens` 的用户侧展示，优先考虑 `/status` 和 footer 的最小可读呈现。
- [x] 汇总四家 provider 当前“完全跑通 / 有边界 / 仍待修”的最终矩阵，并写入 review。

## Compact Investigation (2026-03-26)

- [x] Trace manual compact and auto compact paths in CLI/TUI for local provider-backed threads.
- [x] Identify the minimal isolated `CODEX_HOME` setup and concrete commands/interactions to trigger each path reliably.
- [x] Collect the logs, session events, and files that prove compaction happened.
- [x] Find existing tests or tooling patterns that already exercise or inspect compact behavior.
- [x] Summarize the verified workflow and evidence in the review section.

## MCP stdio smoke test research (2026-03-26)

- [x] Inspect the MCP config docs/schema for the exact `config.toml` shape for a stdio server.
- [x] Inspect the rmcp test server binary and the relevant exec-mode tests that exercise MCP tool calls.
- [x] Derive the smallest isolated `CODEX_HOME` setup and concrete smoke-test commands.
- [x] Summarize the exact config, prompt, commands, and caveats in the review/final response.

### MCP stdio smoke test review

- Minimal manual config shape is a `[mcp_servers.<name>]` table with `command`, optional `args`, and optional nested `[mcp_servers.<name>.env]`; `enabled` defaults to true and `required` defaults to false.
- The existing rmcp stdio test server lives at `codex-rs/rmcp-client/src/bin/test_stdio_server.rs`; it serves `echo`, `echo-tool`, `image`, and `image_scenario` over stdio and returns `structuredContent.echo` plus `structuredContent.env` for the echo tool.
- The plain MCP round-trip test is `codex-rs/core/tests/suite/rmcp_client.rs::stdio_server_round_trip`, which configures `startup_timeout_sec = 10s`, injects `MCP_TEST_VALUE`, and proves Codex calls `mcp__rmcp__echo`.
- The most deterministic exec-mode proof is `codex-rs/core/tests/suite/code_mode.rs::code_mode_can_print_structured_mcp_tool_result_fields`, which enables `features.code_mode` and calls `await tools.mcp__rmcp__echo({ message: "ping" })` from the `exec` tool.
- Local verification was partially blocked by environment state: targeted `cargo test -p codex-core stdio_server_round_trip -- --nocapture` and `cargo test -p codex-core code_mode_can_print_structured_mcp_tool_result_fields -- --nocapture` both failed before running due `No space left on device`, and `cargo build -p codex-rmcp-client --bin test_stdio_server` failed for the same reason with only about `1.1GiB` free on `/System/Volumes/Data`.

- [x] 建立上游基线：克隆 `openai/codex` 到父目录并确认对比基准。
- [x] 盘点当前分支相对 upstream 的全部改动，并按模块分类。
- [x] 核对 DeepSeek、MiniMax、GLM、Kimi 截至 2026-03-25 的官方模型信息与 API 兼容差异。
- [x] 修正模型映射、默认值、供应商兼容层和潜在 bug。
- [x] 更新必要文档与说明，确保适配路径清晰。
- [x] 运行格式化、lint 和针对性测试，记录验证结论。
- [x] 在 `README.md` 和 `README_CN_ADAPTER.md` 补充中文说明，写清改进点、兼容原理、使用方式和限制。
- [x] 将当前分支推送覆盖到用户 fork：`keepkeen/codex`。
- [x] 补充 README 中相对上游仓库的差异说明，并检查 prompts/templates 是否通用。
- [x] 盘点本地磁盘占用并清理当前仓库可重建垃圾，优先释放 `codex-rs/target`。
- [x] 构建独立的 `codex-cli` release 二进制，避免覆盖现有 `codex`。
- [x] 安装隔离的 `codex-cn` 包装脚本和独立 `CODEX_HOME` / `CODEX_SQLITE_HOME`。
- [x] 验证 `codex-cn` 与现有 `/opt/homebrew/bin/codex` 共存且不会共享配置/状态目录。
- [x] 将隔离安装的 `codex-cn` 默认配置切换为 DeepSeek。
- [x] 仅为 `codex-cn` 写入隔离的 DeepSeek API key，不污染现有 `codex` 环境。
- [x] 验证 `codex-cn` 包装脚本会从隔离目录加载 DeepSeek 配置和 key。
- [x] 修复 DeepSeek chat 请求中的 `developer` role 兼容问题。
- [x] 为 chat/completions provider 增加消息角色兼容映射回归测试。
- [x] 验证 `codex-cn` 的 DeepSeek 路径不再发送不支持的 `developer` role。
- [x] 检查 chat 请求里 `tool_calls` 与 `tool` 输出的配对规则，定位 DeepSeek 的后续报错根因。
- [x] 修复 chat 兼容层中 `tool_call_id` 与实际输出 `call_id` 不一致的问题，并补回归测试。
- [x] 重新验证 `codex-api`、回装隔离的 `codex-cn`，并再次清理可重建构建产物。
- [x] 核实当前实际启动的是 `codex` 还是 `codex-cn`，并检查 PATH / 包装脚本是否仍有混用。
- [x] 检查 `~/.codex-cn-test` 下最近会话与日志，确认是否是旧会话历史导致的交互请求污染。
- [x] 如有必要，清理隔离实例的旧会话状态并重新验证交互路径。
- [x] 盘点当前 `web_search` 在 Responses 与 chat provider 下的差异，确定兼容接入点。
- [x] 为 chat/completions provider 增加可执行的 `web_search` function tool 适配与本地 handler。
- [x] 补充 README / 中文文档，明确中国 provider 下联网搜索的新行为与限制。
- [x] 跑定向测试并重装隔离的 `codex-cn`，验证 DeepSeek 路径下 `--search` 可用。
- [x] 审查 `jina-ai/node-DeepResearch` 的搜索、读页、去重和压缩逻辑，挑出可直接迁移的策略。
- [x] 将可复用的检索增强整合进当前 `web_search`：优先做多查询扩展、结果去重/排序、按问题抽取网页相关片段。
- [x] 为新的检索增强补测试与文档，避免扩大上下文污染。
- [x] 重装隔离版 `codex-cn` 并做 DeepSeek 实机联网搜索回归验证。
- [x] 修复 `chat/completions` provider 的 usage 解析与 fallback token 估算，恢复 token 计数刷新。
- [x] 恢复基于 token 阈值的自动压缩，确认中国 provider 下会重新触发 pre-sampling compact。
- [x] 为 `web_search` 增加 GitHub / 文档站专用解析，降低整页正文注入造成的上下文污染。
- [x] 更新 README / 中文文档 / 任务记录，写清新的搜索与 token/compact 行为。
- [x] 运行定向测试、重装隔离版 `codex-cn`，并做一次 DeepSeek 实机回归验证。
- [x] 根据用户纠正，恢复 Kimi 内置目录为官方 `kimi-k2.5`，不再保留 `kimi-latest`。
- [x] 根据用户纠正，确认 MiniMax 当前内置目录为 `MiniMax-M2.7` / `MiniMax-M2.7-highspeed`，不再误写 `M2.5`。
- [x] 为 `glm`、`kimi`、`minimax` 创建各自独立的 `CODEX_HOME` / `CODEX_SQLITE_HOME` / skills 目录，避免与现有 `codex` / `codex-cn` 互相污染。
- [x] 用真实 API key 对 `glm`、`kimi`、`minimax` 做隔离的端到端 exec 回归。
- [x] 对通过基础 exec 的中国 provider 再补一轮 `--search` 实机验证。
- [x] 用真实 API key 对 `glm`、`kimi`、`minimax` 做隔离的 skills / MCP / 子代理 / token 计数 / 自动 compact 实测。
- [x] 补做 DeepSeek 的子代理实测，补齐四家 provider 的协作工具验证。
- [x] 修正 compact 后 footer 容易误导的 `context left` 展示：当百分比仍是 `100%` 时额外显示实际已占用 tokens。
- [x] 记录 MiniMax 被动 Prompt cache / `cached_tokens` 的当前适配行为和限制。
- [x] 对 `glm`、`kimi`、`minimax` 做稳定的交互式 `/compact` 自动化验证。
- [x] 记录三家 provider 的真实兼容结果、失败点和后续修复项。

## Final Live Coverage And Maintainability (2026-03-26)

- [x] 把剩余 live 缺口拆成两类：`image input` 继续走 `exec --json` matrix，交互式 `/compact` 单独走 PTY/TUI harness。
- [x] 让 live workflow 能声明所需 feature support 和 workflow-specific model override，避免新增模型/能力时继续手写 provider 白名单。
- [x] 把 `image input` 接进 provider live matrix，只对声明支持图片输入的内置 provider/model 生效。
- [x] 抽共享的 PTY/TUI live helper，避免 `tui` 和 `tui_app_server` 再各写一套 `/compact` 交互驱动。
- [x] 为 legacy TUI 和 app-server TUI 都补一条 ignored 的真实 `/compact` smoke，并把 provider × workflow 证据统一落到 session log + rollout。
- [x] 把新的 live workflow 接进 GitHub Actions matrix、中文文档和最终 review。

## Live Validation Constraints

- 保持严格 provider/model 语义：不要新增内置后备模型，不要做 request-time 或 catalog-time fallback。
- 若某个内置模型在真实 key 下失败，优先修复适配层错误；若适配层无误，则如实记录权限或官方兼容限制。

# Review

- Secondary feature coverage结论：
  - `artifacts`、`js_repl`、`image_generation`、`output_schema` 现已被纳入 built-in provider feature surface；OpenAI profile 继续标记为 `Native`，四家 chat provider 当前对 `js_repl` / `output_schema` 标记为 `Emulated`，对 `artifacts` / `image_generation` 继续标记为 `Unsupported`。
  - `codex-rs/core/src/tools/spec.rs` 的 provider-aware gating 现在不再一刀切关闭 `js_repl`：chat provider 会暴露同名 function wrapper，`artifacts` / `image_generation` 仍会在工具表构建前被提前关闭，避免“上层可见、下层静默丢弃”。
  - `codex-rs/core/src/project_doc.rs` 会按 wire API 注入不同的 `js_repl` 使用说明，避免 chat provider 继续收到只适用于 freeform tool 的错误指令。
  - `codex-rs/core/src/client_tests.rs` 现在只保留 generic chat provider 的显式拒绝测试；四家内置 chat provider 已经通过 profile strategy 转向本地严格 `output_schema` 校验/修复路径。
  - `codex-rs/core/src/tools/spec_tests.rs`、`codex-rs/core/src/tools/handlers/js_repl_tests.rs`、`codex-rs/core/src/structured_output.rs` 和 `codex-rs/protocol/src/provider_profiles/mod.rs` 现已覆盖 OpenAI 与 chat-provider 两类基线：OpenAI 继续保留 native 路径，四家 built-in chat provider 则走 emulated compatibility layer。这为后续 upstream feature onboarding 提供了明确模板：先扩 feature surface，再为每个 provider 显式标注 `Native | Emulated | Unsupported`，最后补 transport / 校验 / 集成测试。

- Compact investigation结论：
  - 非 OpenAI provider（包括本地 `ollama` / `lmstudio`）不会走远端 `/responses/compact`；`codex-rs/core/src/tasks/compact.rs` 会把它们分到本地 compact 路径，核心判断在 `codex-rs/core/src/compact.rs` 的 `should_use_remote_compact_task(provider.is_openai())`。
  - 手动 compact 的最小可靠入口是交互式 TUI `/compact`；TUI 直接发 `Op::Compact`，旧 TUI 观察点是 `EventMsg::ContextCompacted`，app-server TUI 观察点是 `ThreadItem::ContextCompaction`。
  - 自动 compact 的最小可靠脚本化入口是持久化 `exec` 线程：先跑一轮明显超出 `model_auto_compact_token_limit` 的大 prompt，再 `exec resume --last` 发送下一条用户消息；pre-sampling compact 会在第二轮开始前触发。
  - 证明 compact 最稳的文件证据是 `$CODEX_HOME/sessions/**/rollout-*.jsonl` 里的 `{"type":"compacted",...}` 行；`--ephemeral` 不会写 rollout，因此不适合做 compact 取证。
  - TUI 额外可打开 `CODEX_TUI_RECORD_SESSION=1` + `CODEX_TUI_SESSION_LOG_PATH=...` 记录 UI 入/出事件，便于 grep `/compact` 和后续 compact 相关事件。
  - `codex exec --json` 目前不会输出 `contextCompaction` item；人类可读 exec 输出会打印 `context compacted`，所以如果要脚本取证，应优先看 rollout 文件而不是只看 `--json`。
- 现有可复用测试/工具：
  - `codex-rs/core/tests/suite/compact.rs` 已覆盖本地 provider 下的 manual compact、auto compact、rollout 持久化、resume 后 auto compact，以及 `ContextCompaction` item / `ContextCompacted` 事件链路。
  - `codex-rs/app-server/tests/suite/v2/compaction.rs` 已覆盖 `thread/compact/start` 和本地/远端 auto compact 的 `item/started` + `item/completed` 通知。
  - `codex-rs/tui/src/chatwidget/tests.rs` 和 `codex-rs/tui_app_server/src/chatwidget/tests.rs` 已覆盖 `/compact` 触发与排队行为。
- Provider live matrix 现状：
  - `codex-rs/core/tests/common/provider_live.rs` 抽出了 built-in provider 表、`exec --json` 调用、隔离 `CODEX_HOME` / `CODEX_SQLITE_HOME`、JSONL 事件解析，以及通过 `CODEX_PROVIDER_LIVE_ARTIFACT_DIR` 导出 home/cwd/stdout/stderr 的统一 helper。
  - `codex-rs/core/tests/suite/provider_live_matrix.rs` 现已提供 34 个 ignored live smoke tests，覆盖 `deepseek/glm/kimi/minimax × basic-shell/web-search/skill/mcp-echo/subagent/js-repl/output-schema/auto-compact` 八条真实工作流，以及 `glm/kimi` 两条 `image-input` smoke，并直接断言 JSONL `command_execution`、`web_search`、`mcp_tool_call`、`collab_tool_call`、`function_call`、`turn.completed usage`，以及 compact / image rollout 证据。
  - `.github/workflows/provider-live-matrix.yml` 现在是唯一 live CI 入口，会在 `workflow_dispatch` / `schedule` 下按 provider × workflow 跑这些 ignored tests，并上传 workflow 维度的 live artifacts。
- 当前 matrix 已补进 `image input` 和交互式 `/compact`：前者继续走 `provider_live.rs` 的 `exec --json` runner，后者走 `provider_live_tui.rs` 的 PTY/TUI harness。
  - `provider_live_tui.rs` 现在会区分 legacy / app-server 的 startup-ready 信号、自动解析当前 resume 会话的 rollout 路径，并把 `session log + rollout` 统一导出成 artifact。
- 三家补充的真实 `--search` 回归结论：
  - `glm`：隔离 `CODEX_HOME=~/.codex-live-tests/glm` 下用已安装的 `codex` binary 实测通过；rollout `2026/03/27/rollout-2026-03-27T01-20-32-019d2b29-7dcf-7b50-8bd1-7abe203d52ff.jsonl` 里可见 `function_call name="web_search"` 和返回的 `https://github.com/openai/codex`。
  - `kimi`：隔离 `CODEX_HOME=~/.codex-live-tests/kimi` 下实测通过；rollout `2026/03/27/rollout-2026-03-27T01-21-26-019d2b2a-5020-7dc1-8595-a6bc69df5fa6.jsonl` 里可见至少两次 `web_search` 调用，最终按要求只返回 canonical URL。
  - `minimax`：隔离 `CODEX_HOME=~/.codex-live-tests/minimax` 下实测可完成任务，但 `web_search` 质量明显更弱；rollout `2026/03/27/rollout-2026-03-27T01-22-22-019d2b2b-2a99-7ff1-ada1-a0a158bcaaf7.jsonl` 里多次 `web_search` 返回 `No web results found`，最终答案依赖 repeated search + compact handoff 才稳定收敛，因此应记为“可用但有边界”。
- 本轮 `/compact` live 修复结论：
  - `codex-rs/core/src/compact.rs` 现在会在手动 compact 前显式 `ensure_rollout_materialized()`，因此 resumed TUI 会话不再只发 UI 事件而不 materialize 自己的 rollout。
  - legacy TUI 与 app-server TUI 的 session log 结构并不相同：legacy 会写 `session_configured` / `list_skills_response` / `context_compacted`，app-server 只稳定写 `session_start` / `insert_history_cell` / `op=list_skills`，而 compact 完成信号主要落在 rollout。
  - `provider_live_tui.rs` 这轮又补了两处稳定性修复：
    - compact 完成后的 PTY 收尾不再依赖 `Ctrl-C` 双击状态机，而是显式输入 `/exit` 退出，避免测试已经 compact 成功却因为 TUI 没退出而超时。
    - seed prompt 从容易被模型改写歪的 `sed '...&'` 版本改成 `awk '{print "provider-live-compact-line-" $1}'`，避免 Kimi 这类模型把 `&` 逃逸成字面量导致 sentinel 失真。
  - 交互式 `/compact` live smoke 必须指向当前源码编出来的 binary；继续用旧安装的 `~/.local/codex-cn/bin/codex-cn` 会漏掉最新的 rollout materialization 修复，产生“session log 已 compact、rollout 却没落盘”的假阴性。
  - 使用当前源码 `target/debug/codex` + 真实 key 实测通过：
    - `cargo test -p codex-tui --test all 'suite::provider_live_compact::deepseek_live_manual_compact_smoke' -- --ignored --exact --nocapture`
    - `cargo test -p codex-tui --test all 'suite::provider_live_compact::glm_live_manual_compact_smoke' -- --ignored --exact --nocapture`
    - `cargo test -p codex-tui --test all 'suite::provider_live_compact::kimi_live_manual_compact_smoke' -- --ignored --exact --nocapture`
    - `cargo test -p codex-tui --test all 'suite::provider_live_compact::minimax_live_manual_compact_smoke' -- --ignored --nocapture --test-threads=1`
    - `cargo test -p codex-tui-app-server --test all 'suite::provider_live_compact::deepseek_app_server_live_manual_compact_smoke' -- --ignored --exact --nocapture`
    - `cargo test -p codex-tui-app-server --test all 'suite::provider_live_compact::glm_app_server_live_manual_compact_smoke' -- --ignored --nocapture --test-threads=1`
    - `cargo test -p codex-tui-app-server --test all 'suite::provider_live_compact::kimi_app_server_live_manual_compact_smoke' -- --ignored --exact --nocapture`
    - `cargo test -p codex-tui-app-server --test all 'suite::provider_live_compact::minimax_app_server_live_manual_compact_smoke' -- --ignored --nocapture --test-threads=1`
- 截至本轮真实 key + 隔离 `CODEX_HOME` + 当前源码 binary 的最终矩阵：
  - `deepseek`：完全跑通。基础对话、skills、MCP、子代理、token 计数、自动 compact、`--search`、legacy TUI `/compact`、app-server TUI `/compact` 均已实测通过。
  - `glm`：完全跑通。基础对话、skills、MCP、子代理、token 计数、自动 compact、`--search`、legacy TUI `/compact`、app-server TUI `/compact` 均已实测通过。
  - `kimi`：完全跑通，但存在厂商侧限流波动。基础对话、MCP、子代理、token 计数、自动 compact、`--search`、legacy TUI `/compact`、app-server TUI `/compact` 已实测通过；曾出现一次 seed 阶段 `429 Too Many Requests`，重试后恢复，不归因于适配层协议错误。
  - `minimax`：核心 workflow 跑通，但搜索仍有边界。基础对话、skills、MCP、子代理、token 计数、自动 compact、legacy TUI `/compact`、app-server TUI `/compact` 已实测通过；`--search` 能完成任务，但检索质量明显弱于其余三家，多次返回 `No web results found`，需 repeated search + compact handoff 才稳定收敛，应记为“可用但有边界”。
- 验证结果：
  - 代码与测试路径已核对完成；尝试执行 `cargo test -p codex-core auto_compact_persists_rollout_entries -- --exact` 与 `cargo test -p codex-tui slash_compact_eagerly_queues_follow_up_before_turn_start -- --exact` 时，当前机器在链接阶段因 `No space left on device (os error 28)` 失败，未继续扩大编译范围。
- 已将四家中国 provider 的内置接入从错误的 Responses API 假设改成 `chat/completions` 兼容层，并新增 `codex-api` 的 chat endpoint / request builder / SSE 解析。
- 修复了生产路径中的关键问题：`ThreadManager` 现在会按当前选中的 provider 初始化 `ModelsManager`，不再固定用 OpenAI 模型目录。
- DeepSeek 内置模型目录改为官方别名 `deepseek-chat` / `deepseek-reasoner`；保留 `deepseek-chat-thinking` 和 `deepseek-thinking` 作为兼容别名，并在模型元数据层同步映射到 `deepseek-reasoner`，避免旧配置落回 fallback metadata。
- Kimi 内置目录改为 `kimi-k2.5`，并按官方 `/models` 元数据标记图像输入能力；环境变量以 `MOONSHOT_API_KEY` 为准，同时兼容旧的 `KIMI_API_KEY`。
- MiniMax 内置目录改为 `MiniMax-M2.7` / `MiniMax-M2.7-highspeed`，并在 chat 请求里启用 `reasoning_split=true`。
- 删除了已经失效的 `codex-rs/core/chinese_models.json` 和 6 份重复/过期中文文档，只保留一份准确的说明文档和精简版 `README_CN_ADAPTER.md`。
- README 现已额外写明相对上游仓库的差异：四家 provider 统一走 `chat/completions`、非 OpenAI compact 走本地路径、模型元数据按 provider 加载、保留旧别名和旧环境变量兼容。
- 已检查 `orchestrator`、`collaboration_mode`、`compact`、`memories` 等核心 prompt/template；结论是提示词本身基本通用，真正的边界在 chat 兼容层目前只完整支持 function tools。
- 已将 memory 相关模板里少量 OpenAI/ResponsesAPI 示例改成通用表述，避免 README 写“通用”但模板示例仍然带上游专有语义。
- 本轮先清理了可重建构建产物：`codex-rs/target` 初始约 `15G`，debug 安装验证完成后再次清掉；当前整个仓库约 `414M`，独立安装的 `codex-cn` 二进制约 `265M`。
- 已安装独立测试入口：
  - 二进制：`~/.local/codex-cn/bin/codex-cn`
  - 包装脚本：`~/.local/bin/codex-cn`
  - 隔离配置目录：`~/.codex-cn-test`
  - 隔离 sqlite 目录：`~/.codex-cn-test/sqlite`
- 包装脚本固定导出 `CODEX_HOME=~/.codex-cn-test` 和 `CODEX_SQLITE_HOME=~/.codex-cn-test/sqlite`，因此不会复用现有 `~/.codex`。
- `codex-cn` 现已默认使用 DeepSeek：
  - `~/.codex-cn-test/config.toml` 已设为 `model_provider = "deepseek"` 和 `model = "deepseek-chat"`
  - DeepSeek API key 仅存于 `~/.codex-cn-test/env.sh`，权限已收紧到 `600`
  - `~/.local/bin/codex-cn` 会在启动时仅从这个隔离目录加载该 key
- 本轮已修复 DeepSeek chat 兼容层中的消息角色问题：
  - `codex-rs/codex-api/src/requests/chat.rs` 现在会把不支持 `developer` role 的 chat provider 自动映射为 `system`
  - OpenAI chat 路径保留 `developer` role，不改变原行为
  - 已新增 DeepSeek / OpenAI 两个回归测试覆盖该行为
- 本轮继续修复了 DeepSeek chat 兼容层里的 `tool_call_id` 配对问题：
  - `LocalShellCall` 在 chat 请求中现在优先使用真正会被 output 跟踪的 `call_id`，只有缺失时才回退到 legacy `id`
  - `CustomToolCall` 不再错误使用消息项 `id` 作为 chat `tool_calls[].id`，统一改为 `call_id`
  - 已新增 `local_shell_calls_prefer_call_id_for_tool_messages` 和 `custom_tool_calls_use_call_id_for_tool_messages` 两条回归测试
- 本轮又修复了另一种真实交互顺序下的 DeepSeek chat 序列化问题：
  - 旧会话日志证明模型会先发 `function_call`，再补一条 assistant 文本，最后才有 `function_call_output`
  - `codex-rs/codex-api/src/requests/chat.rs` 现在会把这种 assistant 文本并入同一条带 `tool_calls` 的 assistant message，保证后续 `tool` output 仍然紧跟其后
  - 已新增 `assistant_text_after_tool_call_merges_into_tool_call_message` 回归测试覆盖这条真实顺序
- 本轮为 chat/completions provider 增加了本地 `web_search` 兼容层：
  - `ToolSpec::WebSearch` 在 chat provider 下会转换成名为 `web_search` 的 function tool，而不是在工具列表里丢失
  - 新增 `WebSearchHandler`，支持 `search`、`open_page`、`find_in_page` 三种动作，内部使用 DuckDuckGo HTML 结果页和页面抓取
  - `allowed_domains`、`search_context_size` 会沿用现有 `web_search_config` 并在本地 handler 中生效
- README / 中文文档现已明确说明：
  - 中国 provider 下 `web_search = "live"` / `--search` 走本地兼容 adapter
  - 仍不等价于原生 Responses `web_search`：`cached`、图片搜索、原生 web-search 事件语义仍未完全复刻
- 这轮还克隆并审查了 `jina-ai/node-DeepResearch`（本地路径 `../node-DeepResearch`），没有照搬整套 deep research agent，而是只迁移到当前 `web_search` handler 最有价值的几层逻辑：
  - 长查询的轻量 query expansion
  - 搜索结果 URL 归一化、去重与单 host 限流
  - `open_page.focus` 的 query-aware passage extraction
- 这轮实现额外参考了高质量开源 agent：
  - `crush` 的 DuckDuckGo 搜索适配思路被吸收进本地 `web_search` handler，包括随机请求头和轻量节流
  - `opencode` 没有做通用 web search，而是走 `fetch` + `sourcegraph` 专用代码搜索；因此当前实现采用更贴近 Codex 语义的 `search/open_page/find_in_page` 三段式，而不是只做单一搜索函数
- 已验证：
  - `command -v codex` 仍然是 `/opt/homebrew/bin/codex`
  - `command -v codex-cn` 是 `~/.local/bin/codex-cn`
  - `codex-cn --version` 可正常运行，且在删除仓库内 `target` 后仍可运行，说明已独立于构建目录安装完成
  - `rg` 已确认隔离配置为 DeepSeek 默认模型
  - 包装脚本加载测试返回 `deepseek_key_loaded`
  - `cargo test -p codex-api` 全量通过，新增的 `requests::chat::tests::deepseek_maps_developer_messages_to_system` 通过
  - 修复后的 `codex-cn` 二进制已重新安装，并在再次清理 `codex-rs/target` 后仍可运行
  - `cargo test -p codex-api` 现为 78 个单元测试、4 个 client 集成测试、1 个 models 集成测试、5 个 realtime websocket E2E、1 个 SSE E2E 全部通过
  - 新安装的 `~/.local/codex-cn/bin/codex-cn` 已包含本轮 `tool_call_id` 修复，`codex-cn --version` 通过
  - `cargo clean` 已回收 `15.5GiB` 构建产物；当前仓库约 `414M`，`codex-rs` 目录约 `42M`
  - 真实 DeepSeek 路径验证通过：`codex-cn exec --ephemeral --dangerously-bypass-approvals-and-sandbox -C /Users/liuliming/code/codex/codex "运行 pwd，并只输出命令结果，不要解释。"` 已成功触发 shell 工具并完成返回，不再出现 `tool_calls` / `tool_call_id` 配对错误
  - `nanochat` 目录下昨日可复现报错的提示词 `扫描这个项目` 现在已可完整跑通，多轮 `exec` 工具调用后正常给出总结
  - 恢复旧报错会话 `019d25bd-b2f8-7453-9a93-9b3cb4c4c469` 后再次提问也已成功返回，说明不是“还在用原版”，而是历史序列化缺口已补上
  - `cargo test -p codex-core web_search --lib` 通过，新增测试覆盖了 DuckDuckGo 结果解析、跳转 URL 解码、HTML 文本抽取、`allowed_domains` 提示，以及 chat provider 下 `web_search` -> function tool 的转换
  - 同一组 `cargo test -p codex-core web_search --lib` 最新为 36/36 通过，并新增覆盖 `installation instructions` 命中 `install` 页面片段的回归测试
  - `cargo test -p codex-tui tests::read_session_cwd_prefers_sqlite_when_thread_id_present` 通过，`cargo test -p codex-tui-app-server tests::embedded_app_server_supports_thread_start_rpc` 通过
  - 全量 `cargo test -p codex-tui` 与 `cargo test -p codex-tui-app-server` 已成功完成编译并跑过绝大多数测试；尾部现有慢测试长期不退出，因此没有等待到最终自然结束，不将其视为本轮联网搜索改动的回归信号
  - 已重新构建并回装隔离版 `codex-cn`，随后再次 `cargo clean` 回收 `13.1GiB`；当前仓库 `codex-rs` 约 `42M`，隔离二进制约 `343M`
  - DeepSeek 实机验证通过：
    - 会话文件 `~/.codex-cn-test/sessions/2026/03/26/rollout-2026-03-26T01-41-27-019d2616-47f7-70f1-b51b-daa3996bf5eb.jsonl` 明确记录了 `function_call name="web_search"`，参数为 `{"action":"open_page","url":"https://httpbin.org/uuid"}`，并有对应 `function_call_output`
    - 会话文件 `~/.codex-cn-test/sessions/2026/03/26/rollout-2026-03-26T01-41-52-019d2616-a9ab-7990-8a7a-97cd2abc0331.jsonl` 明确记录了 `function_call name="web_search"`，参数为 `{"action":"search","query":"openai/codex GitHub repository","max_results":1}`，输出中包含 `https://github.com/openai/codex`
    - 会话文件 `~/.codex-cn-test/sessions/2026/03/26/rollout-2026-03-26T12-02-01-019d284e-6ab0-7b83-8e79-128195291a59.jsonl` 第 8 行确认模型主动调用了 `web_search search`，第 17 行确认 `focus = "installation instructions"` 时 `open_page` 已返回 `Focused excerpts ...`，不再直接灌整页页头
- 已执行：
  - `cargo test -p codex-api`：chat 相关新增测试通过；完整 crate 中 5 个 realtime websocket 测试因沙箱禁止本地 `bind` 失败，与本次修改无关。
  - `cargo test -p codex-core`：1645 个测试通过；失败项仍然是既有沙箱相关问题（wiremock 绑定端口、managed loopback proxy、`sandbox-exec`、系统配置 API），不适合作为本次 README / template 改动的结果判定。
  - 直接相关子集测试通过：`cargo test -p codex-api chat:: --lib`、`cargo test -p codex-core deepseek_provider_uses_provider_specific_bundled_catalog --lib`、`cargo test -p codex-core kimi_provider_marks_built_in_model_as_multimodal --lib`、`cargo test -p codex-core new_seeds_models_manager_from_selected_provider --lib`、`cargo test -p codex-core model_providers_reject_reserved_built_in_ids --lib`、`cargo test -p codex-core test_deserialize_chat_wire_api --lib`。
  - 本轮 prompt/template 定向测试通过：`cargo test -p codex-core memories::prompts:: --lib`、`cargo test -p codex-core collaboration_mode_presets:: --lib`。
  - 独立安装验证通过：`cargo build --release -p codex-cli`、`codex-cn --version`。
- 已执行 `just fix -p codex-api`、`just fix -p codex-core`、`just fmt`。
- 已执行 `just fix -p codex-tui`、`just fix -p codex-tui-app-server`、`just fmt`。
- `just argument-comment-lint` 无法执行：仓库当前缺少 `./tools/argument-comment-lint/run-prebuilt-linter.sh`。
- 本轮补充验证：
  - `cargo test -p codex-core deepseek_legacy_thinking_alias_uses_reasoner_metadata --lib` 通过。
  - `cargo test -p codex-core models_manager::manager::tests:: --lib` 中与本次改动直接相关的 10 个测试通过；其余 6 个失败仍然是 `wiremock` 在沙箱下无法绑定本地端口。
- 本轮恢复了 chat provider 的 token 计数链路：
  - `codex-rs/codex-api/src/requests/chat.rs` 现在会为 DeepSeek / GLM / Kimi / MiniMax 的流式 chat 请求携带 `stream_options.include_usage = true`
  - `codex-rs/codex-api/src/sse/chat.rs` 会把最终 chunk 的 `usage` 累积并透传到 `ResponseEvent::Completed`
  - `codex-rs/core/src/codex.rs` 在 provider 没回 usage 时会立即走本地 `recompute_token_usage()` 兜底，因此 footer token 计数和 auto compact 判断都不再卡死在旧值
- 本轮已用真实 DeepSeek 会话确认自动压缩重新触发：
  - 临时隔离目录 `~/.codex-cn-compact-check` 下第一轮 `exec` 的 `turn.completed` 已返回 `input_tokens = 16050`
  - 同一线程第二轮 `resume --last` 的 rollout 文件 `~/.codex-cn-compact-check/sessions/2026/03/26/rollout-2026-03-26T13-33-17-019d28a1-fa76-7eb1-aa9b-634d5a927199.jsonl` 第 14 行记录了 `type":"compacted"`，说明 pre-sampling compact 已经在 chat provider 路径下恢复
  - 同一 rollout 中压缩前后的 `token_count` 事件也已更新，不再停留在固定值
- 本轮已用真实 DeepSeek 会话确认 GitHub / 文档站专用解析生效：
  - GitHub 验证文件 `~/.codex-cn-search-check/sessions/2026/03/26/rollout-2026-03-26T13-35-45-019d28a4-3cd2-7a03-98f8-34639840fad8.jsonl` 第 8 行记录了对 `https://github.com/openai/codex/blob/main/README.md#install-from-source` 的 `open_page`
  - 同一文件第 12 行返回 `Title: README.md`、`Content-Type: text/plain` 和 `Focused excerpts for \`install from source\``，说明 GitHub blob 已按 raw 文本 + fragment focus 读取，而不是整页 chrome
  - DeepSeek 文档验证文件 `~/.codex-cn-search-check/sessions/2026/03/26/rollout-2026-03-26T13-36-48-019d28a5-3467-7e52-adc1-2b1a2ab7e840.jsonl` 第 12 行返回 `Title: Create Chat Completion | DeepSeek API Docs`、`Content-Type: text/html` 和 `Focused excerpts for \`request body\``
- 本轮重新安装的是最新源码编出的 `debug` 版 `codex-cli`，已覆盖到 `~/.local/codex-cn/bin/codex-cn`：
  - `release` 版重新构建在当前机器上会卡在 fat LTO，且清空 `target` 后重新编译一度受 `rusty_v8` 下载 SSL 校验影响，因此这轮为了尽快完成隔离验证，改为本地缓存 `rusty_v8` 归档后编出 `debug` 二进制回装
  - 安装后的 `/Users/liuliming/.local/codex-cn/bin/codex-cn --version` 返回 `codex-cli 0.0.0`
  - 回装和验证完成后再次执行 `cargo clean`，本轮总共回收了约 `41.9GiB`，当前 `codex-rs/target` 已不存在，磁盘可用空间约 `26GiB`
- 本轮用真实 key 对另外三家 provider 做了隔离回归：
  - GLM `glm-5`：基础对话通过；skill 可读取 `test-tui` 并返回 `RUST_LOG="trace"` / `just codex` / `log_dir`；子代理通过；MCP `mcp__rmcp__echo` 可用，但会比 MiniMax/Kimi 更容易重复调用一次；token usage 在 `turn.completed` 与 rollout `token_count` 都会更新；自动 compact 已确认触发，rollout `rollout-2026-03-26T15-41-39-019d2917-8239-7963-b20d-dd105ead3446.jsonl` 含 `type":"compacted"`。
  - Kimi `kimi-k2.5`：基础对话通过；子代理通过；MCP `mcp__rmcp__echo` 通过；token usage 在 `turn.completed` 与 rollout `token_count` 都会更新；自动 compact 已确认触发，rollout `rollout-2026-03-26T15-45-46-019d291b-44f6-75e2-a03c-3e81d662bae9.jsonl` 含 `type":"compacted"`。但 skill 场景稳定性较差：完整 `test-tui` 摘要提示两次命中 `429 Too Many Requests`，极简 skill 提示则出现反复读 skill / 误用文件变更工具的行为，尚未判定为适配层 bug 还是模型行为问题。
  - MiniMax `MiniMax-M2.7`：基础对话通过；skill 可读取 `test-tui` 并返回目标字段，但推理冗长、token 消耗显著高于另外两家；MCP `mcp__rmcp__echo` 通过；子代理通过；token usage 在 `turn.completed` 与 rollout `token_count` 都会更新；自动 compact 已确认触发，rollout `rollout-2026-03-26T15-29-48-019d290c-a601-7d72-8828-c938bcba066e.jsonl` 含 `type":"compacted"`。
- 本轮修复了两条真实 provider 兼容 bug：
  - Kimi / MiniMax 的 streaming chat `usage` 不是总在顶层即时返回：Kimi 可能把 `usage` 放在最终 `choice` 或 trailing usage-only chunk，MiniMax 会先发 `usage: null` 再补 trailing usage。`codex-rs/codex-api/src/sse/chat.rs` 现在会忽略空 usage、读取 `choice.usage`，并等到 trailing usage chunk 到达后再发 `Completed`，因此 token 计数和 auto compact 已在三家 chat provider 上恢复。
  - MiniMax 在 resume / compact 之后要求 `system` 只能出现在消息最前面。`codex-rs/codex-api/src/requests/chat.rs` 现在会把后续出现的 `system` / `developer` 指令提升并合并到首条 system message，解决了真实恢复线程时报的 `invalid message role: system (2013)`。
- 本轮补齐了四家 provider 的子代理实测：
  - DeepSeek 真实会话 `/tmp/deepseek_subagent_check.jsonl` 已记录 `spawn_agent -> wait -> subagent=43`。
  - 结合前面三家的隔离实测，DeepSeek / GLM / Kimi / MiniMax 的子代理协作路径都已确认可用；它们走的是普通 function/collaboration tools，不依赖厂商私有 agent API。
  - 当前观察到的问题不在子代理本身，而在部分模型的工具行为：GLM / Kimi 更容易重复调用 MCP 工具，Kimi 在长 skill 提示上更容易 429 或跑偏，MiniMax tool/skill 推理更冗长。
- 本轮确认了用户提到的 “compact 后 context left 变成 100%” 的根因：
  - 不是 compact 后 token 被清零，而是上游 footer 百分比算法对低于固定 baseline 的上下文会继续显示 `100% context left`。
  - 为避免误导，TUI 和 app-server TUI 现在在这种情况下会同时显示实际上下文占用，例如 `100% context left · 9.88K used`。
- 本轮把 MiniMax 官方文档里值得保留的适配点补进文档：
  - 被动 Prompt cache 不需要额外请求字段；当前适配层已经尽量维持缓存友好的前缀顺序（工具定义顶层、system 在最前、动态消息在最后）。
  - `usage.prompt_tokens_details.cached_tokens` 已经被解析成 `cached_input_tokens` 并进入 turn metrics / telemetry，但当前 `/status` 和 footer 还没有单独拆出缓存命中展示。
- 手动 `/compact` 的交互式 PTY 自动化本轮没有拿到可复用的稳定脚本：TUI 的 trust prompt、启动参数和 slash-command 输入时机在自动驱动下仍不稳定。共享 compact 逻辑本身已经通过三家的 auto compact 证明可用，但 `/compact` 这条交互入口仍需单独补稳定验证。
- 本轮进一步补了搜索和缓存可观测性：
  - `codex-rs/core/src/tools/handlers/web_search_processing.rs` 新增了对 `owner/repo` 类 GitHub 仓库查询的本地 fallback；当外部搜索源返回空结果，但 query 本身已经明确包含仓库 slug 且带 `github/repo/repository` 语义时，会直接返回 canonical `https://github.com/<owner>/<repo>` 结果，避免 DuckDuckGo HTML 空结果把 agent 卡死在“无结果”分支。
  - 针对这条 fallback 已补单元测试 `fallback_search_results_infers_github_repo_from_query`，并通过 `cargo test -p codex-core web_search --lib`。
  - `codex-rs/tui/src/status/card.rs` 和 `codex-rs/tui_app_server/src/status/card.rs` 现在会在 `/status` 中单独显示 `Prompt cache` 行；`Token usage` 仍保持显示非缓存 input + output，缓存命中量则单独展示，避免把 MiniMax 的被动 prompt cache 命中隐藏掉。
  - 对应状态卡测试和 snapshot 已更新并通过：`cargo test -p codex-tui status::tests:: --lib`、`cargo test -p codex-tui-app-server status::tests:: --lib`。
- 本轮补齐了 DeepSeek 的专项 smoke：
  - 独立 MCP smoke 文件 `/tmp/deepseek_mcp_precise.jsonl` 已记录 `mcp_tool_call server=rmcp tool=echo`，返回 `structured_content.env = "deepseek-isolated"`，最终 agent 回复 `deepseek-isolated`，并带 `turn.completed` usage。
  - 独立 skill smoke 文件 `/tmp/deepseek_skill_precise.jsonl` 已记录读取 `.codex/skills/test-tui/SKILL.md`，最终 agent 回复 `RUST_LOG="trace" just codex -c log_dir=/tmp/codex_logs_2D2upJ`，说明 skill 读取和基于内容生成指令可用。
- 本轮对三家 `--search` live regression 的结论仍然保守：
  - GLM 已在真实 rollout `rollout-2026-03-26T16-19-44-019d293a-5cfd-7ea3-a805-b7ca6ad9ab7d.jsonl` 中证明过真实 `function_call name="web_search"` 和 `function_call_output` 链路会发生；当时外部搜索源返回了 `No web results found`，这正是本轮 GitHub fallback 修复要覆盖的情况。
  - 但修复后的 `glm` / `kimi` / `minimax` 在 `exec --search` 下仍存在模型侧是否稳定主动选用 `web_search` 的波动，当前更像模型行为稳定性问题，而不是工具协议已经断裂；因此三家的 live search 仍未全部记为“稳定全绿”。
