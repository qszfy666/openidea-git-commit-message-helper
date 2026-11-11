# 设计说明：OpenIDEA 提交信息生成插件

## 1. 结论先行

OpenIDEA 0.1.40（`openidea:plugin@1.4.0`）已经提供原生提交工作流：

- `[[contributes.menus]] menu = "commit.message"`：在 Commit 工具窗和 Commit 对话框的提交信息上方显示插件命令按钮；API 1.4 允许命令各自声明 `icon`；
- `host::pending_commit()`：返回当前窗口正在准备的提交，包括实际包含的文件、部分 hunk 的 diff、`amend` 和消息草稿；
- `host::set_commit_message(revision, text)`：把生成结果写入原生提交信息框，并按 `revision` 防止覆盖用户后来输入的内容；
- `Project::vcs_log()` 与 `ui.input-multiline`：最近提交风格参考、附加要求输入。

因此插件不再需要自己的 Tool Window、文件勾选清单、剪贴板复制，也不再需要 `process:git`。插件从「工具窗 + 复制粘贴」改为「原生提交面板按钮 + 一键填入」；工具栏上的两个按钮分别对应生成和带附加要求生成；按钮各自可隐藏。

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
        └─ WASM 插件（Rust SDK 1.5）
              ├─ git.rs    从 CommitDraft 构建 prompt 上下文，裁剪 diff，读取历史标题
              ├─ prompt.rs   两种任务的 prompt + JSON 字段契约 + 模板渲染（内置 Default 与命名模板）
              ├─ thinking.rs 思考等级、预设映射、校验、等级收敛与 token budget
              ├─ llm.rs      host::fetch → 三种协议、兼容重试、请求诊断
              ├─ config.rs   设置、服务商预设、所选模板、按钮设置
              ├─ ui.rs       中英日韩四份通知与对话框文案（宿主文案目录只覆盖声明式文本）
              └─ lib.rs      命令入口：generate / generate-with-context / test /
                           template-help / template-default；
                           以及动作显隐的 context key
```

两条并行的入口：

- **生成路径**：提交面板按钮 → prompt → `host::fetch` → 解析 → 本地渲染 → `set_commit_message`。

插件状态只有一处持久化，用 `host::storage`（`storage` 能力，64 MiB 上限，卸载时删除）：按端点/模型/协议/思考映射/温度配置保存的兼容结果缓存（键 `compat:<hash>`）。宿主会在空闲 5 分钟后回收 WASM 实例，因此这份状态不能只放在内存里。

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

Base URL 留空时使用 provider 预设；有路径时直接追加协议后缀，只有主机名时补 `/v1`。API Key 是插件 secret `api-key`，请求头写 `{{secret:api-key}}`，由 IDE 在发出请求时从钥匙串填入，插件代码读不到。

设置使用插件 API 1.3 的声明式多页设置：

- **Commit**：提交信息语言（`auto` 跟随 IDE 界面语言）、最大 diff 字符数、允许的提交类型（标题＋描述表格）；
- **Prompts**：额外系统提示词；
- **Commit Templates**：输出模板表格与默认选择；**Variables**（变量说明、示例与错误）、**Restore Default**（复制内置模板供粘贴）两个动作。
- **LLM Settings**：Provider、协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens、Thinking 组（等级、预设、自定义映射）；页面只有 **Test** 一个动作，先应用待保存设置，再发起一次最小请求并给出诊断摘要。设置页只在按钮下方显示命令错误，成功结果只走通知；设置窗口不显示插件通知，所以插件的通知都用 `Project::notify`（API 1.5）发到项目主窗口，没有项目时才发给触发窗口。Model 是文本框：0.1.41 的设置项只支持 schema 里写死的 `enum`，插件也不能写设置，所以做不成从端点拉取的下拉框。

### 4.2 输出模板与项目默认（0.7.0）

输出模板用声明式表格 `templates`（`array` of `object`）保存，单元格只支持标量，模板列声明 `multiline` 后由宿主渲染为多行输入框。插件对设置只读，所以模板的新建、改名、编辑、复制、删除与排序都由 IDE 设置界面完成。`defaultTemplate` 选择用哪一行，留空或名字不匹配时用内置 **Default** 模板。复制操作与旧表格数据升级需要应用随仓库提供的 [OpenIDEA 宿主补丁](patches/openidea-0.1.40-plugin-settings.patch)，应用说明见迁移清单的验收修复记录。

`defaultTemplate` 是普通设置键，宿主按 project over global 解析：项目层有值就用项目值，项目里留空即继承全局选择；表格本身也按同一规则分层，不合并两层（清单第 10、14 项）。

0.7.0 引入的模型档案（`modelProfiles` / `activeModel`）、提示词档案（`promptProfiles` / `defaultPrompt`）和密钥槽 `api-key-2`～`api-key-4` 已移除，连接、思考参数和额外提示词都只读单项设置；旧版本存下的档案值不再读取。

网络白名单通过 `plugin.toml` 的 optional `net:` 能力声明；内置 OpenAI、Anthropic、DeepSeek、Qwen、Kimi、GLM、MiniMax、SiliconFlow、Volcengine、Azure OpenAI、OpenRouter、Groq、Mistral、Google Gemini、`127.0.0.1`（Magpie）与 `localhost`（Ollama）。可选的 `net:ask`（API 1.5）覆盖列表以外的 HTTPS 主机：命令第一次连接某个主机时宿主弹框询问，用户逐个允许或拒绝；后台调用不询问，非 loopback 的明文 HTTP 仍被拒绝。`clipboard` 也是可选能力，只用于 **Restore Default** 复制内置模板。Magpie 示例：`provider = Magpie`、`baseUrl = http://127.0.0.1:3425`、`model = <Magpie 模型 id>`。

