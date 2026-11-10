# 设计说明：OpenIDEA 提交信息生成插件

## 1. 结论先行

OpenIDEA 0.1.40（`openidea:plugin@1.4.0`）已经提供原生提交工作流：

- `[[contributes.menus]] menu = "commit.message"`：在 Commit 工具窗和 Commit 对话框的提交信息上方显示插件命令按钮；API 1.4 允许命令各自声明 `icon`；
- `host::pending_commit()`：返回当前窗口正在准备的提交，包括实际包含的文件、部分 hunk 的 diff、`amend` 和消息草稿；
- `host::set_commit_message(revision, text)`：把生成结果写入原生提交信息框，并按 `revision` 防止覆盖用户后来输入的内容；
- `Project::vcs_log()`、`ui.input-multiline`、`ui.pick` 与 `clipboard`：最近提交风格参考、附加要求输入、模型列表选择和复制。

因此插件不再需要自己的 Tool Window、文件勾选清单、剪贴板复制，也不再需要 `process:git`。插件从「工具窗 + 复制粘贴」改为「原生提交面板按钮 + 一键填入」；工具栏上的四个按钮分别对应生成、带附加要求生成、草稿格式化和手动编辑（打开结构化编辑器视图）；按钮各自可隐藏。

## 2. 架构

```
OpenIDEA Commit 工具窗 / Commit 对话框
  └─ 提交信息工具栏（插件命令 Generate Commit Message）
        │
        ├─ host::pending_commit()
        │     └─ CommitDraft { revision, message, amend, changes }
        │          每个 change：repository / path / status / diff / truncated
        │
        ├─ Project::vcs_status()（仓库根目录、分支）
        ├─ Project::vcs_log()（最近五条提交标题，风格参考）
        │
        └─ WASM 插件（Rust SDK 1.4）
              ├─ git.rs    从 CommitDraft 构建 prompt 上下文，裁剪 diff，读取历史标题
              ├─ prompt.rs   三种任务的 prompt + JSON 字段契约 + 模板渲染（内置 Default 与命名模板）
              ├─ thinking.rs 思考等级、预设映射、校验、等级收敛与 token budget
              ├─ llm.rs      host::fetch → 三种协议、模型列表、兼容重试、请求诊断、密钥槽
              ├─ config.rs   设置、服务商预设、命名档案解析、项目层选择、编辑器与按钮设置
              ├─ editor.rs   手动结构化编辑器（视图）：七个字段的表单、本地渲染写回、按项目草稿
              ├─ ui.rs       中英日韩四份通知与对话框文案（宿主文案目录只覆盖声明式文本）
              └─ lib.rs      命令入口：generate / generate-with-context / format / manual / models /
                           endpoint / test / template-preview / template-help / template-default /
                           thinking-levels / thinking-help；以及 views 导出、动作显隐的 context key
```

两条并行的入口：

- **生成路径**：提交面板按钮 → prompt → `host::fetch` → 解析 → 本地渲染 → `set_commit_message`。
- **手动路径**（清单 3）：`[[contributes.views]]` 工具窗表单 → 字段事件写入插件状态 → `Write to commit message` → 本地渲染 → `set_commit_message`，与生成共用同一 `revision` 保护。

插件状态只有两处持久化，都用 `host::storage`（`storage` 能力，64 MiB 上限，卸载时删除）：按项目保存的编辑器草稿（键 `editor:<项目根>`）和按端点/模型/协议/思考映射/温度配置保存的兼容结果缓存（键 `compat:<hash>`）。宿主会在空闲 5 分钟后回收 WASM 实例，因此这两份状态不能只放在内存里。

## 3. 数据采集

- `host::pending_commit()` 读取当前窗口的 Commit draft；没有仓库时返回 `None`。
- 只包含用户在 Commit 面板中勾选的文件和变更块；部分 hunk 选择也按实际包含范围提供 diff。
- `CommitChange::diff` 为相对 HEAD 的 unified diff；二进制内容为 `None`；所有变更的 diff 合计最多 1 MiB，超过后 `truncated = true`。
- 读取前会保存相关文件的未保存编辑器，与用户直接点击 Commit 时的行为一致；不会暂存或提交任何内容。
- `Project::vcs_status()` 提供分支和仓库根目录；`Project::vcs_log(None, 5)`（API 1.4）提供最近五条提交标题，只作风格参考写入 prompt。读取失败（新仓库、未信任项目、无提交）时该段落省略，生成流程不受影响。
- 发给模型的数据范围：所选文件的状态、`pending_commit()` 返回的所选 diff，以及上述五条历史标题。**不**使用 `Project::vcs_diff()`（工作区全量差异），也不读取历史提交的正文或 diff。清单第 30 项要求的数据范围说明即在此处体现。
- 不需要 `git` 可执行文件，也不需要 `process:git` 高风险能力。

