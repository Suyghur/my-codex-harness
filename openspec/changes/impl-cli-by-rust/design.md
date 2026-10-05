# Design

## Context

动机见 proposal.md。用户明确要求忽略现有 Go 内部实现，从 Rust 的类型、所有权、标准库和生态出发重新设计，同时完整实现当前 CLI 功能。本 change 的三项能力规范、角色/清单/场景数据格式及用户可观察行为是实施依据；前期读过的 Go 代码不成为模块划分、内部 API、算法组织或测试组织的约束。当前构建产物路径与 README 示例不一致，须统一；CI 的语言工具链须替换。

用户确认 CLI 尚未公开，没有外部兼容要求，允许升级 bundle 算法。OpenSpec 当前无主规范，此 change 首次记录现有契约与必要调整。工作区存在大量未提交/未跟踪内容，实施前须保留基线，不能依赖 git checkout 恢复未跟踪文件。

## Goals / Non-Goals

**Goals:**
- 用单个 Rust package 独立设计模块、类型和错误边界，以规范场景证明功能完整，不要求与 Go 文件或函数逐项映射。
- 明确 v3 指纹字节规则，以固定向量验证，避免摘要随路径、键声明顺序或构建平台漂移。
- 用现有场景和真实二进制验证安装行为，以同平台数据评估迁移收益。

**Non-Goals:**
- 不逐文件翻译 Go，不复制 Go 的函数签名、可变结果收集方式或测试辅助函数；不把旧代码是否存在某分支作为 Rust 验收依据。
- 不建设通用插件框架、服务容器、多源配置或异步调度，不将角色 access 解释为运行时权限。
- 不保证安装跨多个文件事务，也不新增并发安装锁或对恶意并发修改的完整防护。
- 不保证 v2 bundle 字节兼容；不修改角色、模型和场景内容以适配测试。

## Decisions

### 1. 单包与薄入口

Cargo package 可沿用仓库名，显式声明 `[[bin]] name = "my-codex-harness"`。采用 Rust 2024 edition，工具链先固定本机已具备的 1.95.0，并在 Cargo.toml 声明 rust-version、提交 Cargo.lock；实施时验证全部选定依赖支持该工具链，CI 使用同一声明。依赖精确解析由 lockfile 固定，不在文档臆测最新版本。

按用户确认的子命令领域分层组织：`src/cli.rs` 仅负责根参数、命令枚举与分发；`src/commands/check/`、`src/commands/install/`、`src/commands/version/` 各自维护参数、用例与输出。安装专属领域行为放在 `commands/install/domain.rs`；多命令共用的资产校验、指纹和路径能力放在 `src/shared/`，避免命令之间重复实现。`main.rs` 处理进程退出，`lib.rs` 只暴露最小可测试入口。单元测试邻近所属领域，`tests/` 按用户场景运行真实进程。这是按 Rust 用例重新设计，不映射 Go 文件或内部 API。采用单包，避免为小工具建立通用框架。

typed CLI 使用子命令枚举，合法的资产数据形成经过校验的值；校验失败通过 Result 传播，输出层将结果映射为外部 JSON/text。路径和模型选项使用 PathBuf/Option 等适当类型，不用字符串约定模拟 Go 内部状态。模块接口按 Rust 使用方式设计，只固定规范要求的用户结果。

使用 clap derive、serde、toml、serde_json、sha2、anyhow、tempfile。thiserror 仅在结构化错误确有消费者时加入，不作为默认依赖。保持同步标准文件操作，无 Tokio、日志或配置合并框架。

### 2. 显式处理解析、输出和退出

命令参数以 OsString/PathBuf 接收，采用可返回错误的 clap 解析 API，统一将参数错误映射为 2、操作错误映射为 1，帮助与版本成功映射为 0。不依赖 `main -> anyhow::Result` 自动格式化以替代 JSON 与退出码契约。核心返回类型化结果，由输出边界进行格式化和写入；仅在写入失败等有价值场景注入 writer，不要求重建 Go 的 Run/NewRoot 接口。每次调用无共享可变参数状态；写入失败返回操作失败，生产路径不使用 unwrap 控制流程。