### 4.1 参数兼容、预算与诊断

一次命令的首次请求和兼容重试共享 50 秒预算，处于宿主 60 秒后台任务窗口内；重试前若剩余不足 8 秒则停止并报告，不再发请求。

| 协议 | token 字段 | 拒绝时的调整 |
| --- | --- | --- |
| Chat Completions | `max_tokens` | 4xx 明确要求 `max_completion_tokens` 时改用该字段 |
| Responses | `max_output_tokens` | 不调整 |
| Anthropic Messages | `max_tokens` | 不调整（Anthropic 本就接受该字段） |

`temperature` 可以用 **Send temperature** 主动省略；任何协议下，4xx 明确拒绝温度时也会去掉后重试。只调整插件自己选择的参数：密钥错误、5xx 和其他失败直接报告，不重试；最多三次尝试。成功通知和 **Test** 结果都列出实际请求地址、token 字段及其值、温度、尝试次数、已做的调整，以及实际发送或省略的思考参数。

### 4.3 思考参数（0.7.1）

LLM Settings 有一组 Thinking 值：等级 `DEFAULT / DISABLED / ENABLED / LOW / MEDIUM / HIGH / MAX`、预设 `model-default / auto / openai-effort / nested-effort / thinking-object / enable-thinking / anthropic-budget / anthropic-adaptive / custom`，以及可选的自定义映射。`DEFAULT` 或 `model-default` 不发送任何思考参数，0.7.0 的请求形态保持不变。

预设是一个按等级名作键的 JSON 对象，每个值是一段请求体片段，生成请求时合并到顶层字段。`auto` 按所配协议选标准参数（Chat Completions 用 `reasoning_effort`，Responses 用 `reasoning.effort`，Anthropic 用 `thinking` 对象加预算占位符）；其余预设对应 `reasoning_effort`、嵌套 effort、`thinking` 开关、`enable_thinking` 布尔值、Anthropic budget / adaptive 等参数族。`auto` 的 `MAX` 映射为 `high`，需要 `max` / `xhigh` 的网关写自定义映射。这里不做参考项目按供应商名猜测参数族的启发式：映射是请求片段，不是能力探测。

`thinkingMapping` 列非空时覆盖预设模板，清空即恢复。进入处校验 JSON 对象、等级名、隐式 `DEFAULT`、空片段、null、保留请求字段（`model`、`messages`、`temperature`、各 token 字段等）、`$thinking_budget` 只能出现在 `thinking.budget_tokens`、嵌套不超过 12 层、大小不超过 64 KiB。`thinking.budget_tokens` 的占位符按等级占响应上限的 25/50/75/90%，预留 256 token 给答案；显式数字需不小于 1024 且小于上限，上限不足时该等级不可用。

占位符位置按真实 JSON 路径片段判断。预算型思考的 **Test** 使用设置里的响应上限，与生成时发送的预算一致；其他连接探针使用 16 token。模板渲染结果为空或只有空白时，生成流程会回退内置 Default 并报告模板错误。

插件在解析时收敛：等级没有映射、映射无效或 budget 不通过时按 Model default 发送，并在诊断里写明原因。请求诊断也显示实际发送的片段或未发送的原因。被服务端明确拒绝思考字段时直接报告失败，移除重试（第 38 项）和拒绝结果缓存（第 39 项）不在本批。

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

Commit Templates 页的 **Variables** 给出变量、条件段、示例、错误类型和恢复入口，**Restore Default** 把内置模板复制到剪贴板（未授权剪贴板时在通知里显示）。插件不能写设置值，恢复需要用户粘贴或清空 **Default template**。原来的 **Preview** 已移除：它要弹选择框和输入框，而设置窗口不显示插件对话框，宿主等回答又没有超时，会让插件后续的命令一直排队。

## 6. 交互流程

1. 用户打开 Commit 工具窗或 Commit 对话框，勾选本次要提交的文件/变更块。
2. 点击工具栏上的两个按钮之一（或用 Find Action / `⌥⌘G` 调用生成）：
   - **Generate Commit Message**：只用所选变更生成；
   - **Generate Commit Message with Requirements**：先用 `ui.input-multiline` 填写本次的业务背景、Bug 说明、issue 或 Skip CI 要求，取消或留空则不发请求，填写内容只进入这一次的 user prompt。
