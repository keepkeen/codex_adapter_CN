# Lessons

- 当用户补充“发布名/产品名不能和 upstream 一样”这类身份约束时，必须立刻把包名、二进制名、仓库文档名和发布目标一起纳入变更范围，不能只盯着功能适配和测试通过。
- 对外命令名可以改成 `codex-cn`，但安装层不能把 `codex` 本体也直接改名执行；这个仓库依赖 `arg0` 分发，包装脚本应保留外部别名，内部仍执行原名 `codex` 二进制。

- 当用户先纠正执行顺序时，先把新优先级写进 `tasks/todo.md`/`tasks/lessons.md`，再继续实现，避免后续工作和用户的最新约束脱节。
- 涉及磁盘紧张的仓库改造时，先清理 `target/` 这类可重建产物，再继续跑大编译和测试，避免验证过程把环境先拖死。
- 当用户要求 README 说明“和上游/原仓库不同的点”时，不能只写新增支持项；要显式写清行为路径改了什么、哪些能力仍有边界，以及这些边界是 prompt 问题还是工具协议问题。
- 当 provider 兼容修复在真实 API 上仍报错时，不能把第一次局部修复当作完成；要继续沿真实报错链条追到下一层协议约束，并补 live-path 对应的回归测试。
- 当用户报告“还是不行”时，优先核实实际运行的命令路径和会话状态；新二进制验证通过并不代表交互端没有复用旧会话或旧历史。
- 当用户提醒“可以参考高质量开源实现”时，不要只停留在本仓库猜设计；应先去看强相关项目的源码实现，尤其是网络工具、解析器和 agent workflow 这类已有成熟先例的模块。
- 当用户指出“自动压缩没触发、token 计数不变”时，不能只盯 UI；要顺着 `request -> SSE parser -> ResponseEvent::Completed -> Session token_info -> TUI/footer` 整条链路排查，确认 usage 是否真的回写到 core。
- 当用户明确要求“不要 fallback / 不要后备模型”时，后续修复必须保持 provider 和模型语义严格一致；宁可暴露真实不兼容，也不要为了跑通而偷偷改成降级路径。
- 当用户纠正“当前/最新模型”时，必须以用户给出的官方页面为准重新核对内置目录和文档，不要沿用之前的搜索印象或稳定别名自作主张。
- 当用户指出 compact 后的 `context left` 显示可疑时，除了核对 token 统计链路，还要检查展示公式本身；上游固定 baseline 会让已压缩但仍较小的上下文继续显示 `100%`，需要用绝对 token 占用补足可观测性。
- 当用户指出“你丢失了上下文”时，下一步先回到 `tasks/todo.md`、当前 diff 和已完成 live 验证重新对账，再继续设计或回答，不能只凭短期记忆复述剩余工作。
- 当用户要求“面向未来维护”时，抽象目标不能只覆盖当前四家 provider 的现状；还要把 upstream 新 feature 的接入点、测试入口和能力矩阵一起设计成中心化扩展面。
- 当用户要求“完整覆盖”时，不能只覆盖主路径 workflow；像 `artifacts`、`js_repl`、`image_generation`、`output_schema` 这类 secondary features 也要进入 provider support matrix、显式 gating 和自动化测试，而不是只在文档里标记 Unsupported。
- 当用户把某个子能力提升为“必须做”时，要立刻把范围收窄到该能力本身并写回 `tasks/todo.md`，不要继续按原先的“以后再做”优先级回答或实现。
- 当某个 chat provider 的 thinking/tool 协议比另外几家更严格时，不要把“缺字段时直接省略”当成通用默认；要把这类约束提升到 provider dialect/profile 层，用显式能力声明驱动 request builder，否则旧会话 resume 时最容易暴露这类历史回放 bug。
- 对要求“保留 reasoning_content”的 provider，不能只保证字段存在；当多条 `function_call` 被合并成同一条 assistant tool-call message 时，后续分组项和补充 assistant 文本都不能把前面已经拿到的真实推理内容覆盖成空串。