`--root` 使用全局参数；根级版本显式处理，禁用会绕过空 root 校验的自动版本快捷路径，并拒绝根版本与子命令同时出现。路径层分别处理词法绝对化和需要文件存在的 canonicalize：新 codex-home 不能直接 canonicalize。保留 `~`/`~/` 展开、空路径错误以及上溯发现语义。未知参数属于参数层错误；资产读取/验证留在操作层，使 `check --json` 失败仍产生结果对象。

check 保留字段和空数组，bundle 不存在时省略；Role 的 model/reasoning_effort 缺失保持 null。默认文本仍为中文，帮助排版和底层系统错误措辞无需复现 Cobra。版本 JSON 保留 version/os/arch，增加 rust_version，删除 go_version；build.rs 运行编译工具链的 rustc --version 并注入字符串，失败使构建失败，运行时不执行 rustc。OS/arch 使用 Rust 目标命名并在文档说明，属于未公开接口允许的调整。

### 3. 资产校验与 v3 编码

保持现有 schema_version=1 和 access 集合：read-only、read-only-with-scratch、read-only-with-report、planning-artifacts、task-scoped。TOML typed model 校验角色字段；另保留完整 toml::Value 清单，未知 metadata 必须参加指纹，不使用丢弃未知字段的 typed manifest 生成摘要。

`sha256-json-v3` 的哈希输入定义如下：

- 顶层对象为 `algorithm`、`manifest`、`roles`。algorithm 为算法标识，manifest 为完整解析清单，roles 为与 bundle 输出一致的角色摘要，按 name 升序排列；不包含 bundle fingerprint 本身。
- 将所有对象的键递归按 UTF-8 字节字典序排列，数组保持原顺序；禁止使用 preserve_order 的声明顺序作为摘要顺序。
- TOML 字符串/布尔/整数/有限浮点分别转换为 JSON 对应类型；日期时间转换为解析器的规范 TOML 日期时间字符串（统一日期时间分隔符为 `T`，其余含本地时间/偏移的合法表示由固定 toml 版本决定），递归处理表和数组。NaN/Infinity 在转换阶段返回明确错误，不能默默变成 null。
- 使用锁定 serde_json 版本的紧凑 UTF-8 编码，无 BOM、缩进、尾换行、额外 HTML 转义或 Unicode 归一化；保留 absent 模型设置为 null。摘要为上述字节 SHA-256 的小写十六进制。
- 将编码器版本约束与固定向量写入实现注释；依赖升级必须重跑向量，若编码变化则显式更新算法标识，不在同一算法内改变摘要。

选择 v3 而非重现 Go 编码，是因为旧接口未发布且用户无需旧摘要兼容。这里不声称采用 RFC 8785；整数、浮点和日期时间规则由本算法及固定向量约束。固定样例覆盖嵌套/重排键、未知 metadata、Unicode、HTML 特殊字符、整数/浮点、日期时间、null 模型、末尾空行和跨目录复制；记录编码字节与预期摘要，不只测试“调用两次相等”。

### 4. 安装计划、备份和替换分阶段

从安全规范推导安装执行顺序，而非复刻 Go 的循环和 pending 结构：

```text
load assets --> inspect all targets --> dry-run output
                       |
                       v
                create directories
                       |
                       v
                backup all conflicts
                       |
                       v
                replace each link
```

使用 canonicalize 检查源文件物理边界；component-aware 路径判断而非字符串前缀。目标类型使用 symlink_metadata，避免漏掉断链；判定已安装时解析目标，与规范化源路径比较。允许 agents 目录自身链接到仓库，正确源文件不得被重新链接。

临时目录由 tempfile 在目标 agents 下创建，同文件系统内建立临时 Unix symlink 后 rename 到目标；不能先删除原目标。备份目录位于 codex-home/backups/my-codex-harness，链接用 read_link + symlink 保留原字面量，普通文件独占创建、复制后保留权限并检查写入/关闭（或明确 flush）错误。tempfile 清理仅作用于本次临时链接目录，不能自动删除供恢复的备份目录。

以经过校验的安装计划值记录动作和冲突，在执行前完成检查；Rust 所有权与 RAII 管理本次临时资源，长期备份显式保留。该表示只服务本工具，不增加通用调度或文件系统框架。