## 4. LLM 协议与配置

支持：

| 协议 | 路径 | 认证 |
| --- | --- | --- |
| OpenAI Chat Completions | `{base}/chat/completions` | `Authorization: Bearer` |
| OpenAI Responses | `{base}/responses` | `Authorization: Bearer` |
| Anthropic Messages | `{base}/v1/messages` | `x-api-key` + `anthropic-version` |

Base URL 留空时使用 provider 预设；有路径时直接追加协议后缀，只有主机名时补 `/v1`。API Key 是插件 secret，由 IDE 在发出请求时填入，插件代码读不到。

设置使用插件 API 1.3 的声明式多页设置：

- **Commit**：提交信息语言（`auto` 跟随 IDE 界面语言）、最大 diff 字符数、允许的提交类型（标题＋描述表格）；
- **Prompts**：额外系统提示词、提示词档案表格与默认选择；
- **Commit Templates**：输出模板档案表格与默认选择；**Preview**（编辑模板并切换示例字段查看渲染结果）、**Variables**（变量说明、示例与错误）、**Restore Default**（复制内置模板供粘贴）三个动作。
- **LLM Settings**：Provider、协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens、Thinking 组（等级、预设、自定义映射）、模型档案表格与活动档案选择；页面的 **Test**、**Models**、**Request URL**、**Thinking Levels**、**Thinking Help** 五个动作都会先应用待保存设置。**Test** 发起一次最小请求并给出诊断摘要，**Models** 请求 `GET {base}/models` 后选择并把 id 复制到剪贴板（声明式设置对插件只读，用户自行粘贴到 Model），**Request URL** 显示按 Base URL 和协议拼接出的实际请求地址。**Thinking Levels** 显示当前映射实际提供的等级，**Thinking Help** 显示映射规则、当前校验结果并复制预设模板。

### 4.2 命名档案与项目默认（0.7.0）

三种档案都用声明式表格（`array` of `object`）保存，单元格只支持标量，长文本列声明 `multiline` 后由宿主渲染为多行输入框。插件对设置只读，所以档案的新建、改名、编辑、复制、删除与排序都由 IDE 设置界面完成。复制操作与旧表格数据升级需要应用随仓库提供的 [OpenIDEA 宿主补丁](patches/openidea-0.1.40-plugin-settings.patch)，应用说明见迁移清单的验收修复记录。

| 档案 | 表格键 | 选择键 | 未选择时的回退 |
| --- | --- | --- | --- |
| 模型 | `modelProfiles` | `activeModel` | LLM Settings 原有的单份设置 |
| 提示词 | `promptProfiles` | `defaultPrompt` | 原有的 `extraPrompt` |
| 输出模板 | `templates` | `defaultTemplate` | 内置的 **Default** 模板 |

`Config::load()` 把选中档案解析成一组扁平的最终值，之后 `llm.rs`、`prompt.rs` 只读这些值；三个页面动作读取的也是活动模型档案。选择键或名字不匹配时直接回退，0.6.x 的设置不需要迁移。

选择键是普通设置键，宿主按 project over global 解析：项目层有值就用项目值，项目里留空即继承全局选择；表格本身也按同一规则分层，不合并两层档案库（清单第 10、14 项）。

密钥不能写进表格。manifest 静态预声明四个密钥槽（`api-key`、`api-key-2`、`api-key-3`、`api-key-4`），模型档案在 **Key slot** 列用 enum 选一个，请求头写 `{{secret:<slot>}}` 由宿主从钥匙串填充。槽名在进入处校验，非法值回退 `api-key`；只检查活动档案对应的槽是否填写（清单第 20 项）。

