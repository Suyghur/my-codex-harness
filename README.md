# my-codex-harness

Rust CLI，用于集中管理自定义 Codex harness 的角色资产、部署工具与评估约定。`agents/` 是子智能体定义的维护来源，通过符号链接供本地 Codex 使用；`harness.toml` 声明角色集合、模式和版本。

```text
agents/       独立可用的 Codex 角色定义
harness.toml  角色资产清单与版本
Cargo.toml    Rust 包、二进制与依赖
Cargo.lock    精确依赖解析
rust-toolchain.toml  固定构建工具链
src/main.rs   进程入口
src/cli.rs    根参数与命令分发
src/commands/ 按 check/install/version 子命令分领域目录
src/shared/   共用资产、指纹与路径能力
tests/        真实二进制隔离场景
templates/    按需使用的任务交接和续接记录
skills/       独立可安装的 Codex skills
evals/        尚未执行的行为回归种子
docs/         工程边界、设计依据和行为评估方法
```

## 子智能体

| 角色 | 负责领域 | OpenSpec 职责 |
| --- | --- | --- |
| `explorer` | 只读调查本地实现、规范、调用关系与影响范围 | Explore 及各阶段的事实调查 |
| `librarian` | 核实外部契约、库版本、厂商能力与适用条件 | 为探索、设计、实施和核验提供第一方证据 |
| `plan` | 需求缺口分析、架构诊断、方案选择和规划工件 | Explore / Propose / Update |
| `worker` | 范围明确的实施或场景 QA，分配时明确模式 | Apply；明确分配时执行 Sync / Archive |
| `reviewer` | 独立审查计划可执行性、实现质量或验收证据 | Verify |

主智能体负责目标、路由、授权、文件所有权和最终验证。按实际问题选择角色，不要求每个任务运行完整流水线。QA 不自动修复，审查不修改代码或工件；同一文件由明确的单一写入者负责。

OpenSpec 项目遵循本地 schema、技能和 change 上下文；未采用 OpenSpec 的项目继续使用通用职责，不自动初始化或创建变更。

## 构建与使用

需要 Rust 1.95.0（见 `rust-toolchain.toml`）；Cargo 依赖解析由 `Cargo.lock` 固定。支持 macOS / Linux 的本地符号链接安装。

在仓库根目录直接运行，或构建二进制：

```sh
cargo run --locked -- check
cargo run --locked -- install --dry-run
make build
./target/release/my-codex-harness --help
```

将 CLI 安装到 Cargo 的 bin 目录（默认 `~/.cargo/bin`），并将该目录加入 PATH：

```sh
cargo install --path . --locked
my-codex-harness version
```

二进制和角色资产分开：默认从当前目录向上查找 `harness.toml`。在仓库外运行时指定来源，角色定义变化不需要重新编译：

```sh
my-codex-harness check --root /path/to/my-codex-harness --json
my-codex-harness install --root /path/to/my-codex-harness --dry-run
my-codex-harness install --root /path/to/my-codex-harness --codex-home /path/to/codex-home
```

`install` 将清单中的定义链接到 `${CODEX_HOME:-~/.codex}/agents/`，显式参数优先。完整预检后才写入；重复运行跳过正确链接。已有普通文件或其他链接全部先备份到目标配置目录的 `backups/my-codex-harness/`，再逐个原子替换。dry-run 不创建文件或目录，不处理清单外角色，不修改 `config.toml`。多文件安装不是事务。预检失败不写入；备份执行失败会保留已产生的目录和备份，但不会替换任何角色；替换中途失败可能已有部分角色完成，保留备份供检查和恢复，修复错误后可重新运行。

退出码：`0` 成功，`1` 资产检查或安装失败，`2` 命令/参数解析错误。`version` / `--version` 显示 CLI 版本，`check` 返回清单资产版本；二者独立维护。

