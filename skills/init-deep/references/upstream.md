# 上游来源与适配

- 来源：[oh-my-openagent / init-deep](https://github.com/code-yeongyu/oh-my-openagent/blob/a8019016f47a9d814ebcc24bd921a561f065ee80/packages/shared-skills/skills/init-deep/SKILL.md)。
- 固定版本：`a8019016f47a9d814ebcc24bd921a561f065ee80`，2026-10-05 获取 `dev` HEAD 并读取该 commit 的源文件。
- 本目录为修改版，沿用分层 AGENTS.md、update/create-new、max-depth、目录评分、先根后子与去重核验的工作流。
- 上游的 Sustainable Use License 随目录保存在 [LICENSE.md](../LICENSE.md)，复制或分发本目录时一并保留。

## Codex 适配

| 上游行为 | 本版本 |
| --- | --- |
| OpenCode 的 task / TodoWrite / background_output | 当前环境的计划与协作工具，无工具时主智能体完成 |
| 立即启动六个调查任务，再按规模公式追加 | 按独立证据缺口与可用并发决定委派 |
| LSP 与 ast-grep 强制参与 | 尊重项目检索约定，工具可用时补充，未知指标不编造 |
| create-new 先删除全部既有指导 | 先读取、保留约定、生成新稿，再逐文件写入；删除需要明确范围 |
| 未明确的加权评分 | 满足条件加权，定义统计边界、未知项和合理例外 |
| 固定章节、行数、生成元数据 | 按信息价值选择，行数为目标，不为模板制造内容 |

保留核心能力，同时允许没有自定义角色、LSP、ast-grep 或持久索引的 Codex 环境使用。本 skill 不安装这些能力，也不运行模型调度服务。
