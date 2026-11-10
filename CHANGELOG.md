# Changelog

## 0.7.1

迁移清单第二批的子批次 2（11、12、24–28）：模板预览与帮助、思考等级与参数映射。仍需要 OpenIDEA 0.1.40（插件 API 1.4），已有配置无需迁移。

- **模板实时预览**（11）：Commit Templates 页新增 **Preview** 动作。选择哪些示例字段有值后，动作打开预填当前活动模板的多行输入框；确认即显示渲染结果，可反复修改，取消结束。宿主声明式表格没有编辑事件，也没有自定义设置页，因此「实时」以「编辑 → 预览 → 再编辑」的命令循环适配。
- **模板变量帮助与恢复默认**（12）：**Variables** 列出八个变量、条件段语法、示例模板和会报错的写法，并指向恢复入口；**Restore Default** 把内置 Default 模板复制到剪贴板（未授权剪贴板时在通知里显示），粘贴到 Template 列或清空 **Default template** 即可恢复。预览会提前显示模板错误，生成时仍回退内置 Default。
- **思考等级与预设**（24、25）：LLM Settings 新增 Thinking 组（**Thinking level**、**Thinking preset**、**Thinking mapping**），模型档案表格也有同三列。等级为 DEFAULT / DISABLED / ENABLED / LOW / MEDIUM / HIGH / MAX；预设覆盖 `reasoning_effort`、嵌套 `reasoning.effort`、`thinking` 对象、`enable_thinking` 布尔开关、Anthropic `budget_tokens` / adaptive 与自定义映射，`auto` 按协议选择标准字段。默认 `DEFAULT` + `model-default` 不发送任何思考参数，0.7.0 的请求不变。
- **自定义映射校验**（26）：映射是按等级名作键的 JSON 对象，每个值是非空请求片段。进入处校验 JSON 格式、未知/隐式 DEFAULT 等级、null、保留请求字段、`$thinking_budget` 的位置、嵌套深度（12）和大小（64 KiB）；**Thinking Help** 显示规则、当前映射的校验结果，并复制当前预设模板供粘贴。
- **等级随映射收敛**（27）：声明式设置不能做条件枚举，**Thinking Levels** 动作显示映射实际提供的等级；映射没有该等级、映射无效或 budget 不可用时，请求按 Model default 处理并在诊断里写明原因。
- **思考 token budget 校验**（28）：`thinking.budget_tokens` 的 `$thinking_budget` 占位符按等级占响应 token 上限的 25/50/75/90% 解析，显式数字需不小于 1024 且小于上限；上限太小放不下合法预算时该等级不可用。
- **请求诊断**（40）：成功通知与 **Test** 结果现在列出实际发送或省略的思考参数；兼容结果缓存命中（39）仍未接入，因此 40 仍为部分完成。
- `cargo test` 从 70 个增加到 93 个，新增覆盖模板预览/帮助/恢复、等级与预设解析、映射校验、等级收敛、budget 解析与拒绝、请求片段合并、诊断摘要，以及原有写回保护等回归入口。

边界：清单 38（思考参数被明确拒绝后的兼容重试）与 39（兼容结果缓存）不在本批；服务端以 4xx 拒绝思考字段时目前直接报告失败，不做移除重试。首次请求与既有兼容重试仍共享 50 秒预算，本批没有新增请求路径。

界面与真实端点行为仍未实测：五个新页面动作、多行输入框、多选字段在 OpenIDEA 0.1.40 设置里的表现，以及真实服务端点对各预设片段的接受情况，目前只有 SDK mock host 的行为验证与包结构校验。

## 0.7.0

迁移清单第二批的子批次 1（8–10、13–15、19、20、23）：把单份活动配置扩展为可复用的命名档案。第二批剩余条目（11、12、24–28）留在同一批的子批次 2。仍需要 OpenIDEA 0.1.40（插件 API 1.4）。

- **模型档案**：LLM Settings 新增 **Model profiles** 表格与 **Active model profile**。每个档案独立保存 Provider、协议、Base URL、Model、Temperature、Send temperature 和 **Max response tokens**（23），可增删、改名、复制、排序。留空 Active model 时继续使用原有单份设置，0.6.x 的设置无需迁移。
- **每档案独立密钥**（20）：manifest 预声明四个固定密钥槽（`api-key`、`api-key-2`、`api-key-3`、`api-key-4`），档案在 **Key slot** 列选择其中一个。密钥仍由系统钥匙串保存，插件读不到值；只检查活动档案对应的槽是否填写。
- **提示词档案**：Prompts 页新增 **Prompt profiles** 表格与 **Default prompt profile**；选中档案的文本取代原来的额外系统提示词，输出约束继续生效。未选择时沿用 `extraPrompt`。
- **输出模板**：新增 **Commit Templates** 页，含 **Commit templates** 表格与 **Default template**。模板用 `{{field}}` 变量与 `{{#field}}…{{/field}}` 条件段控制字段顺序、分隔符、可选段落和 footer；内置 **Default** 模板始终存在，同名表格行不会覆盖它。模板无效时回退到内置 Default，并在通知里写明原因。完整 Velocity 兼容（清单 46 项）仍单独评估。
- **提交类型描述**：Commit 页的 **Allowed commit types** 改为「标题＋描述」表格，描述会写进 prompt 帮助模型选类型；旧的字符串数组和逗号分隔设置仍可读取，模型返回的 `type` 仍只按标题校验。
- **项目默认选择**：Active model / Default prompt / Default template 都是普通设置键，项目层的值覆盖全局层；项目里留空即继承全局选择。档案表格本身也遵循宿主的 project-over-global 规则。
- `cargo test` 从 51 个增加到 70 个，新增覆盖档案驱动请求、独立密钥槽、每档案 token 上限、提示词档案、模板渲染与回退、内置 Default 保护、类型描述，以及旧设置的回退路径。

