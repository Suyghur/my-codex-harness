# Spec Delta

## Purpose

定义本地角色资产 CLI 的用户接口，明确命令、路径选择、输出格式、退出状态及构建交付，使语言迁移后的工具能够被用户可靠调用，同时显式区分程序版本与资产版本。

## ADDED Requirements

### Requirement: 命令与参数边界
CLI SHALL 提供 `check`、`install`、`version` 和根级 `--version`；`--root` SHALL 支持位于子命令前后。无参数 SHALL 显示帮助并成功退出。未知命令、未知参数、多余位置参数、显式空路径和根级 `--version` 与子命令组合 MUST 作为参数错误拒绝。

#### Scenario: 合法调用和帮助
- **WHEN** 用户执行无参数、帮助、版本命令，或分别将 `--root` 放在 `check` 前后
- **THEN** 帮助和版本无需资产仓库即可返回成功，两个合法 check 调用检查同一指定仓库

#### Scenario: 不合法参数
- **WHEN** 用户执行 `check --root=`、`install --codex-home=`、`version extra` 或 `--version check`
- **THEN** 程序返回退出码 2，诊断写入 stderr，且不执行资产安装

### Requirement: 资产根目录与目标配置目录
CLI SHALL 优先使用显式 `--root`，否则从当前目录向上发现常规文件 `harness.toml`。安装目标 SHALL 按显式 `--codex-home`、非空 `CODEX_HOME`、`~/.codex` 选择。路径 SHALL 支持 `~`、`~/` 和相对路径，拒绝 `~user`。目标目录尚不存在 MUST 不因参数解析而失败。

#### Scenario: 仓库外运行
- **WHEN** 程序在无清单目录运行，用户传入有效 `--root`
- **THEN** 程序使用指定资产根目录，角色内容不依赖编译时目录

#### Scenario: 安装路径优先级与不存在的目录
- **WHEN** 显式目标与 CODEX_HOME 均存在于参数或环境中，显式目标目录尚未创建
- **THEN** install 使用显式目标；dry-run 可以成功预览且不创建任何目标目录

#### Scenario: 自动发现失败
- **WHEN** check 未指定 root，当前目录及所有祖先均无清单
- **THEN** 程序返回退出码 1并提示用 `--root` 指定来源

### Requirement: 退出状态和输出流
CLI SHALL 使用退出码 0 表示成功、1 表示资产或安装操作失败、2 表示命令或参数错误。结果 SHALL 写 stdout，诊断 SHALL 写 stderr。`check --json` 的操作失败 MUST 仍输出可解析结果，包含 `status=failed`、非空 `errors` 和 `behavior_evaluation=not_run`；缺少 bundle 时 SHALL 省略 bundle 字段。

#### Scenario: 结构化失败
- **WHEN** 用户对无有效清单的目录执行 `check --json`
- **THEN** stdout 仅包含失败 JSON，errors 为数组，stderr 不重复输出检查错误，退出码为 1

#### Scenario: 成功 JSON 与文本诊断
- **WHEN** 用户分别执行成功的 `check --json` 与文本 check
- **THEN** JSON 检查返回 `status=passed`、`errors=[]` 和 bundle，stderr 为空；文本检查的行为评估说明写 stderr，结果写 stdout

### Requirement: 程序版本元数据
CLI SHALL 将程序版本与清单资产版本分别输出。`version --json` SHALL 提供 `version`、`os`、`arch`、`rust_version` 字符串，不提供 `go_version`；rust_version MUST 描述构建该二进制的 Rust 编译器，而非运行环境。版本命令 SHALL 不要求资产仓库存在，但仍拒绝显式空 root。

#### Scenario: 独立版本查询
- **WHEN** 用户在仓库外执行 `version --json`
- **THEN** 得到程序版本及构建平台、编译器信息，JSON 中不存在 go_version，退出码为 0

### Requirement: 单一可执行交付
项目 SHALL 以 Rust 构建唯一维护的 CLI 实现，二进制名称为 `my-codex-harness`，标准 release 构建产物为 `target/release/my-codex-harness`。构建及验证入口 MUST 与使用文档一致，Linux/macOS 的验证 SHALL 覆盖真实二进制与隔离安装场景。

#### Scenario: 按文档构建运行
- **WHEN** 用户按文档执行 `make build` 并运行 `target/release/my-codex-harness --help`
- **THEN** 可以运行构建结果，不要求 Go 工具链，仓库不保留并行维护的 Go CLI 源码