网络白名单通过 `plugin.toml` 的 optional `net:` 能力声明；内置 OpenAI、Anthropic、DeepSeek、Qwen、Kimi、GLM、MiniMax、SiliconFlow、Volcengine、Azure OpenAI、OpenRouter、Groq、Mistral、Google Gemini、`127.0.0.1`（Magpie）与 `localhost`（Ollama）。`clipboard` 也是可选能力，只用于复制选中的模型 id。Magpie 示例：`provider = Magpie`、`baseUrl = http://127.0.0.1:3425`、`model = <Magpie 模型 id>`。

### 4.1 参数兼容、预算与诊断

一次命令的首次请求和兼容重试共享 50 秒预算，处于宿主 60 秒后台任务窗口内；重试前若剩余不足 8 秒则停止并报告，不再发请求。

| 协议 | token 字段 | 拒绝时的调整 |
| --- | --- | --- |
| Chat Completions | `max_tokens` | 4xx 明确要求 `max_completion_tokens` 时改用该字段 |
| Responses | `max_output_tokens` | 不调整 |
| Anthropic Messages | `max_tokens` | 不调整（Anthropic 本就接受该字段） |

`temperature` 可以用 **Send temperature** 主动省略；任何协议下，4xx 明确拒绝温度时也会去掉后重试。只调整插件自己选择的参数：密钥错误、5xx 和其他失败直接报告，不重试；最多三次尝试。成功通知和 **Test** 结果都列出实际请求地址、token 字段及其值、温度、尝试次数、已做的调整，以及实际发送或省略的思考参数。

### 4.3 思考参数（0.7.1）

每个模型档案（或单份设置）有一组 Thinking 值：等级 `DEFAULT / DISABLED / ENABLED / LOW / MEDIUM / HIGH / MAX`、预设 `model-default / auto / openai-effort / nested-effort / thinking-object / enable-thinking / anthropic-budget / anthropic-adaptive / custom`，以及可选的自定义映射。`DEFAULT` 或 `model-default` 不发送任何思考参数，0.7.0 的请求形态保持不变。

预设是一个按等级名作键的 JSON 对象，每个值是一段请求体片段，生成请求时合并到顶层字段。`auto` 按档案协议选标准参数（Chat Completions 用 `reasoning_effort`，Responses 用 `reasoning.effort`，Anthropic 用 `thinking` 对象加预算占位符）；其余预设对应 `reasoning_effort`、嵌套 effort、`thinking` 开关、`enable_thinking` 布尔值、Anthropic budget / adaptive 等参数族。`auto` 的 `MAX` 映射为 `high`，需要 `max` / `xhigh` 的网关写自定义映射。这里不做参考项目按供应商名猜测参数族的启发式：映射是请求片段，不是能力探测。

`thinkingMapping` 列非空时覆盖预设模板，清空即恢复。进入处校验 JSON 对象、等级名、隐式 `DEFAULT`、空片段、null、保留请求字段（`model`、`messages`、`temperature`、各 token 字段等）、`$thinking_budget` 只能出现在 `thinking.budget_tokens`、嵌套不超过 12 层、大小不超过 64 KiB。`thinking.budget_tokens` 的占位符按等级占响应上限的 25/50/75/90%，预留 256 token 给答案；显式数字需不小于 1024 且小于上限，上限不足时该等级不可用。

占位符位置按真实 JSON 路径片段判断。预算型思考的 **Test** 使用活动档案的响应上限，与生成时发送的预算一致；其他连接探针使用 16 token。模板渲染结果为空或只有空白时，生成流程会回退内置 Default 并报告模板错误。

声明式表格的 enum 无法条件化，因此 **Thinking Levels** 动作显示映射实际提供的等级；插件在解析时收敛：等级没有映射、映射无效或 budget 不通过时按 Model default 发送，并在诊断里写明原因。请求诊断也显示实际发送的片段或未发送的原因。被服务端明确拒绝思考字段时直接报告失败，移除重试（第 38 项）和拒绝结果缓存（第 39 项）不在本批。

## 5. 输出格式

模型返回结构化字段：

```json
{"type":"","scope":"","subject":"","body":"","changes":"","closes":"","skipCi":""}
```

插件在本地渲染为：

```
type(scope): subject

- body bullet
- body bullet

BREAKING CHANGE: ...
Closes #123
[skip ci]
```