界面与真实端点行为仍未实测：以上由 SDK mock host 的行为测试、0.1.40 CLI 的包结构校验覆盖；声明式表格在 OpenIDEA 设置里的实际编辑体验、四个密钥槽的填写与真实服务端点仍需人工验证。

## 0.6.0

迁移清单第一批（1、2、21、22、30、36、37、40、41）。需要 OpenIDEA 0.1.40：SDK 升到 `v0.1.40`，manifest 改为 `engines.openidea = "^1.4"`，`openidea-plugin check .` 用 0.1.40 的 CLI 验证，组件被识别为插件 API 1.4。

- **提交面板新增两个按钮**。**Generate Commit Message with Requirements** 用 `ui.input-multiline` 收集本次的业务背景、Bug 说明、issue 或 Skip CI 要求，只进入这一次的 prompt；取消或留空不发请求。**Format Commit Message** 把提交框里已有的草稿连同所选 diff 交给模型，要求保留事实、issue 号和语言并重写为规范字段；草稿为空时直接提示。两个命令各有自己的图标（API 1.4 的 `contributes.commands.icon`）。
- **最近提交风格参考**：`Project::vcs_log(None, 5)` 读取最近五条标题写入 prompt，明确标注为风格提示；不使用 `vcs_diff()`，不读取历史正文或 diff。受限项目或读取失败时省略该段落，生成照常进行。
- **模型列表与请求地址**：LLM Settings 页新增 **Models**（`GET {base}/models` → 选择 → 把 id 复制到剪贴板）和 **Request URL**（显示按 Base URL 和协议拼出的实际地址）。宿主的声明式设置对插件只读，所以模型 id 需要粘贴到 Model 字段；该字段始终可手动填写。新增可选 `clipboard` 能力，未授权时通知里直接显示 id。
- **参数兼容与共享预算**：模型以 4xx 明确拒绝 `max_tokens` 并要求 `max_completion_tokens` 时改用后者；明确拒绝 `temperature` 时省略它。新增 **Send temperature** 设置，可一开始就省略。首次请求和最多两次重试共享 50 秒预算（宿主任务上限 60 秒），剩余不足 8 秒即停止；密钥错误、5xx 等不重试。请求超时不再固定 55 秒，随预算剩余递减。
- **请求诊断**：成功通知和 **Test** 结果都列出实际请求地址、token 字段及其值、温度、尝试次数和已做的兼容调整。Test 失败仍由设置页在按钮旁内联显示。
- `cargo test` 从 27 个增加到 51 个，覆盖三种 prompt、历史标题与降级、模型列表、两种兼容重试、配置省略温度、模型选择取消、地址预览、诊断摘要以及原有的协议/解析/写回保护回归。

界面与真实端点的行为尚未实测：以上由 SDK mock host 的行为测试和 0.1.40 CLI 的包结构校验覆盖。

## 0.5.3

- The plugin icon moved from the repository root to `icons/` (`icon = "icons/icon.svg"`, `icon-dark = "icons/icon_dark.svg"`), the layout OpenIDEA's own plugins use. The IDE still shows the same glyph in Settings › Plugins and on the Commit-panel button; nothing about the plugin changes. The move stops other tools from reading the icon as this repository's logo: Paseo derives a project's sidebar icon from a known filename set (`icon.svg`, `favicon.svg`, `logo.png`, …) in the project root and showed the sparkles instead of the first-letter placeholder.

## 0.5.2

- **Max response tokens** now defaults to 64000 (was 4096): enough for a reasoning model to finish thinking and still write the answer, and within what Anthropic models accept. The setting still takes 256‑128000, so a gateway that allows more can be raised.
- The **Test** action no longer fails on reasoning models. The probe sends `max_tokens: 16`, which a reasoning model spends on `reasoning_content`, leaving `content: null` and `finish_reason: "length"`; a 2xx answer whose body parses as JSON now reports `HTTP 200 · model` instead of “did not contain any text”. A non-JSON body is still a failed test.
- The “no API key” message (the notification and the Test/generate errors) now names where the key is: **Settings › Plugins › Installed › select Git Commit Message Helper › Secrets**. The key is not a field on the Commit, Prompts or LLM Settings pages; it is a plugin secret in the system keychain.

