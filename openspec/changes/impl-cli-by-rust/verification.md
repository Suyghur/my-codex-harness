# impl-cli-by-rust 验收记录

日期：2026-10-05。工作分支：feat/cli。平台：macOS / x86_64。工具链：rustc 1.95.0 (59807616e 2026-04-14)，cargo 1.95.0 (f2d3ce0bd 2026-03-21)。

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
