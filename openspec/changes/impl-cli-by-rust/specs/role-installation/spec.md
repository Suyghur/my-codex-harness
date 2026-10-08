# Spec Delta

## Purpose

定义 macOS 与 Linux 上将登记角色链接到用户配置目录的安全行为，覆盖安装前检查、原文件保护、符号链接备份、重复执行与预览，避免语言迁移造成用户配置损失或误报事务保证。

## ADDED Requirements

### Requirement: 安装预检与平台范围
install SHALL 仅支持 macOS/Linux，并在创建安装目录或替换角色前校验全部资产、目标目录祖先、全部目标文件类型及必要备份目录。目标只允许普通文件或符号链接；已有目标目录或不可用目录祖先 MUST 导致失败且不产生安装写入。

install MUST 在只读预检中拒绝忽略 ASCII 大小写后重复的目标名称，采用一致的可移植策略，不依赖目标卷是否区分大小写。实际写入目录 SHALL 解析现有祖先的物理路径，固定解析后的路径用于执行；安装或备份写入位于物理源 agents 内，以及目标替换会覆盖资产清单时 MUST 被拒绝。目标已经全部正确且无需任何写入时 SHALL 保留幂等跳过。

`check --codex-home` SHALL 与 install 共用物理目录解析规则；包含不存在路径段与 `..` 的参数在安装和检查之间 MUST 保持一致，且不创建被规整掉的目录。

#### Scenario: 安装与检查的目录解析一致
- **WHEN** 使用相同的 missing/../home 目标参数执行 dry-run、install 和 check
- **THEN** 正式安装后 check 通过，预览和正式安装均不创建 missing 目录

#### Scenario: 缺失段之后的目录链接与不可用祖先
- **WHEN** 目标路径包含 missing/../alias/../home，alias 指向既有 remote/sub 目录
- **THEN** 预览、安装和检查使用 remote/home，既有链接后的父目录保持物理语义，missing 不被创建
- **AND** 若该位置是普通文件或断链，则预览与安装均失败且不产生写入

#### Scenario: 大小写目标碰撞
- **WHEN** Alpha 与 alpha 分别来自不同合法源目录，用户执行 install 或 dry-run
- **THEN** 在任何支持平台均返回失败且不写入目标或源资产，即使目标目录链接到其中一个源目录

#### Scenario: 子目录源角色与安装目录别名
- **WHEN** 角色登记 agents/nested/worker.toml，目标 agents 链接到源 agents
- **THEN** install 与 dry-run 均拒绝向源目录创建新目标，源文件集合和内容不变

#### Scenario: 备份或清单写入重叠
- **WHEN** 必需备份目录解析到源 agents 内，或某角色目标替换会覆盖 harness.toml
- **THEN** 安装预检失败，不创建安装目录、备份或角色链接，不修改原清单与冲突目标

#### Scenario: 后续角色存在冲突目录
- **WHEN** 一个登记角色目标是目录，其他目标均可安装
- **THEN** 整次预检失败，其他角色不被提前安装

#### Scenario: 不可用备份路径
- **WHEN** 安装需要备份但备份祖先是普通文件或断链
- **THEN** 安装失败，原角色文件和安装目录内容保持不变

### Requirement: 冲突全部先备份
install SHALL 在任何角色替换前，将所有冲突普通文件或符号链接备份到目标配置目录的 `backups/my-codex-harness/agents-*` 唯一目录。普通文件 SHALL 保留内容与权限位；链接 SHALL 保留原始链接字面量，包括相对链接和断链。任何备份失败 MUST 阻止全部角色替换，已产生的备份 SHALL 保留供恢复。

#### Scenario: 相对断链备份
- **WHEN** 某角色目标是指向 `../missing.toml` 的链接
- **THEN** 备份仍为相同字面量的符号链接，不读取或创建链接目标

#### Scenario: 备份中途失败
- **WHEN** 一个冲突已备份而另一个冲突备份失败
- **THEN** 所有角色目标均未替换，错误报告失败，并保留已完成的备份

### Requirement: 单链接原子替换
install SHALL 通过同一文件系统内的临时链接原子替换每个目标；新链接 SHALL 指向经解析的角色源文件。多文件安装 MUST 不宣称具有事务或自动回滚保证。中途替换失败 SHALL 返回失败并保留冲突备份，已完成链接可在下一次运行中检查和继续。

#### Scenario: 替换中断
- **WHEN** 部分链接已完成而下一次替换失败
- **THEN** 返回操作失败，已安装链接与备份可检查，未完成目标不被先删除

#### Scenario: 结果输出失败
- **WHEN** 文件系统安装成功，但结果输出管道不可写
- **THEN** 程序返回操作失败，输出错误不提前中断角色安装；文件系统失败时仍保留并尝试报告备份路径和已完成动作

### Requirement: 幂等与用户配置保护
install SHALL 跳过已解析到正确源文件的目标，不创建无必要备份，仅处理清单登记角色，不修改 config.toml、清单外角色或源资产。`check --codex-home` SHALL 检查每个登记目标是否解析到正确源文件并报告缺失或错误目标。

#### Scenario: 重复安装与目录链接
- **WHEN** 安装已成功后再次执行，或目标 agents 目录已经链接到仓库 agents
- **THEN** 正确目标被跳过，不额外备份，不把源文件替换成链接

#### Scenario: 保留用户自定义配置
- **WHEN** 目标包含自定义角色和 config.toml
- **THEN** 安装登记角色后，这些用户文件内容不变

#### Scenario: 缺失目标检查
- **WHEN** 指定配置目录缺少登记角色或目标指向其他来源
- **THEN** check 报告受影响角色并返回操作失败

### Requirement: 零写入预览
install --dry-run SHALL 执行与正式安装相同的只读预检并输出拟安装链接，但 MUST 不创建目录、备份、临时文件或链接，不替换现有文件。

#### Scenario: 新目录预览
- **WHEN** 目标配置目录不存在，用户执行有效 dry-run
- **THEN** 输出预览后退出成功，目标目录仍不存在

#### Scenario: 有冲突预览
- **WHEN** 目标含冲突普通文件和链接，用户执行 dry-run
- **THEN** 输出预览，所有原文件和链接保持不变，备份目录未创建
