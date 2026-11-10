# Changelog

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
