# Spec Delta

## Purpose

定义角色资产、完整清单和场景目录的校验及摘要契约，确保外置文件的真实内容可被识别和比较，并将结构校验结果与模型可用性、权限隔离和角色行为评估明确区分。

## ADDED Requirements

### Requirement: 清单和角色有效性
资产检查 SHALL 校验 schema_version=1、非空名称和版本、至少一个角色、角色名与文件名及 TOML name 一致、非空描述和提示词、至少一个非空白且不重复的 mode、已支持的 access；模式名 MUST 不受固定名称列表限制。可选模型和推理设置 MUST 为非空字符串。登记角色源路径 SHALL 唯一，agents 直属 TOML 文件 SHALL 全部登记。

#### Scenario: 非法角色和清单
- **WHEN** 清单 schema 不受支持、模式重复、access 无效、模型为数字或空白、角色名称不匹配，或存在未登记 TOML
- **THEN** 资产检查失败并指出受影响的资产，不返回通过结果

#### Scenario: 用户定义新模式
- **WHEN** 角色声明新的非空白模式名且无重复，其他字段有效
- **THEN** 资产检查接受该模式，无需修改 CLI 内置模式列表

### Requirement: 角色源文件边界
角色路径 SHALL 为仓库相对路径，并在解析符号链接后位于物理 `agents/` 目录内且具有 `.toml` 扩展名。绝对路径、上级逃逸和指向外部的源链接 MUST 被拒绝。

#### Scenario: 源链接逃逸
- **WHEN** 登记路径位于 agents 内但链接实际指向该物理目录外
- **THEN** 资产检查失败，install 不修改目标配置

### Requirement: 源文件和提示词摘要
每个角色 SHALL 提供完整源文件原始字节的 SHA-256 和解析后的 developer_instructions UTF-8 字节的 SHA-256，使用小写十六进制；模型与推理设置缺失时摘要对象 SHALL 保留对应 JSON null。

#### Scenario: 文件格式变化
- **WHEN** 角色文件仅增加末尾空行且解析后提示词不变
- **THEN** 源文件摘要变化，提示词摘要不变，bundle 指纹变化

### Requirement: 版本化确定性资产指纹
bundle SHALL 使用 `fingerprint_algorithm=sha256-json-v3`，对包含算法标识、完整解析清单和按名称排序角色摘要的确定性编码计算 SHA-256。指纹 MUST 与 checkout 路径无关，包含清单未知元数据，并在相关内容变化时变化；同一算法内无序对象键的声明顺序 MUST 不影响指纹。旧 v2 与 v3 指纹 SHALL 不作为同算法直接比较。

#### Scenario: 路径与键顺序无关
- **WHEN** 相同资产复制到另一绝对路径，或仅调整清单表键声明顺序
- **THEN** 两者具有相同 v3 bundle 指纹

#### Scenario: 完整清单参与摘要
- **WHEN** 清单未知 metadata 字段改变但版本与角色不变
- **THEN** v3 指纹变化，清单 schema_version 仍为 1

### Requirement: 场景结构校验的证据边界
`check --scenarios` SHALL 校验场景 schema_version=1、status=seed_not_run、至少一个场景、唯一非空 ID、已登记角色及有效模式、非空 setup/task/criteria。校验 MUST 不调用模型或执行角色，check 的 behavior_evaluation SHALL 始终为 not_run。

#### Scenario: 无效路由和空判据
- **WHEN** 场景使用未知角色或模式、重复 ID 或空白判据
- **THEN** 检查失败，且未运行模型

#### Scenario: 结构通过
- **WHEN** 当前种子场景结构与角色路由有效
- **THEN** 检查可以通过，JSON 仍标明 behavior_evaluation=not_run
