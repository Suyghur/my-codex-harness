# Harness 工程约定

本仓库是 Rust CLI 工程，管理 Codex 角色资产与本地部署工具，不实现模型调用、任务调度、沙箱或审批运行时。

- `agents/*.toml` 是角色定义的维护来源；`harness.toml` 声明角色集合、模式与资产版本。新增或删除角色时同步维护两处及行为评估场景。
- 角色提示词必须能独立使用，不假设用户克隆了本仓库或已安装特定插件。模式和 access 是行为约定，不能描述为运行时权限强制。
- 保留已有用户改动。当前角色与模型设置未经任务要求不要变更，不恢复已删除的 default 或旧框架注册。
- OpenSpec 遵循目标项目实际配置和 schema；不要在本工程或目标工程自动初始化、创建变更或强制完整流水线。
- 安装只处理清单中的角色，先完整预检，冲突文件先备份，单个链接原子替换；dry-run 不应创建目录或文件，不修改用户完整 config.toml。
- `src/main.rs` 维护可执行入口，`src/cli.rs` 负责根参数和分发；按 check/install/version 子命令在 `src/commands/` 分领域目录，安装领域逻辑位于 install/domain.rs，共用资产、指纹与路径能力位于 `src/shared/`。单元测试邻近模块，`tests/` 包含真实二进制场景。角色资产保持外置，跨目录运行用 `--root` 指定来源。
- CLI 名称是 `my-codex-harness`，使用 clap 类型化命令；每次调用无共享可变参数状态。参数、输出和退出码留在命令边界，文件系统逻辑留在对应领域。结果写 stdout，诊断写 stderr，不污染 JSON。实现按 Rust 类型和所有权设计，不翻译旧语言代码。
- 验证使用 `make check`、`make test`、`make build` 和 `git diff --check`。涉及安装时测试隔离目录与子进程环境，不修改全局 HOME/CODEX_HOME/CWD 或真实用户配置。
- CLI 依赖通过 Cargo 管理并提交 Cargo.toml/Cargo.lock；工具链以 rust-toolchain.toml 和 CI 为准，不保留 Go 或 Python 的并行实现。v3 指纹编码调整必须验证固定向量，编码变化须升级算法标识。
- 结构检查和行为评估分开报告；没有实际运行模型时不得宣称角色行为通过。修复已确认 Badcase 后保留对照场景和观察依据，不用提示词包含某字符串的测试代替行为测试。
- `work/` 存放临时研究；`.harness/` 存放明确需要保存的运行与评估记录，两者均不提交。不要存储凭证、原始私密上下文或写入用户长期记忆。
