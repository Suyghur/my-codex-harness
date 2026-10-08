# impl-cli-by-rust 验收记录

日期：2026-10-05。工作分支：feat/cli。平台：macOS / x86_64。工具链：rustc 1.95.0 (59807616e 2026-04-14)，cargo 1.95.0 (f2d3ce0bd 2026-03-21)。

## 2026-10-08 第二轮审查修复与优化验收

- 修复缺失段规整后未重新检查既有组件的问题：目录链接后的父目录按物理语义处理，普通文件和断链祖先在预览及安装前拒绝；安装和 check 使用同一解析规则。
- 修复直属 TOML 后缀目录误报，读取清单和场景前拒绝 FIFO 等非普通文件；真实进程超时保护验证异常输入及时返回失败 JSON。
- 清单仅解析一次，再从完整树提取已知字段并验证类型；场景角色、模式及重复 ID 使用借用索引。未知 metadata 保持参与摘要。
- v3 编码直接输出至 SHA-256，避免完整 JSON 树重建及编码缓冲；原向量及实际资产指纹未变，补充负零、有限浮点极值、整数边界、小数秒日期时间和角色顺序向量。
- 安装领域返回类型化事件，命令层在执行结束后输出。备份创建即记录恢复位置；故障注入验证失败后的事件和文件状态。真实 release 关闭输出管道后返回 1，但全部登记角色安装完成。
- make check、make test、make build、release check --json --scenarios、OpenSpec strict 和 git diff --check 通过：34 个单元测试、22 个 macOS 真实进程测试；12 次 release 隔离回归通过。新增故障事件断言曾因 /var 与 /private/var 路径别名比较失败，改为比较物理路径后全部通过。
- 独立复审重新复现路径场景通过；143 组原反序列化与新清单转换对照、34 组原树编码与流式摘要对照全部一致。临时对照程序在 /tmp/harness-fingerprint-cross-review/，不作为维护实现。
- 7 份角色、清单与场景哈希和本轮修改前一致，实际 bundle 指纹保持 ab5d77c85d6578eefad0167f3806804e0f9f1cf207bfa3e27e173d6b5af1f1ea。未改角色及模型配置、Cargo 依赖或 v3 算法标识。

本轮同平台修复前后 release 对照，3 次预热、交替执行，version/check 各 31 样本，压力输入各 11 样本；缓存未清空，计时包含子进程和输出捕获。RSS 为 macOS time -l 的单次样本，不代表内存分布，不能据此声称所有输入内存均下降。

| 场景 | 修复前中位 ms | 修复后中位 ms | 修复前 RSS B | 修复后 RSS B |
| --- | ---: | ---: | ---: | ---: |
| version --json | 8.460 | 8.499 | 913,408 | 888,832 |
| 当前资产 check --json | 12.059 | 11.794 | 1,191,936 | 1,314,816 |
| 20,000 个 metadata 字段（清单 889,606 B） | 133.161 | 66.743 | 26,730,496 | 14,819,328 |
| 20,000 个场景（JSON 2,388,952 B） | 65.093 | 60.809 | 12,361,728 | 14,708,736 |

release 由 909,704 B 降至 893,352 B。当前小资产耗时未见明显变化；大 metadata 场景耗时约减半，单次 RSS 样本降低约 45%；场景检查耗时略降但本次 RSS 样本更高，不宣称全局性能或内存收益。所有对照输入的修复前后指纹一致。release 增量构建约 36.53s，未做冷构建对照。

可重跑测量脚本及样本：work/impl-cli-by-rust/review-round2/benchmark.py、benchmark.json；make-test.log、release-check.json、release-regressions.json 和 verification.json 保存本轮记录。RSS 工具在沙箱内被 macOS sysctl 权限阻止，经工具批准后在沙箱外执行隔离基准。work/ 被忽略，不提交配置、日志或二进制。

本轮仅本机 macOS/x86_64 验证；Linux 和远端 CI 未执行，模型行为仍 not_run。未安装到真实用户配置、未发布、未提交或归档。

## 2026-10-06 深度审查修复验收

本次修复原审查的 2 个 P1、2 个 P2，以及独立复审发现的路径解析一致性 P2。初次 22/22 和历史测试结果未覆盖这些边界，下文基准属于初次实现记录；本节为当前修复版本证据。

- 源保护：预检拒绝向物理源 agents 安装/备份、替换清单；子目录源结合目标目录别名保持零写入，全部正确的目标仍幂等跳过。物理目录投影固定执行路径，避免缺失路径段加 `..` 在源目录创建无用目录。
- 目标碰撞：名称只允许非空 ASCII 字母、数字、下划线和连字符；安装在所有平台采用忽略 ASCII 大小写后的键拒绝碰撞，保持预检只读。现有角色全部符合规则。
- 登记身份：合法仓库内源链接别名按物理来源及角色名识别，不同名称的额外直属 TOML 链接仍被拒绝。
- 名称规则：资产和安装共用验证；点名、分隔符及非 ASCII 名称在资产加载时一致拒绝。
- 复审补充：安装与 check 链接检查共用物理目录解析，相同 missing/../home 参数完成 dry-run→install→check，且 missing 未创建。补上自动发现失败时 `--root` 引导诊断断言。
- make check/test/build 通过；29 个单元测试与 17 个 macOS 真实进程测试通过。格式、clippy、当前种子结构校验及 diff 检查通过，模型行为仍 not_run。
- 最新 release 执行 13 次隔离回归调用及当前资产/场景 JSON 检查通过；隔离 cargo install 和 version --json 通过。release 大小 909,704 字节；未重测启动性能，初次基准不代表最新构建的测量结果。
- 独立复审确认原 P1 已覆盖，发现的新增 P2 修复后使用原复现参数检查返回 0、errors=[]，聚焦复审无新增实质问题。
- 14 份角色/清单/场景/skill/模板哈希与基线一致；Cargo 依赖和编码固定向量未变。未触碰真实用户配置，未发布，改动未提交。Linux/远端 CI 未执行。

