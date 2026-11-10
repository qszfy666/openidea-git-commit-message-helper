# 参考插件功能迁移清单

本清单对照 IntelliJ 参考插件与当前 OpenIDEA 插件，记录已覆盖能力、可迁移功能、适配条件和建议顺序，供后续实现时逐项核对。

| 对照项 | 基线 |
| --- | --- |
| 分析日期 | 2026-11-10 |
| 参考仓库 | `AutismSuperman/git-commit-message-helper` |
| 参考源码 | `d71db758f23c3264272e5c5c39d1f2fdc43279b7`，2026-10-25 |
| 参考本地目录 | `/Users/liuzhiqiang/WorkProjects/Lab/OpenSource/git-commit-message-helper` |
| 当前插件 | `qszfy666.commit-message-helper`，0.5.3，提交 `1a5f264` |
| 当前 SDK | OpenIDEA `v0.1.39`，插件 API 1.3 |
| 目标宿主能力 | OpenIDEA `v0.1.40`，插件 API 1.4 |

参考项目的 manifest、Changelog 版本标记存在差异，因此以以上源码提交作为功能依据。清单来自静态代码和接口核对，界面与网络行为仍需实测。P0 表示建议优先，P1 表示后续完善，P2 表示按需求安排。优先级和分批顺序均为分析建议，实施范围待选定。

