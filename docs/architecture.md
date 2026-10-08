# 工程边界与设计依据

本仓库是Rust CLI 工程：角色定义、资产清单、校验/部署命令、协作模板和回归场景。Codex 负责模型会话、工具调用与真实权限控制，目标项目负责业务代码、OpenSpec 规范和验收环境。

CLI 使用 clap 类型化子命令、serde/toml 读取资产、sha2 计算摘要，同步文件操作满足当前规模。`src/main.rs` 处理进程退出，`src/cli.rs` 只处理根参数与分发；`src/commands/check/`、`install/`、`version/` 各自维护参数与用例，安装领域行为位于 install/domain.rs。多个命令使用的资产、指纹和路径策略位于 `src/shared/`，核心不依赖 clap。

实现按 Rust 类型与所有权独立设计，不映射旧 Go 内部 API 或测试。领域返回结果，命令边界处理 JSON/text 与退出码，stdout 承载结果，stderr 承载诊断。测试按能力规范设计，真实子进程独立设置 HOME/CODEX_HOME/CWD，避免并行测试修改全局状态。支持 macOS/Linux 链接安装。

Cargo.lock 固定依赖，Rust 1.95.0 工具链在 rust-toolchain.toml 和 CI 中一致声明。make check 执行格式、clippy、测试、资产场景和 diff 检查，make build 使用 release/locked 生成 target/release/my-codex-harness。release 采用体积优化、strip、LTO 和单 codegen unit，保留 unwind；收益由同平台测量确认。

## 如何分工

主智能体选择问题与模式，维护唯一权威任务状态，协调文件责任并核查最终结果。角色自行完成职责内的常规调查，独立工作有收益才委派。交接使用稳定的版本和产物引用，不复制全部历史或完整日志。多个角色产出冲突时由主智能体结合证据解决。

OpenSpec 的主规范、目标 change 和 tasks 是契约与进度来源；本工程的模板只帮助交接，不另建调度器。角色输出是候选材料，经过与验收标准关联的核查后才成为完成依据。等待输入、等待外部事件、验证缺失及行为失败需分别描述，不把停止输出当作完成。

角色的只读、QA 和文件边界是提示词约定。`harness.toml` 的 access 仅用于资产发现与审查，不是 sandbox 策略，不限制工具，也不替代真实身份、权限和用户授权。涉及副作用时依赖当前 Codex 会话及目标系统的实际控制。

## 可复现与部署

清单声明角色、模式和版本；校验器返回每份完整配置及提示词的 SHA-256，bundle 指纹同时包含清单。版本号相同但工作区修改后指纹仍变化，便于比较实际部署内容。校验器只做结构及可选链接检查，不能判断账号是否支持模型或角色执行质量。

Rust CLI 的 bundle 指纹使用 `sha256-json-v3`，与旧算法值不直接比较；清单 schema_version 仍为 1，单文件原始字节和解析后提示词 UTF-8 字节仍以 SHA-256 摘要。CLI 版本由 Cargo 包版本提供，与资产版本独立；version --json 输出 version/os/arch/rust_version，编译器信息在构建时捕获。

清单只解析一次，已知字段从完整 TOML 树借用提取并验证，未知 metadata 继续参与指纹。指纹使用借用序列化器将确定性 JSON 直接写入 SHA-256，避免完整 JSON 树重建与编码缓冲；排序仅收集键和角色引用。固定向量同时约束字节与摘要，覆盖负零、有限浮点极值、整数边界及小数秒日期时间。场景检查使用借用角色/模式索引。

v3 编码规则：

- 输入对象包含 algorithm、完整解析清单 manifest 和按 name 排序的角色摘要 roles，不含指纹自身或物理路径。模型设置缺失保留 null，未知清单 metadata 参与摘要。
- 递归对象键按 UTF-8 字节序排序，数组保持声明顺序；TOML 字符串、布尔、整数和有限浮点转为对应 JSON 类型。非有限浮点返回错误；日期时间转为规范 TOML 字符串，日期时间分隔符为 T。
- 锁定版本的 serde_json 输出紧凑 UTF-8，无缩进、BOM、尾换行或额外 HTML 转义，不进行 Unicode 归一化。所得字节 SHA-256 以小写十六进制输出；不声称符合 RFC 8785。
- 固定编码/摘要向量覆盖 Unicode/HTML、日期时间、数值、null、嵌套键。升级编码依赖须重跑向量，若结果变化必须升级算法标识。

评估记录必须保存算法标识，不能只用版本号判断资产相同。