命令以 clap 类型化参数组织，`--root` 可放在子命令前后。版本输出包含运行平台；`my-codex-harness version --json` 可读取版本、OS、架构和 构建时 Rust 编译器信息（`rust_version`）。OS/架构使用 Rust 目标名称，例如 `macos/x86_64` 或 `linux/aarch64`；不再输出 `go_version`。结果写 stdout，诊断写 stderr，JSON 输出保持可解析。

Codex 可从用户级 `agents/` 目录发现独立 TOML 定义，无需逐个注册。文件中的 `name` 是角色标识；`description` 用于角色选择，`developer_instructions` 定义执行行为。参见 [Codex 自定义子智能体说明](https://learn.chatgpt.com/docs/agent-configuration/subagents)。

后续直接编辑本仓库的 `agents/*.toml`。如果移动仓库，请重新运行 `my-codex-harness install`。恢复时将备份放回原目标位置，链接须保留原始字面量，避免解引用相对链接或断链。

链接使用仓库当前内容；需要固定版本时先检出固定 commit 的独立副本，再通过 `--root` 安装该副本。模型可用性及真实权限由当前 Codex 会话决定，清单中的 access 不是 sandbox 配置。

## Skills

[`init-deep`](skills/init-deep/SKILL.md) 改编自 oh-my-openagent，用当前实现和目录复杂度创建或更新分层 `AGENTS.md`。支持默认增量更新、`--create-new` 重建和 `--max-depth=N`；保留现有用户约定，工具缺失时仍可调查，不依赖本仓库角色已安装。固定上游版本、适配差异和许可随 skill 保存。

CLI 当前只管理角色资产，`check` / `install` 不包含 skills。可在仓库根目录单独链接此 skill；若目标已存在，`ln` 会失败，先核对现有内容再处理：

```sh
mkdir -p "${CODEX_HOME:-$HOME/.codex}/skills"
ln -s "$PWD/skills/init-deep" "${CODEX_HOME:-$HOME/.codex}/skills/"
```

在可发现该 skill 的 Codex 会话中使用 `$init-deep`，例如：`使用 $init-deep --max-depth=2 更新当前仓库指导`。安装使用仓库当前内容；需要固定版本时从独立固定副本链接。行为回归判据见 [init-deep 场景](evals/init-deep.md)。

## 检查与评估

```sh
make check
make test
cargo run --locked -- check --json --scenarios
cargo run --locked -- check --codex-home /path/to/codex-home
git diff --check
```

校验器只读检查角色、路径、必需字段及可选的安装链接，并生成当前资产指纹；不运行模型、不宣称行为评估通过。`--scenarios` 仅核查场景结构与角色路由。GitHub Actions 配置了 Linux/macOS 的 Rust 检查、隔离测试与 release 场景验证。

`make check` 包含 rustfmt、clippy、测试、资产场景检查与 diff 检查；`make test` 运行 Rust 测试，不宣称替代 Go race 检查。`make build` 生成 `target/release/my-codex-harness`。

bundle 指纹带 `fingerprint_algorithm`，当前为 `sha256-json-v3`；不与旧算法 bundle 值直接比较，单文件/提示词 SHA-256 不变。完整编码规则见 [工程设计依据](docs/architecture.md)。

角色或协作规则发生实质变化时，按 [行为评估方法](docs/evaluation.md) 执行 [种子场景](evals/scenarios.json)。长任务按需使用 [交接模板](templates/task-handoff.md) 和 [续接模板](templates/continuation.md)，已有 OpenSpec 状态直接引用，不重复建任务系统。

本轮工程取舍参考 [阿里云 AI Agent Handbook](https://github.com/aliyun/ai-agent-handbook)，具体章节与固定版本见 [工程设计依据](docs/architecture.md)。

CLI 按子命令领域独立设计，共用资产能力集中维护；以功能规范和真实进程场景验证完整性。

不复制用户的完整 `config.toml` 或全局 `AGENTS.md`，不实现模型运行时，也不自动初始化目标项目的 OpenSpec。
