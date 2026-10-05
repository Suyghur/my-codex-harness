# Proposal

## Why

当前 CLI 尚未公开发布，也没有外部兼容要求，适合按现有功能契约重新设计 Rust CLI，实现统一的 Rust 构建与分发。Go 源码不作为架构或算法模板；资产管理和安装功能必须完整覆盖。代码量、产物体积及启动耗时的改善须通过同功能实测确认，不作为未经验证的收益承诺。

## What Changes

- 以 Rust 完整替换 Go CLI，继续提供 `check`、`install`、`version`、`--root` 和现有路径优先级、退出码及输出流契约。
- 从本 change 的规范、资产格式和用户可观察行为独立设计 Rust 类型、模块和错误处理，不逐文件翻译 Go，不沿用 Go 内部 API 或测试组织；Go 可执行产物仅作黑盒功能和性能对照，不能替代规范验收。
- 保留角色 TOML 外置、资产校验、场景结构检查、安装预检、冲突全部先备份、单链接原子替换与 dry-run 零写入。
- **BREAKING**：bundle 指纹算法从 `sha256-json-v2` 升级为 `sha256-json-v3`，采用明确定义的确定性 JSON 编码；不复现旧 Go 编码。源文件和提示词 SHA-256 保持原语义。
- **BREAKING**：统一可执行文件名为 `my-codex-harness`，发布构建产物为 `target/release/my-codex-harness`；版本 JSON 移除 `go_version`，提供 `rust_version`，继续区分 CLI 与资产版本。
- 替换 Go 构建、依赖、CI 和工程文档，补齐真实二进制集成测试；验收后仅保留 Rust 实现。
- 对 Go 与 Rust 的同平台 release 产物进行体积和启动耗时对照，记录构建配置与局限。
- 不新增模型调用、调度、沙箱、审批、skills 安装或 Windows 安装支持；不更改角色内容和模型设置。

## Capabilities

### New Capabilities

当前主规范为空，首次将现有行为和本次必要契约调整纳入以下能力，不表示全部功能均为新实现：

- `cli-interface`：命令、路径参数、版本、JSON、退出码及单一 Rust 构建交付。
- `role-assets`：角色与清单校验、资产摘要、确定性版本指纹和场景结构校验。
- `role-installation`：macOS/Linux 的角色链接安装、预检、备份、幂等和零写入预览。

### Modified Capabilities

无已有主规范需要修改。

## Impact

- 替换 `main.go`、`internal/cli/`、`internal/harness/`、`go.mod`、`go.sum`；新增 `Cargo.toml`、`Cargo.lock`、Rust 工具链声明、`src/` 与 `tests/`。
- 更新 `Makefile`、`.github/workflows/check.yml`、`.gitignore`、`README.md`、`AGENTS.md` 和 `docs/architecture.md` 中的语言、构建、测试、版本及指纹约定。
- 运行依赖拟采用 clap、serde、toml、serde_json、sha2、anyhow、tempfile；测试按需采用 assert_cmd、predicates。不引入异步运行时或多源配置框架。
- 保留用户工作区已有改动、角色资产与清单内容；测试和测量仅使用隔离配置目录，不改真实 Codex 配置。