链接模式让本地 Codex 直接读取仓库当前定义，适合个人开发，但不是不可变发布：编辑立即改变下次读取的内容。需要稳定实验时先使用固定 commit 的独立 checkout，再链接该 checkout；不将“已安装”误称为“已发布”或“行为已验证”。安装前预检，冲突先备份，逐个链接原子替换；多文件安装不是事务，运行中断时检查实际链接再续行。

角色名称使用统一的非空 ASCII 字母/数字/下划线/连字符规则；安装目标按 ASCII 忽略大小写去重，所有平台保守拒绝碰撞。直属文件登记匹配物理来源和角色名，支持合法仓库内源路径别名。安装与链接检查共用物理目录解析，预检固定物理写入目录，拒绝安装/备份写入源 agents 或替换清单；完全无需写入的正确目录别名可幂等跳过。

缺失目录段允许被父目录规整，但后续遇到既有组件时重新检查类型并解析目录链接，保持链接后父目录的物理语义，拒绝普通文件及断链祖先。直属 TOML 后缀目录不作为角色文件；清单与场景输入须为普通文件，避免 FIFO 读取阻塞。

安装领域收集类型化事件，命令层在文件系统执行结束后格式化结果。备份目录创建即记录恢复事件，失败时仍尝试输出已完成动作；输出失败不提前中断安装，文件系统错误优先报告。

二进制不嵌入角色资产。默认从当前工作目录向上发现 `harness.toml`，仓库外用 `--root` 指定 checkout；不依赖编译时的绝对路径。备份中的符号链接保留原始字面量，恢复时应放回原目标路径，以保持相对链接语义。

## CLI 功能验收

实现以 OpenSpec 的 cli-interface、role-assets、role-installation 契约为依据，按用户场景验证命令、资产与安装；旧可执行文件仅提供黑盒和性能对照，不作为 Rust 内部设计模板。安装全量预检后全部备份，再逐个原子替换；临时链接与目标同文件系统。备份失败不提前替换角色，替换中途失败保留备份及已完成链接；不承诺多文件事务。测试使用私有故障注入验证失败顺序，不提供用户故障开关。

## 参考与适用取舍

本轮参考 aliyun/ai-agent-handbook，固定版本为 `6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8`：

- [第 4 章：任务与受控委派](https://github.com/aliyun/ai-agent-handbook/blob/6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8/02-build/%E7%AC%AC%204%20%E7%AB%A0%20%E4%BB%BB%E5%8A%A1%EF%BC%9A%E7%BC%96%E6%8E%92%E3%80%81%E9%95%BF%E7%A8%8B%E6%8E%A8%E8%BF%9B%E4%B8%8E%E5%8D%8F%E4%BD%9C%E6%B5%81%E8%BD%AC.md)：任务完成需要环境证据，规划、委派与执行可以按复杂度选择。
- [第 5 章：上下文与状态](https://github.com/aliyun/ai-agent-handbook/blob/6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8/02-build/%E7%AC%AC%205%20%E7%AB%A0%20%E4%BF%A1%E6%81%AF%EF%BC%9A%E4%B8%8A%E4%B8%8B%E6%96%87%E3%80%81%E7%8A%B6%E6%80%81%E4%B8%8E%E5%8F%AF%E5%A4%8D%E7%94%A8%E8%83%BD%E5%8A%9B%E8%B5%84%E4%BA%A7.md)：最小充分上下文、证据卸载与可续接记录。
- [第 11 章：协作与成果接受](https://github.com/aliyun/ai-agent-handbook/blob/6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8/03-run/%E7%AC%AC%2011%E7%AB%A0%20%20Multi-Agent%20%E5%8D%8F%E4%BD%9C%E4%B8%8E%E7%BC%96%E6%8E%92.md)：责任、版本、结果接受和根任务完成分别管理。
- [第 15 章：资产管理](https://github.com/aliyun/ai-agent-handbook/blob/6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8/04-governance/%E7%AC%AC%2015%20%E7%AB%A0%E3%80%80AI%20%E8%B5%84%E4%BA%A7%E7%9A%84%E5%8F%91%E7%8E%B0%E4%B8%8E%E7%AE%A1%E7%90%86.md)：版本化清单与有效输入的可复现性。
- [第 22 章：Badcase](https://github.com/aliyun/ai-agent-handbook/blob/6d12dd2dc006eefd0f89f213c4e0ca2edfe7e9a8/05-optimization/%E7%AC%AC%2022%20%E7%AB%A0%E3%80%80Agent%20%E4%BC%98%E5%8C%96%EF%BC%9ABadcase.md)：真实样本、明确判据与同条件的对比回归。

以上转化为本工程的本地机制，未引入书中的云平台、服务、运行时或固定审批流程，也未复制上游章节内容。