前置校验失败零写入；备份执行失败允许已创建目录和部分备份存在，但所有角色目标尚未替换；替换失败允许部分角色完成，备份保留。不新增全局事务。以可注入的窄文件操作边界测试备份失败及第 N 次替换失败，不通过更改真实用户权限制造故障。TOCTOU 风险不因 Rust 消失；不将类型安全当作并发文件安全保证。

### 5. 构建、检查与行为验收

`make build` 执行 `cargo build --release --locked`，产物固定 target/release/my-codex-harness；`cargo install --path . --locked` 作为本地安装方式。make fmt/fmt-check/check/test 的语义在文档明确：check 包含 fmt 检查、clippy 全 targets 且警告失败、cargo test --locked、资产场景检查和 git diff --check；test 执行 cargo test --locked。Rust 不伪造 Go race 检查的等价替代。

release 使用 strip、LTO、单 codegen unit 和 opt-level="s" 作为初始体积配置；保留默认 unwind 策略，不为未经测量的节省采用 panic=abort。CI 在 Linux/macOS 运行检查、release 构建及 release 二进制 check --json --scenarios；安装测试全部隔离 HOME/CODEX_HOME/CWD，并对子进程设置环境，不修改并行测试进程的全局环境。

以三项能力规范建立功能覆盖矩阵，每个规范场景关联 Rust 的测试或验收命令；独立编写 fixture 和断言，不移植 Go 测试代码或依赖其内部接口。测试覆盖真实二进制参数/JSON/退出码、仓库外路径、新目录 dry-run、断链备份与部分失败。旧二进制可作黑盒观察辅助，但已明确变更的指纹/版本字段以新规范为准，不能把旧行为盲目设为期望值。角色场景仍只校验结构，不执行模型，不能报告行为评估通过。

## Risks / Trade-offs

- [clap 默认版本/帮助/参数规则导致退化] → 明确解析入口，逐条验证已有失败参数和无需仓库的帮助/版本。
- [路径和文件系统 API 的语义差异] → 在 Linux/macOS 覆盖断链、目录链接、权限备份和同目录 rename；故障注入核对备份先于替换。
- [指纹漂移或未知 metadata 丢失] → 完整清单转换、固定编码向量、依赖锁定；v2/v3 明示不可直接比较。
- [依赖数量及 Rust 编译成本可能上升] → 单包、精简 feature，记录直接和传递依赖，不以运行时轻量推断构建轻量。
- [体积/启动收益未知] → 比较同平台 Go 默认及 `-trimpath -ldflags='-s -w'` 产物与 Rust release，记录工具链、架构、大小、重复运行分布与冷/热缓存条件，不设置虚构的改善门槛。
- [未跟踪 Go 代码删除后无法 Git 回退] → 实施前在 work/ 保存仅源码、依赖、构建文件的可恢复基线副本与文件哈希，不复制用户配置或私密运行记录；保留直到验收完成。

## Migration Plan

1. 在实施授权后记录工作区基线和资产文件哈希，保存可恢复副本；Go 仅构建黑盒对照产物，不研究或运行其内部测试作为 Rust 设计前置条件，产物放在忽略目录 work/，不提交。依据三份能力规范建立功能覆盖矩阵。
2. 独立完成 Rust 类型、模块、实现、固定向量和场景测试，使用复制资产验证矩阵覆盖。过渡阶段 Go 仅供黑盒功能/性能对照与回退保存，最终交付不维护双实现。
3. Rust 验收通过后替换 Makefile/CI/文档/工程约定，移除仅属于 Go CLI 的源码、模块文件和过时产物；不重置用户其他改动。
4. 运行 Rust 检查、release 场景验证和 git diff --check；记录本机验证及 CI 未执行的平台缺口，产物测量保存至 .harness/ 或 work/，均不提交。
5. 若未通过验收，从保存副本逐文件恢复本次替换的 Go CLI 内容，保留用户其他改动；不以全目录 reset 回退。不自动替换真实用户安装或发布工具。

## Open Questions

无阻塞设计决策。具体依赖解析版本与性能数据由实施时工具链兼容验证和测量确定；这些不改变已定义契约和任务范围。