最终格式由插件控制，而不是让模型自由发挥；便于保证团队一致性。解析支持 `<think>` 包裹、Markdown 代码围栏、body 数组、字段逐个提取，最后回退到普通 Conventional Commit 行。

0.7.0 起最终格式可由命名模板覆盖。变量为 `{{type}}`、`{{scope}}`、`{{subject}}`、`{{body}}`、`{{changes}}`、`{{closes}}`、`{{skipCi}}` 与 `{{header}}`（即 `type(scope): subject`）；条件段 `{{#field}}…{{/field}}` 只在该字段非空时输出，因此字段顺序、分隔符、可选段落和 footer 都由模板决定，静态文本原样保留。内置 `DEFAULT_TEMPLATE` 与旧的本地渲染等价，且不作为表格行存在，同名行无法覆盖它。模板解析遇到未知变量、嵌套段或未闭合段时返回错误，生成流程回退内置 Default 并在通知里说明，不会丢掉模型结果。完整 Velocity 兼容（清单第 46 项）单独评估。

Commit Templates 页的 **Preview** 用 `pick_many` 选择哪些示例字段有值，再用多行输入框编辑模板：确认即渲染，可反复修改，取消结束；模板错误在预览里直接显示。**Variables** 给出变量、条件段、示例、错误类型和恢复入口，**Restore Default** 把内置模板复制到剪贴板（未授权剪贴板时在通知里显示）。宿主表格没有编辑事件，插件也不能写设置值，所以预览不是设置页内的随改随刷，恢复需要用户粘贴或清空 **Default template**。

## 6. 交互流程

1. 用户打开 Commit 工具窗或 Commit 对话框，勾选本次要提交的文件/变更块。
2. 点击工具栏上的四个按钮之一（或用 Find Action / `⌥⌘G` 调用生成）：
   - **Generate Commit Message**：只用所选变更生成；
   - **Generate Commit Message with Requirements**：先用 `ui.input-multiline` 填写本次的业务背景、Bug 说明、issue 或 Skip CI 要求，取消或留空则不发请求，填写内容只进入这一次的 user prompt；
   - **Format Commit Message**：把提交框里已有的草稿连同所选 diff 交给模型，要求保留事实并重写为规范字段；草稿为空时直接提示，不发请求；
   - **Edit Commit Message Fields**：打开手动编辑器视图，不调用模型（见下方手动编辑路径）。
3. 插件在后台任务中调用 `pending_commit()`，按任务类型构建 prompt 并请求 LLM；同时如果提交信息框为空，先写入「正在生成提交信息…」作为执行中的可见反馈（1.4 仍没有按钮旁的 spinner，也没有输入框占位符 API），失败或取消时清掉，用户中途输入过则不动。
4. 模型返回后，插件解析并渲染 Conventional Commit 文本，再调用 `set_commit_message(draft.revision, message)`；通知里附上本次请求诊断。
5. 用户在原生提交框里检查/修改后手动提交；插件不会自动提交。

如果生成期间用户修改了消息或勾选内容，`draft.revision` 已变化，`set_commit_message` 返回 `Error::Stale`，插件只提示「提交面板内容已变化」，不覆盖当前文本。

手动编辑路径（清单 3）：提交信息上方的 **Edit Commit Message Fields** 按钮调用 `host::reveal_view("commit-editor")`，打开工具窗表单（type / scope / subject / body / breaking change / closes / Skip CI）。表单渲染由 `Commit` 设置页决定：六个显隐开关、类型排序（表格顺序或常用置顶）、Skip CI 标记表格与默认值。文本字段变化只更新插件状态并写入 `storage`，保留宿主输入状态；选择框和复选框变更后刷新视图以同步显示；`Write to commit message` 先按生成路径的同一标题约束校验，再读取 `pending_commit()` 的当前 `revision`，替换已有提交信息；读取后若草稿变化则拒绝写入，成功或清空后删除草稿。宿主表单的选择项只渲染下拉框，所以「单选列表 / 混合样式」不可用，类型样式部分只完成排序与数量。