基线依据见 [参考源码](https://github.com/AutismSuperman/git-commit-message-helper/tree/d71db758f23c3264272e5c5c39d1f2fdc43279b7)、[当前依赖](../Cargo.toml)、[当前 manifest](../plugin.toml) 和 [OpenIDEA 0.1.40 发布说明](https://github.com/l0ng-ai/openidea/releases/tag/v0.1.40)。

状态标记（0.6.0 第一批、0.7.0 第二批子批次 1）：编号列的 `✅` 表示已完成，`◐` 表示部分完成，无标记即尚未开始；批次表的标记表示整批进度（第一批 ✅ 全部完成，第二批 ◐ 子批次 1 完成）。逐项说明见文末「第一批实施记录」与「第二批实施记录」。基线表及未标记条目仍是 2026-11-10 的分析快照。

当前已覆盖以下能力，后续功能可复用现有实现。

| 能力 | 当前情况 | 实现入口 |
| --- | --- | --- |
| 根据所选变更生成提交信息 | 原生 Commit 工具窗和对话框，包含部分 hunk | `src/lib.rs`、`src/git.rs` |
| 三种 LLM 协议 | Chat Completions、Responses、Anthropic Messages | `src/llm.rs` |
| 七个结构化字段 | `type`、`scope`、`subject`、`body`、`changes`、`closes`、`skipCi` | `src/prompt.rs` |
| 本地渲染 | 标题、正文、BREAKING CHANGE、Closes、Skip CI | `src/prompt.rs` |
| 自定义提交类型 | 字符串列表，可增删、排序 | `settings/commit.json`、`src/config.rs` |
| 长 diff 处理 | 字符预算、hunk 摘要、遗漏标记 | `src/git.rs` |
| 响应清理与容错解析 | 思考标签、代码围栏、部分字段、普通提交行 | `src/prompt.rs` |
| 长期提示词 | 单份 `extraPrompt`，保留输出约束 | `settings/prompts.json`、`src/prompt.rs` |
| 基础模型设置 | Provider、协议、URL、模型、温度、输出 token 上限 | `settings/llm.json`、`src/config.rs` |
| API Key | 宿主 secret，使用系统钥匙串 | `plugin.toml`、`src/llm.rs` |
| 连接测试与进度 | Test 命令、后台任务、生成提示 | `src/lib.rs` |
| 语言 | 提交信息中英文及自动选择，通知中英文 | `src/config.rs`、`src/ui.rs` |
| 写回保护 | `revision` 校验，生成期间用户修改草稿时拒绝旧结果写回 | `src/lib.rs` |

实现依据见 [生成入口](../src/lib.rs)、[上下文与 diff](../src/git.rs)、[协议客户端](../src/llm.rs)、[解析与渲染](../src/prompt.rs) 和 [配置](../src/config.rs)。

提交面板操作可以补齐四个主要动作及配套交互。

| 编号 | 功能 | 当前差距及迁移方式 | 优先级 |
| --- | --- | --- | --- |
| 1 ✅ | 带附加要求生成 | 新增多行输入，填写本次业务背景、Bug 说明、issue、Skip CI 要求 | P0 |
| 2 ✅ | 格式化现有提交草稿 | 把当前消息与所选 diff 一起交给模型，重写为规范字段 | P0 |
| 3 | 手动结构化编辑器 | 提供七个字段的表单，填写后本地渲染并写回 | P1 |
| 4 | Smart Echo 智能回填 | 将已有提交文本通过 LLM 解析回字段，再进入结构化编辑器 | P1 |
| 5 | 手动编辑草稿恢复 | 取消结构化编辑时按项目保留字段，下次继续，确认应用后清除 | P1 |
| 6 | 每个动作独立显隐 | 设置中控制生成、附加要求生成、格式化、手动创建四个按钮，通过 `when` 和 context key 映射 | P1 |
| 7 | 每个动作独立图标 | 为四个动作配置不同图标，API 1.4 可接入 | P1 |

参考实现见 [动作注册](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/resources/META-INF/plugin.xml)、[附加要求动作](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/action/GenerateCommitWithAdditionalContextAction.java)、[格式化动作](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/action/FormatCommitByLlmAction.java) 和 [手动创建与 Smart Echo](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/action/CreateCommitAction.java)。

模板、提示词和编辑偏好可以从当前固定输出格式、单份长期提示词扩展为可复用的命名档案。

| 编号 | 功能 | 当前差距及迁移方式 | 优先级 |
| --- | --- | --- | --- |
| 8 ✅ | 自定义输出模板 | 配置字段顺序、分隔符、可选段落及 footer，完整 Velocity 兼容范围见第 46 项 | P1 |
| 9 ✅ | 多模板档案 | 新建、重命名、编辑、删除，并保护内置 Default 模板 | P1 |
| 10 ✅ | 全局和项目默认模板 | 项目选择优先，未配置时使用全局选择 | P1 |
| 11 | 模板实时预览 | 编辑模板时查看渲染结果，并切换示例字段是否有值，需要适配预览界面 | P1 |
| 12 | 模板变量帮助与恢复默认 | 展示变量说明、示例、模板错误及恢复入口 | P1 |
| 13 ✅ | 多提示词档案 | 保存不同写作偏好，支持新建、重命名、编辑、删除 | P0 |
| 14 ✅ | 全局和项目默认提示词 | 当前已有项目设置覆盖机制，可进一步选择命名档案 | P0 |
| 15 ✅ | 提交类型描述 | 将字符串类型扩展为“标题＋描述”，供编辑器和模型使用 | P1 |
| 16 | 类型展示样式 | 下拉框、单选列表、混合模式，以及常用类型显示数量 | P2 |
| 17 | 编辑字段显隐 | 控制类型、scope、正文、breaking change、issue、Skip CI 等字段 | P2 |
| 18 | Skip CI 预设与默认值 | 选择不同标记，设置默认选项、默认勾选状态和下拉框显示 | P1 |

类型列表的排序当前已支持。参考项目的 emoji 类型可通过自定义标题表达，迁移重点在描述、示例和编辑体验。第 16–18 项依赖手动编辑器，第 4 项的 Smart Echo 也依赖该编辑路径。

参考实现见 [模板管理与预览](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/ui/setting/TemplateEditPanel.java)、[提示词档案](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/ui/setting/PromptEditPanel.java)、[结构化编辑界面](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/ui/commit/CommitPanel.java) 和 [设置与项目选择](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/storage/GitCommitMessageHelperSettings.java)。

模型管理可以从单份活动配置扩展为多档案，并提供按具体模型和协议配置的思考参数。

| 编号 | 功能 | 当前差距及迁移方式 | 优先级 |
| --- | --- | --- | --- |
| 19 ✅ | 多模型档案 | 保存多组配置，支持新建、编辑、复制、删除和选择活动模型 | P0 |
| 20 ✅ | 每档案独立密钥 | 结合 OpenIDEA manifest 预声明的 secret 设计密钥槽和档案关联 | P1 |
| 21 ✅ | 拉取服务端模型列表 | 请求模型列表后选择，同时保留手动输入 | P0 |
| 22 ✅ | 最终请求地址预览 | 根据 Base URL 和协议，显示实际请求路径 | P0 |
| 23 ✅ | 每档案输出 token 上限 | 当前已有单份配置，扩展到每个模型独立保存 | P1 |
| 24 | 思考等级 | 模型默认、关闭、启用、低、中、高、最大等，按配置提供 | P0 |
| 25 | 思考参数预设 | 覆盖 `reasoning_effort`、嵌套 effort、thinking 对象、布尔开关、Anthropic budget/adaptive 等 | P0 |
| 26 | 自定义思考 JSON 映射 | 按模型与协议配置请求片段，并校验格式、深度及保留字段 | P1 |
| 27 | 思考等级随映射变化 | 映射只配置启用时，界面仅提供相应等级 | P1 |
| 28 | 思考 token budget 校验 | 解析预算占位符，检查预算与最大响应 token 的关系 | P1 |
| 29 | 自定义提交信息语言 | 当前固定中英文，可扩展为语言名称输入或更多选项 | P1 |

模型档案中的密钥需要通过宿主 secret 管理。声明式设置可表达档案表格，思考映射等条件界面需要结合插件视图设计。思考等级和映射应按具体端点、模型及协议配置，模型默认选项表示使用服务端默认参数。

参考实现见 [模型档案管理](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/ui/central/LlmSettingPanel.java)、[模型列表和地址预览](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/ui/central/LlmProfileEditor.java)、[思考映射实现](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/ThinkingParameterMappings.java) 和 [思考配置说明](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/doc/llm-thinking.md)。宿主表格和界面约束见 [设置接口文档](https://github.com/l0ng-ai/openidea/blob/v0.1.40/plugins/README.md#settings)。

生成质量和协议兼容可以在现有解析基础上逐项补充。

| 编号 | 功能 | 当前差距及迁移方式 | 优先级 |
| --- | --- | --- | --- |
| 30 ✅ | 最近提交风格参考 | 原作读取最近五条标题，API 1.4 的 `vcs_log` 可提供上下文 | P0 |
| 31 | JSON 修复请求 | 响应不完整或解析失败时，携带原因请求模型修复一次 | P1 |
| 32 | 生成质量检查 | 对空泛标题、仅文件名等结果做本地检查，当前主要靠 prompt 约束 | P1 |
| 33 | 质量纠正请求 | 根据质量检查结果，要求模型重写一次 | P1 |
| 34 | 字段规范化 | 补充标题尾标点、标识符和重复 `Closes`、`BREAKING CHANGE` 前缀处理 | P1 |
| 35 | 单行多句正文转列表 | 当前主要按换行整理，原作还按句号、分号拆分多项描述 | P2 |
| 36 ✅ | Token 参数兼容 | 支持 `max_completion_tokens`，服务端明确拒绝时调整字段 | P0 |
| 37 ✅ | Temperature 兼容 | 根据模型配置省略温度，或在服务端明确拒绝后调整请求 | P0 |
| 38 | 思考参数兼容重试 | 明确拒绝映射字段时移除该覆盖并重试 | P1 |
| 39 | 兼容结果缓存 | 记住端点、模型、协议及思考映射的拒绝结果 | P1 |
| 40 ◐ | 请求诊断摘要 | 显示实际 token 字段、思考参数、尝试次数、兼容调整和缓存命中 | P0 |
| 41 ✅ | 更详细的连接测试 | 当前显示 HTTP 状态与模型，可加入上述诊断摘要和界面测试状态 | P0 |

参考实现见 [生成、修复及质量纠正](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmCommitService.java)、[协议参数调整](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/OpenAiCompatibleLlmProviderClient.java)、[兼容缓存](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmCapabilityCache.java)、[诊断摘要](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmRequestDiagnostics.java) 和 [VCS 接口](https://github.com/l0ng-ai/openidea/blob/v0.1.40/sdk/rust/openidea-plugin/wit/workspace.wit)。

以下功能涉及平台差异，需要适配设计或补充接口。

| 编号 | 功能 | 迁移条件 | 建议 |
| --- | --- | --- | --- |
| 42 | 流式请求及流式失败回退 | API 1.4 的 `fetch` 返回完整响应体，实时接收 SSE 需要宿主补充能力 | 待平台支持 |
| 43 | Git Log 编辑历史提交消息 | 原作能取得正在编辑的 commit hash，并读取该提交自身的 diff，当前公开接口缺少完整对应流程 | 待平台支持 |
| 44 | 原按钮切换为停止按钮 | 参考项目动态修改动作图标和状态，OpenIDEA 当前未暴露同等接口 | 先使用宿主任务取消 |
| 45 | 生成期间锁定提交输入框 | 原作直接控制 IntelliJ 编辑器只读状态，OpenIDEA 未提供对应控制 | 当前采用 revision 写回保护 |
| 46 | 完整 Velocity 模板兼容 | 包括条件、变量及 `$velocityTool` 字符串方法，需设计 Rust/WASM 兼容范围 | 单独评估 |
| 47 | 日文、韩文及完整界面本地化 | 原作有中英日韩资源，当前通知为中英文，设置和命令文案需要补齐 | P2 |

参考实现见 [历史提交上下文](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/GitContextService.java)、[提交面板控制](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/action/CommitPanelActionSupport.java)、[Velocity 渲染](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/utils/VelocityUtils.java) 和 [本地化资源](https://github.com/AutismSuperman/git-commit-message-helper/tree/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/resources/i18n)。平台约束见 [网络定义](https://github.com/l0ng-ai/openidea/blob/v0.1.40/sdk/rust/openidea-plugin/wit/io.wit)、[提交接口](https://github.com/l0ng-ai/openidea/blob/v0.1.40/sdk/rust/openidea-plugin/wit/commit.wit) 和 [提交工具栏实现](https://github.com/l0ng-ai/openidea/blob/v0.1.40/src/features/vcs/CommitView.tsx)。

实施时需要把以下事实纳入设计和验收。

- 参考项目把 subject 少于 12 个字符判为低质量，短中文标题也会触发。迁移第 32–33 项时需要重新定义语言相关判据。[原质量规则](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmCommitService.java#L617)
- 当前请求超时为 55 秒，宿主后台任务预算为 60 秒。JSON 修复、质量纠正和参数兼容重试需要共享预算，直接串联会超过现有执行窗口。[当前请求超时](../src/llm.rs)、[宿主任务预算](https://github.com/l0ng-ai/openidea/blob/v0.1.40/plugins/README.md#the-api)
- 参考项目的主生成流程会收齐流式 JSON，再渲染并回填。提交框逐字显示需要另行实现。[实际调用路径](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmCommitService.java#L202)
- “用户外部模板文件导入”暂列待确认。Changelog 提到 external template files，当前源码可确认的是读取包内 `includes/defaultTemplate.vm`，尚未找到用户选择外部文件的入口。[模板加载实现](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/constant/GitCommitConstants.java)
- 参考项目的兼容缓存采用进程内集合。OpenIDEA 会回收空闲 WASM 实例，迁移第 39 项时需明确缓存生命周期。[参考缓存](https://github.com/AutismSuperman/git-commit-message-helper/blob/d71db758f23c3264272e5c5c39d1f2fdc43279b7/src/main/java/com/fulinlin/service/LlmCapabilityCache.java)、[宿主生命周期](https://github.com/l0ng-ai/openidea/blob/v0.1.40/plugins/README.md#activation)
- 增加历史提交上下文后，需要更新发送给模型的数据范围说明。生成输入继续采用 `pending_commit()` 返回的所选文件和 hunk，`vcs_diff()` 提供的是工作区差异。[VCS 语义](https://github.com/l0ng-ai/openidea/blob/v0.1.40/sdk/rust/openidea-plugin/wit/workspace.wit)、[提交草稿语义](https://github.com/l0ng-ai/openidea/blob/v0.1.40/sdk/rust/openidea-plugin/wit/commit.wit)
- 模板、Smart Echo 和手动编辑结果写回时，复用现有 `revision` 校验。字段规范化与容错解析仍需满足当前允许类型和有效标题校验。[现有写回与校验](../src/lib.rs)、[字段校验](../src/prompt.rs)

建议分三批实施，每批再选定具体条目和验收条件。

| 批次 | 目标 | 建议包含的条目 |
| --- | --- | --- |
| 第一批 ✅ | 改善现有生成流程 | 1、2、21、22、30、36、37、40、41，即附加要求、草稿格式化、模型列表、地址预览、历史风格、基础参数兼容和诊断 |
| 第二批 ◐ | 完善配置复用 | 8–15、19、20、23–28，即多模型档案、思考参数、模板和提示词档案及项目默认选择。子批次 1（0.7.0）已完成 8–10、13–15、19、20、23；11、12、24–28 留在子批次 2 |
| 第三批 | 补齐手动编辑路径和其他体验 | 3–7、16–18，以及按需选择的质量、语言和本地化条目 |

接入 API 1.4 能力前，需要把 SDK 切换到 `v0.1.40`，同步更新 manifest 的 API 要求、兼容性说明和相关测试。原生提交选择、三种协议、secret 使用和 revision 写回保护是各批次共同的回归验证入口。已有架构说明见 [design.md](design.md)。

## 第一批实施记录（0.6.0）

上方各表编号列的 `✅` / `◐` 与本节逐项对应：第一批 9 项已在 0.6.0 实施，其中 40 为部分完成。SDK 升到 `v0.1.40`，`engines.openidea = "^1.4"`，`openidea-plugin check .`（0.1.40 CLI）通过，组件被识别为插件 API 1.4。上方基线表仍是 2026-11-10 的分析快照，未改写。

| 条目 | 实现入口 | 适配与边界 |
| --- | --- | --- |
| 1 | `generate-with-context` 命令、`ui.input-multiline`、`prompt::Task::Requirements` | 多行对话框；取消或留空不发请求；内容只进入本次 user prompt |
| 2 | `format` 命令、`prompt::Task::Format` | 草稿连同所选 diff 交给模型，保留事实、issue 号和语言；空草稿直接提示 |
| 21 | LLM Settings 的 **Models** 动作、`llm::models` | `GET {base}/models` + `ui.pick` + 剪贴板；宿主声明式设置对插件只读，选中 id 需粘贴到 Model，手动输入始终保留 |
| 22 | **Request URL** 动作、`llm::endpoint` | 设置页不支持计算字段，改为页面动作显示实际请求地址 |
| 30 | `git::build_context` 的 `recent_subjects`、prompt 的 Recent Commit Subjects 段 | `vcs_log(None, 5)` 只作风格参考；未使用 `vcs_diff()`；受限项目自动省略；数据范围说明已同步到 design.md 第 3 节与两份 README |
| 36 | `llm::RequestShape` | 4xx 明确拒绝 `max_tokens` 且要求 `max_completion_tokens` 时切换；未提供主动配置该字段的开关 |
| 37 | `sendTemperature` 设置 + 拒绝重试 | 两条路径都做了；Anthropic 与 Responses 不调整 token 字段 |
| 40 ◐ | `llm::Diagnostics` | 列出请求地址、token 字段及值、温度、尝试次数、兼容调整；**思考参数与缓存命中未纳入**，它们依赖第二/三批的 24–28、39 项，因此标为部分完成 |
| 41 | `test` 命令通知 | HTTP 状态与模型之外附诊断摘要；失败仍由设置页内联显示 |

清单末尾的时间预算约束已落实：首次请求与兼容重试共享 50 秒预算，剩余不足 8 秒不再发起请求，最多三次尝试；JSON 修复与质量纠正（31、33）不在本批，未串联。清单列出的共同回归入口（原生提交选择、三种协议、secret、revision 写回保护）均有测试覆盖，共 51 个单元测试。

界面与网络行为仍需实测：三个提交面板按钮、多行输入框、模型列表选择在 OpenIDEA 0.1.40 上的表现，以及真实服务端点的兼容重试，目前只有 SDK mock host 的行为验证。

## 第二批实施记录（0.7.0，子批次 1）

上方第二批建议的 16 项按主题拆成两个子批次。本轮子批次 1 完成 8–10、13–15、19、20、23；11、12、24–28 留在子批次 2。界面适配方式已确定为「设置页命令动作 + 通知/多行输入」，因为宿主声明式设置不支持条件字段与自定义设置页。验收沿用第一批门禁（cargo test / fmt / clippy / wasm build / 0.1.40 CLI check）。上方编号列的 `✅` 与下表逐项对应。

| 条目 | 实现入口 | 适配与边界 |
| --- | --- | --- |
| 8 | `prompt::DEFAULT_TEMPLATE`、`prompt::render_template` | 自定义模板用 `{{field}}` 变量与 `{{#field}}…{{/field}}` 条件段控制顺序、分隔符、可选段落与 footer；静态文本即 footer。不支持嵌套、循环、表达式与 `$velocityTool`（第 46 项另评）。模板无效时回退内置 Default，并在生成通知里写明原因，不丢结果 |
| 9 | `settings/templates.json` 的 `templates` 表格、`config::selected` | 表格行可增删、改名、编辑、排序；内置 **Default** 不作为表格行，`defaultTemplate` 为 `Default` 或留空时一律用内置模板，同名行无法覆盖 |
| 10 | `defaultTemplate` 设置键 | 宿主设置本身是 project over global：项目层有值就用项目值，留空继承全局。档案表格也按同一规则分层，未额外合并两层档案库 |
| 13 | `settings/prompts.json` 的 `promptProfiles` 表格 | 档案文本取代 `extraPrompt` 进入 system prompt，输出约束段落保留；表格单元格用 `multiline` 渲染为多行输入框 |
| 14 | `defaultPrompt` 设置键 | 与第 10 项相同的 project-over-global 语义；未选择或名字不存在时沿用 0.6.x 的单份 `extraPrompt` |
| 15 | `settings/commit.json` 的 `commitTypes` 表格、`config::CommitType`、`prompt::allowed_types` | 标题＋描述对象数组；描述写入 prompt 的 Allowed types 段，模型返回的 `type` 仍只按标题校验。字符串数组与逗号分隔旧值仍可解析，默认 11 个类型带英文描述 |
| 19 | `settings/llm.json` 的 `modelProfiles` 表格、`activeModel`、`config::Config::load` | 档案含 Provider、协议、Base URL、Model、Temperature、Send temperature、Max tokens、Key slot；`activeModel` 留空或名字不存在时回退原有单份设置。Test / Models / Request URL 三个页面动作读取的也是活动档案 |
| 20 | `plugin.toml` 四个 `[[contributes.secrets]]`、`config::secret_slot`、`llm::secret_placeholder` | 宿主要求 secret 静态预声明且插件读不到值，因此改为四个固定密钥槽，档案用 enum 选槽；请求头写 `{{secret:<slot>}}` 由宿主填充。槽名在进入处校验，非法值回退 `api-key` |
| 23 | 模型档案的 `maxTokens` 列 | 每个档案独立保存输出 token 上限，请求与诊断摘要都取活动档案的值 |

共同回归入口（原生提交选择、三种协议、secret、`revision` 写回保护）继续有测试覆盖，单元测试从 51 个增加到 70 个。清单末尾的时间预算约束未改变：本批没有新增请求重试路径。

界面与真实端点行为仍需实测：四个密钥槽的填写、声明式表格（含多行单元格）在 OpenIDEA 0.1.40 设置里的编辑体验、项目层覆盖全局层的选择，以及真实服务端点上的档案切换，目前只有 SDK mock host 的行为验证与包结构校验。子批次 2 的 11、12、24–28 将按已选的命令动作方式适配条件界面。