3. 插件在后台任务中调用 `pending_commit()`，按任务类型构建 prompt 并请求 LLM；同时先把提交信息框换成「正在生成提交信息…」作为执行中的可见反馈（框里原来有字也换），失败或取消时恢复原来的文字，用户中途输入过则不动。
4. 模型返回后，插件解析并渲染 Conventional Commit 文本，再调用 `set_commit_message(draft.revision, message)`；通知里附上本次请求诊断。
5. 用户在原生提交框里检查/修改后手动提交；插件不会自动提交。

如果生成期间用户修改了消息或勾选内容，`draft.revision` 已变化，`set_commit_message` 返回 `Error::Stale`，插件只提示「提交面板内容已变化」，不覆盖当前文本。

提交信息上方的两个按钮（生成 / 带附加要求生成）各自有独立 `when`：`activation = ["on-startup-finished"]` 与 `settings-changed` 事件时，插件把 `Buttons` 设置写成 `plugin.<id>.<action*>` context key，`plugin.toml` 的菜单 `when` 读取同名键，命令自身仅要求仓库可用，因此按钮隐藏后仍可从 Find Action 和快捷键运行。启动即写是为了避免「按钮隐藏后插件永不激活」的死锁。

## 7. 已知限制

1. `pending_commit()` 的 diff 总量由 IDE 限制为 1 MiB；插件再按 `maxDiffChars` 裁剪 prompt。
2. 单次命令受宿主 60 秒后台任务上限约束；插件把首次请求和兼容重试放进共享的 50 秒预算，单次请求超时随剩余时间递减且不低于 8 秒。无流式输出。
3. `pending_commit` / `set_commit_message` 只适用于可信项目，且必须从某个窗口的 command/event 调用。
4. 多仓库项目会在 `changes` 中返回多个仓库的文件；分支与最近提交来自当前项目对应的仓库状态。
5. 插件设置是 IDE 声明的多页设置，不支持任意 Web 设置页、条件字段或由插件提供的下拉选项，插件也不能写入设置值，所以 Model 只能手动填写；模板表格列只支持标量、不支持嵌套集合。设置窗口既不显示插件通知也不显示插件对话框，设置页动作的结果改发到项目主窗口，需要对话框的动作不放在设置页。思考等级不在映射里时按 Model default 发送，由请求诊断说明，没有单独的查看入口。
6. 诊断摘要列出 token 字段、温度、尝试次数、兼容调整、缓存命中，以及实际发送或省略的思考参数（24–28、39、40 项）。
7. 思考参数被服务端以 4xx 明确拒绝时直接报告失败，不做移除重试，也不缓存该拒绝（38 项仍未实现）；缓存只覆盖 token 字段与 temperature。首次请求与既有兼容重试仍共享 50 秒预算，重试次数未改变。
8. 插件自身的通知、对话框与 LLM 错误文案按 `host::locale()` 选择（中、英、日、韩），声明式文本走 `locales/zh-CN.json`、`ja.json`、`ko.json`，覆盖命令、设置页、动作、密钥以及设置项的标题、分组和说明；设置值（如类型表格的默认说明）不翻译，它们会进入 prompt。OpenIDEA 0.1.40 的界面语言没有日文，`ja.json` 暂时不会被选中。

## 8. 验证

- `cargo test`：127 个单元测试（配置、命名档案与项目默认回退、字段规范化与断句、质量判据、自由文本语言与四语言文案、编辑器渲染/写回/显隐/排序/预设、按项目草稿、动作显隐与事件、兼容缓存命中与分片、CommitDraft 上下文、历史标题与受限项目降级、diff 裁剪、三种任务的 prompt、解析/模板渲染与错误回退、模板预览/帮助/恢复、思考等级与预设模板、映射校验与等级收敛、token budget 解析与拒绝、三种协议响应解析、密钥槽、模型列表、兼容重试、请求诊断、生成并写回提交信息、中文文案引用设置页译名、LLM 错误按界面语言输出且各语言占位符一致）；另有 1 个集成测试（`tests/locales.rs`）核对三份词条覆盖 manifest 与设置 schema 中的全部显示文本。
- `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`。
- `cargo build --release --target wasm32-wasip2`。
- `openidea-plugin check .`（用 OpenIDEA 0.1.40 构建的 CLI）：manifest（含四个按钮的 `when`、`activation`）、贡献点、能力、`locales/*.json` 词条格式与 wasm 组件全部通过，组件被识别为插件 API 1.4。
- 另以宿主真实 React 控件验证了类型与 Skip CI 下拉显示更新、复选框勾选/取消，并以当前 manifest 核对 Find Action 和快捷键条件，2 项隔离检查通过。
- 正式 OpenIDEA 0.1.40 上的联调结果见 `docs/integration-test-report-0.8.0.md`：生成、按补充要求生成、格式化、参数兼容重试与缓存、提示词档案、提交模板、思考参数和四个按钮图标已用本地 mock 端点实测通过。仍未实测的项列在报告第 6 节，包括字段列表的逐项编辑与写回、五个页面动作、日韩界面语言，以及真实服务端点上的 Models、Request URL、Test 和 50 秒预算超时。