## 0.5.1

- Diff trimming keeps both sides of a large replacement: a hunk now keeps up to 12 added and 12 removed lines (a shared 24-line budget could drop every addition when many lines were removed), and the room for the "omitted" marker is reserved before the content, so the summary is always marked and never exceeds `maxDiffChars`.
- The model's answer is validated before it is written: the type must be one of the configured allowed types and contain no whitespace, control character, parenthesis or colon; type, scope and subject must not contain control characters; a wrong JSON value (a number or array where text belongs) is treated as a bad response instead of being stringified. `body` remains the only field that accepts an array.
- An empty or unparsable answer can no longer reach the message field: it is reported as "bad response" and the progress placeholder is cleared.

## 0.5.0

- Published under the publisher id `qszfy666` (repository renamed to `qszfy666/openidea-git-commit-message-helper`): the plugin is `qszfy666.commit-message-helper`; package `qszfy666.commit-message-helper-0.5.0.oiplugin`. Earlier ids were never listed in the marketplace.

## 0.4.0

- Published under the publisher id `vincent66666`: the plugin is `vincent66666.commit-message-helper` (earlier ids `com.liuzhiqiang.*` and `liuzhiqiang.*` were never listed, so no one has to migrate — remove an old local install and install this one).
- Signed package, built from this repository alone against the SDK at tag `v0.1.39`.

## 0.3.0

- Published to the OpenIDEA community marketplace: the plugin id is `liuzhiqiang.commit-message-helper` (was `com.liuzhiqiang.commit-message-helper`, so existing local installs are a different plugin and can be removed), and packages are signed with the publisher key.
- The SDK comes from `https://github.com/l0ng-ai/openidea` at tag `v0.1.39` instead of a local path, so the package builds from this repository alone.
- README documents why each capability is needed (the registry reviews that).

## 0.2.5

- The button's static icon is the four-pointed sparkles again. The ring with a centre dot is the *running* state in other Git clients, and plugin API 1.3 gives a command one icon for all states: the glyph comes from the manifest's `icon`, and `commit.message` slot items are rendered as a plain icon button. A spinner on the button would need an IDE change, not a plugin one.
- Progress while generating stays as the message-field placeholder plus the status-bar task.

## 0.2.4

- New button icon: a ring with a centre dot (`icon.svg` / `icon_dark.svg`), the glyph other Git clients use for the commit-message action, replacing the four-pointed sparkles.

## 0.2.3

- The Commit panel now shows that generation is running: when the message field is empty, the command writes `Generating commit message…` there and replaces it with the result. A field you already typed in is left alone, text typed during generation still wins (revision check), and a failed or cancelled run clears the placeholder again.
- Plugin API 1.3 has no slot next to the button for a spinner, and no placeholder/hint API for the message field, so this is the feedback the panel can show; the background task in the status bar stays.

## 0.2.2

- The plugin ships its own icon (`icon.svg` / `icon_dark.svg`): the Commit panel button and Settings › Plugins show it instead of the generic plug. Plugin API 1.3 renders a command's `icon` only from API 1.4, so the plugin icon is how a 1.3 plugin gets a glyph.
- The prompt's analysis instructions no longer mention recent commits, which 0.2.1 stopped providing.

## 0.2.1

- Target plugin API 1.3 (`engines.openidea = "^1.3"`) so the package installs on released OpenIDEA 0.1.39; API 1.4 exists only on main.
- The prompt no longer carries the last five commit subjects: plugin API 1.3 has no `vcs.log`.
- Commit message language `auto` follows the IDE UI language instead of the recent commit subjects.

## 0.2.0

- Generate directly from OpenIDEA's native Commit tool window and Commit dialog: the command appears in the commit message toolbar.
- Read exactly the files and changed blocks included in the pending commit through `host::pending_commit()`, including partial hunk selections.
- Write the generated message into the native commit message field through `host::set_commit_message()`; the revision check preserves text typed while the model is answering.
- Removed the plugin's own tool window, file checklist, clipboard copy flow and `process:git` capability.
- Multi-page plugin settings (Commit / Prompts / LLM Settings) with an editable commit-type list and an LLM connection Test action.
- Requires OpenIDEA 0.1.39 or newer (plugin API 1.4).

## 0.1.0

- First release.
- Tool window lists staged, unstaged and untracked files; selection is remembered per repository.
- Generate a Conventional Commits message from the selected changes with an LLM; preview, edit and copy it.
- OpenAI Chat Completions, OpenAI Responses and Anthropic Messages protocols.
- Provider presets for the common public services plus Magpie/Ollama on localhost.
- API key kept in the system keychain; Chinese/English UI.