提交信息上方的四个按钮（生成 / 带附加要求生成 / 格式化 / 手动编辑）各自有独立 `when`：`activation = ["on-startup-finished"]` 与 `settings-changed` 事件时，插件把 `Buttons` 设置写成 `plugin.<id>.<action*>` context key，`plugin.toml` 的菜单 `when` 读取同名键，命令自身仅要求仓库可用，因此按钮隐藏后仍可从 Find Action 和快捷键运行。启动即写是为了避免「按钮隐藏后插件永不激活」的死锁。

## 7. 已知限制

1. `pending_commit()` 的 diff 总量由 IDE 限制为 1 MiB；插件再按 `maxDiffChars` 裁剪 prompt。
2. 单次命令受宿主 60 秒后台任务上限约束；插件把首次请求和兼容重试放进共享的 50 秒预算，单次请求超时随剩余时间递减且不低于 8 秒。无流式输出。
3. `pending_commit` / `set_commit_message` 只适用于可信项目，且必须从某个窗口的 command/event 调用。
4. 多仓库项目会在 `changes` 中返回多个仓库的文件；分支与最近提交来自当前项目对应的仓库状态。
5. 插件设置是 IDE 声明的多页设置，不支持任意 Web 设置页或条件字段，插件也不能写入设置值：模型列表选择通过剪贴板转交用户；档案管理由声明式表格完成，表格列只支持标量、不支持嵌套集合。四个密钥槽是 manifest 静态声明的上限，档案数量不受此限，但只有四个可独立密钥。模板预览与思考映射等条件界面（清单 11、12、26、27）改用设置页命令动作：预览是「编辑 → 确认 → 看结果」的循环而不是表格内的随改随刷，等级收敛由 **Thinking Levels** 动作显示而不是动态枚举。
6. 诊断摘要列出 token 字段、温度、尝试次数、兼容调整、缓存命中，以及实际发送或省略的思考参数（24–28、39、40 项）。
7. 思考参数被服务端以 4xx 明确拒绝时直接报告失败，不做移除重试，也不缓存该拒绝（38 项仍未实现）；缓存只覆盖 token 字段与 temperature。首次请求与既有兼容重试仍共享 50 秒预算，重试次数未改变。
8. 工具窗表单的控件由宿主决定：`FieldValue` 只有文本、多行、复选框与下拉，因此类型样式没有单选列表；表单在设置变化后不会自动重绘，需要重新打开视图或再次触发动作。
9. 插件自身的通知与对话框文案按 `host::locale()` 选择（中、英、日、韩），声明式文本走 `locales/*.json` 目录；目录只覆盖标题类文案，设置项说明仍是英文，没有词条的文本由宿主回退原文。

## 8. 验证

- `cargo test`：127 个单元测试（配置、命名档案与项目默认回退、字段规范化与断句、质量判据、自由文本语言与四语言文案、编辑器渲染/写回/显隐/排序/预设、按项目草稿、动作显隐与事件、兼容缓存命中与分片、CommitDraft 上下文、历史标题与受限项目降级、diff 裁剪、三种任务的 prompt、解析/模板渲染与错误回退、模板预览/帮助/恢复、思考等级与预设模板、映射校验与等级收敛、token budget 解析与拒绝、三种协议响应解析、密钥槽、模型列表、兼容重试、请求诊断、生成并写回提交信息）。
- `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`。
- `cargo build --release --target wasm32-wasip2`。
- `openidea-plugin check .`（用 OpenIDEA 0.1.40 构建的 CLI）：manifest（含视图、四个按钮的 `when`、`activation`）、贡献点、能力、`locales/*.json` 词条格式与 wasm 组件全部通过，组件被识别为插件 API 1.4。
- 另以宿主真实 React 控件验证了类型与 Skip CI 下拉显示更新、复选框勾选/取消，并以当前 manifest 核对 Find Action 和快捷键条件，2 项隔离检查通过。
- 完整桌面界面与真实端点的行为尚未实测：上述验证覆盖 SDK mock host、宿主控件隔离检查与包结构校验；在 OpenIDEA 0.1.40 + 本地 Magpie 或真实服务端点上的四个按钮（含 `when` 生效与图标）、工具窗表单的渲染与字段事件、五个页面动作、字段多选、多行输入框、模型列表选择、日韩文案在设置页的展示，以及各思考预设片段和兼容缓存在真实端点上的表现，仍需人工验证。