新增场景对应测试：source_directory_alias_with_nested_roles_is_rejected_without_writes、case_colliding_installation_names_are_rejected_on_every_platform、physical_source_alias_registration_can_be_checked_and_installed、invalid_role_names_are_rejected_consistently_before_installation、backup_directory_alias_and_manifest_overlap_are_rejected_before_writes、installation_and_check_share_missing_parent_path_resolution、missing_parent_traversal_does_not_create_discarded_source_directories，以及 shared::assets/shared::role_name 新增单元测试。

本次记录在 work/impl-cli-by-rust/review-fixes/ 的 build-checks.json、release-regressions.json、verification.json；work/ 被忽略，不提交隔离配置、产物及日志。

## 交付

- 单一 Rust package，二进制名 my-codex-harness，release 产物 target/release/my-codex-harness。
- 按 check/install/version 子命令领域组织；共用资产、指纹与路径位于 src/shared。独立依据功能规范设计，未研究或翻译旧 Go 内部实现、未移植旧测试。
- 完成清单/角色/场景结构检查、v3 指纹、帮助和版本、目录发现、只读预览、全部冲突先备份、单链接原子替换、幂等安装与链接检查。
- 移除旧 Go 源码、模块文件和确认属于旧 CLI 的根目录构建产物。保留角色、清单、场景、skill 和模板；14 份资产哈希与基线一致。
- README、AGENTS.md、架构及评估文档、Makefile、Linux/macOS CI 已切换至 Rust。工作区已有改动保留，未提交或推送。

## 已执行验证

- make check：rustfmt、clippy --all-targets --locked -D warnings、24 个单元测试和 11 个真实进程测试、当前资产及种子场景结构检查、git diff --check 全部通过。
- make test、make build、release check --json --scenarios 通过。check 返回 passed、errors=[]、behavior_evaluation=not_run。
- release 二进制在仓库外执行 11 次帮助/版本/显式来源检查/JSON/预览/安装/重复安装调用，结果保存于 work/impl-cli-by-rust/release-scenarios.json。
- cargo install --path . --locked --root work/impl-cli-by-rust/cargo-install 成功，隔离安装的 version --json 验证通过。
- 三份能力规范的 27 个场景已逐项关联真实测试或构建/进程验证：work/impl-cli-by-rust/coverage.json。
- 安装备份/替换中断通过私有故障注入验证；不承诺多文件事务。文件权限及绝对/相对断链字面量被保留，失败后备份仍可检查，重跑可继续。
- 两轮独立静态复核无 P1/P2 问题；OpenSpec strict 校验通过。静态复核不替代以上运行结果。

## 大小与启动测量

同一复制资产，同一平台，3 次预热及每项 31 个样本，轮换二进制执行顺序。计时包含 subprocess 启动和输出管道开销；文件缓存未清空，不代表冷启动。

| 产物 | 操作 | 字节数 | 中位耗时 ms |
| --- | --- | ---: | ---: |
| go-default | version | 5,991,504 | 10.985 |
| go-stripped | version | 4,030,512 | 10.818 |
| rust-release | version | 897,392 | 8.260 |
| go-default | check | 5,991,504 | 12.430 |
| go-stripped | check | 4,030,512 | 12.312 |
| rust-release | check | 897,392 | 10.575 |

Go 基线为 go version go1.27.1 darwin/amd64：默认构建约 5.02s，-trimpath -ldflags='-s -w' 约 3.89s。Rust release 使用 opt-level=s、strip、LTO、codegen-units=1，完整 release 编译约 38.64s，无变更构建约 0.24s。依赖已下载，dev/test 已构建，各构建缓存不同，构建时间仅作成本记录，不构成语言编译性能的受控比较。原始样本及构建日志位于 work/impl-cli-by-rust/benchmark.json、go-builds.json、rust-build.json。

## 限制与恢复

- 本机验证只覆盖 macOS/x86_64；远端 Linux/macOS CI 未运行。非 UTF-8 文件名进程测试限定 Linux：macOS 文件系统在准备目录时拒绝该字节序列；Linux 用例尚未在本机执行。
- 未运行模型，场景结构通过不表示角色行为或模型可用性通过；未安装到真实用户 Codex 配置，未发布。
- v3 不是 RFC 8785；依赖固定在 Cargo.lock，调整编码需验证固定字节/摘要向量，编码改变须升级算法标识。v3 与旧 v2 bundle 摘要不直接比较。
- 全量预检并非对外部并发文件变更的锁定；单个链接原子替换，多文件安装可部分完成。备份失败不会提前替换目标。
- 旧源码/依赖/构建与原文档副本在 work/impl-cli-by-rust/baseline，原 Go 二进制在 original-go-binary；baseline.json 保存原始哈希。恢复前先核对后续用户改动，只复制需要恢复的文件，避免覆盖其他内容。
- 复核文件清单与资产哈希记录：work/impl-cli-by-rust/final-audit.json。work/ 整体被忽略，不提交基线、测试配置或二进制。
