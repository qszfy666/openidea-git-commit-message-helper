# Git Commit Message Helper（OpenIDEA 插件）

在你正在准备的提交上，用 LLM 生成规范、清晰的 **Conventional Commits** 提交信息——直接在 OpenIDEA 的原生提交面板里完成。

这是 OpenIDEA 的 WebAssembly 插件（Rust SDK，插件 API 1.5），产品行为参考 IntelliJ 插件 [git-commit-message-helper](https://github.com/AutismSuperman/git-commit-message-helper)。

## 功能

- 在 Commit 工具窗和 Commit 对话框的提交信息工具栏中增加两个按钮（`commit.message`）：**Generate Commit Message** 和 **Generate Commit Message with Requirements**。每个按钮都可以在设置里单独隐藏。
- **With Requirements** 弹出多行输入框，填写本次的业务背景、Bug 说明、issue 引用或 Skip CI 要求，只对这一次生成生效。
- 通过 `host::pending_commit()` 读取本次提交实际包含的文件和代码块（包括部分 hunk 选择）；读取前会像 Commit 一样保存相关文件的未保存编辑器。
- 把这些文件的状态和实际包含的 diff（按 hunk 裁剪并受字符预算限制）发送给 LLM，另附 `Project::vcs_log()` 取到的最近五条提交标题作为风格参考。历史只作风格提示：不使用工作区的 `vcs_diff()`，生成内容仍以所选变更为准。
- 模型返回结构化字段（`type`、`scope`、`subject`、`body`、`changes`、`closes`、`skipCi`）；插件在本地渲染成最终提交信息，保证格式一致。
- 命名输出模板：可以保存多个模板并选择默认模板；项目可以选自己的，未配置时继承全局选择。
- 按模型配置思考参数：选择等级和预设（或自定义 JSON 映射），插件会校验映射、按响应 token 上限解析 `thinking.budget_tokens`，并在诊断里显示实际发送的内容。默认不发送思考参数，已有请求形态不变。
- 输出模板帮助：**Variables** 说明语法，**Restore Default** 复制内置模板。
- 点击后能看出正在执行：提交信息框会先换成「正在生成提交信息…」，生成完再替换成结果，状态栏也有进度任务。生成失败或取消时恢复框里原来的文字；生成期间你自己输入的内容不会被覆盖。
- 通过 `host::set_commit_message()` 把生成结果写回原生提交信息框。如果生成期间你修改了提交信息或勾选内容，版本号校验会拒绝写入，当前文本不会被覆盖。
- 模型明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或 `temperature` 时，只去掉该参数再重试一次；所有尝试共享 50 秒预算，处于宿主 60 秒后台任务窗口内。**Send temperature** 设置也可以一开始就省略温度。
- 成功通知与 **Test** 结果都会列出实际发送的请求地址、token 字段、温度、尝试次数、兼容调整，以及是否命中兼容缓存。
- 生成后会在本地检查标题（空泛、仅是文件名、不足十二列），命中只在通知里列出，不改写结果。
- 在提交面板里检查后直接提交。命令也可以在 **Find Action** 和快捷键（`⌥⌘G` / `Ctrl+Alt+G`）中使用。

## 环境要求

- OpenIDEA 0.1.41 或更新（插件 API 1.5）。
- 一个 OpenAI 兼容 / OpenAI Responses / Anthropic Messages 端点。API Key 存在系统钥匙串，是插件 secret `api-key`，不写入设置或插件存储。

不需要 `git` 命令：待提交内容和 diff 由 IDE 提供。

从旧版本升级时保留提交类型，以及表格里的 **Duplicate** 操作，需要应用随仓库提供的 OpenIDEA 宿主补丁。应用与验证说明见[迁移清单的验收修复记录](docs/migration-checklist.md)。

## 安装

**Settings › Plugins › Available** 里搜索 **Git Commit Message Helper**（市场 id：`qszfy666.commit-message-helper`，通过[社区 registry](https://github.com/l0ng-ai/openidea-plugins) 收录），安装并按提示授予能力。

离线时自己构建：

```sh
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin pack . --out dist/qszfy666.commit-message-helper-0.9.0.oiplugin   # 未签名，本地安装用
```

然后：

- **Settings › Plugins › Install Plugin from Disk…** 选择那个 `.oiplugin` 文件或本目录；或
- 开发时 `openidea-plugin link /Users/liuzhiqiang/WorkProjects/Rust/openidea-plugin/git-commit-message-helper`。

打开 **Settings › Plugins** 加载变更，插件名为 **Git Commit Message Helper**。

## 配置

**Settings › Plugins › Git Commit Message Helper** 现在有四页：

| 页面 | 设置 |
| --- | --- |
| Commit | 提交信息语言（可填任意语言名称，`auto` 跟随 IDE 界面语言）、diff 字符上限、允许的提交类型（「标题＋描述」表格，描述会写进 prompt），以及提交面板显示哪几个按钮。 |
| Prompts | 额外的系统提示词，写固定的写作偏好。 |
| Commit Templates | 输出模板表格、**Default template**，以及两个动作：**Variables**（变量说明、示例与错误）、**Restore Default**（复制内置模板）。 |
| LLM Settings | Provider 预设、API 协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens、Thinking 组（等级、预设、自定义映射）；以及 **Test** 动作：先应用待保存设置，再发一个很短的请求。失败原因显示在按钮下方，成功结果在通知里。 |

API Key 是插件 secret，不在这四页里：**Settings › Plugins › 已安装里选中 Git Commit Message Helper › 右侧「密钥」› LLM API key › 保存**。它存在系统钥匙串，不写入设置文件。

**Model** 需要手动填写模型 id。OpenIDEA 0.1.41 的设置项不支持由插件提供下拉选项，插件也不能写设置，所以这里只能是文本框。

### 模板与项目默认

**Commit templates** 表格可以保存多个输出模板，**Default template** 指定用哪一个；留空或填了不存在的名称时使用内置 **Default**。宿主的声明式设置对插件只读，所以模板的新建、改名、编辑、复制、删除和排序都在 IDE 设置界面完成。

输出模板用变量 `{{type}}`、`{{scope}}`、`{{subject}}`、`{{body}}`、`{{changes}}`、`{{closes}}`、`{{skipCi}}` 和 `{{header}}`（即 `type(scope): subject`）控制最终文本；条件段 `{{#body}}…{{/body}}` 只在该字段非空时输出，因此字段顺序、分隔符和可选段落都由模板决定；变量之外的文本原样保留，footer 就是最后一行。例如下面这个模板保留标题，仅在有正文时输出正文段，并总是追加 footer（模板里的空行就是真实换行）：

```text
{{header}}{{#body}}

{{body}}{{/body}}

Signed-off-by: Team
```

内置 **Default** 始终存在，同名表格行无法覆盖它。不支持嵌套条件段、循环和表达式；模板无效时回退内置 Default，并在通知里说明原因。完整 Velocity 兼容单独评估。

**Default template** 是普通设置键，宿主按 project over global 解析：项目层有值就用项目值，项目里留空即继承全局选择；模板表格本身也按同一规则分层，两层不合并。

### 模板帮助

Commit Templates 页有两个动作。**Variables** 列出八个变量、条件段、示例、错误类型和恢复入口。**Restore Default** 把内置 Default 模板复制到剪贴板（剪贴板未授权时在通知里显示），粘贴到 Template 列或清空 **Default template** 即可恢复。两个动作的结果都在项目主窗口的「通知」面板里。模板有错误时在生成时报告：回退内置 Default，并在通知里说明原因。

### 思考参数

LLM Settings 的 Thinking 组包含 **Thinking level**、**Thinking preset** 和 **Thinking mapping**。等级为 `DEFAULT`、`DISABLED`、`ENABLED`、`LOW`、`MEDIUM`、`HIGH`、`MAX`；预设为 `model-default`（不发送）、`auto`（按所配协议选标准字段）、`openai-effort`、`nested-effort`、`thinking-object`、`enable-thinking`、`anthropic-budget`、`anthropic-adaptive` 和 `custom`。默认值保持 0.7.0 的请求体不变。

映射是按等级名作键的 JSON 对象，值为请求片段，例如 `{"HIGH":{"reasoning_effort":"high"}}`；填写后覆盖预设模板，清空即恢复。插件拒绝非法 JSON、未知等级、隐式 `DEFAULT`、空片段、null、保留请求字段（`model`、`messages`、`temperature`、各 token 字段等）、`thinking.budget_tokens` 之外的 `$thinking_budget`、超过 12 层嵌套和超过 64 KiB 的映射。`$thinking_budget` 按等级占 **Max response tokens** 的 25/50/75/90%，不小于 1024、小于上限，并为答案预留 256 token；显式数字同样校验。

等级没有映射、映射无效或预算不可用时按 Model default 发送，请求诊断写明原因。服务端以 4xx 明确拒绝思考字段时直接报告，不重试（思考参数的兼容重试与缓存不在本批）。

插件声明的可选网络能力（按需授权）：

`api.openai.com`、`api.anthropic.com`、`api.deepseek.com`、`dashscope.aliyuncs.com`、`api.moonshot.cn`、`open.bigmodel.cn`、`api.minimax.chat`、`api.siliconflow.cn`、`ark.cn-beijing.volces.com`、`*.openai.azure.com`、`openrouter.ai`、`api.groq.com`、`api.mistral.ai`、`generativelanguage.googleapis.com`、`127.0.0.1`（Magpie、本地服务）、`localhost`（Ollama）。`net:ask`（可选）用于其他 HTTPS 主机上的 Base URL：命令第一次连接某个主机时，IDE 会弹框问是否允许。后台调用不会弹框。对 loopback 以外主机的明文 HTTP 仍然被拒绝。`clipboard`（可选）用于复制选中的模型 id、内置模板和预设思考映射。

Magpie 示例：Provider 选 **Magpie**，Base URL 填 `http://127.0.0.1:3425`，Model 填 Magpie 里对应的模型名。IDE 只允许对 loopback 使用明文 HTTP。

### 按钮与界面语言

Commit 页的 **Buttons** 组决定提交信息上方两个按钮的显隐：生成、带附加要求生成。隐藏的按钮会从工具栏移除（命令仍可从 Find Action 运行）。插件在启动时和设置变化时把两个选择写成 context key。

提交信息语言可以填任意语言名称，例如 `English`、`Simplified Chinese`、`Japanese`、`한국어` 或 `繁體中文`；`auto` 或留空跟随 IDE 界面语言。插件自己的文案（通知、对话框）也跟随该语言：内置英文、简体中文、日文和韩文，其他语言回退英文。

生成结果写回前还会做一次本地质量检查（空泛标题、标题只是文件名、标题不足十二列），命中只在通知里列出，不改写内容；自动纠正请求属于另一条目。

### 为什么需要这些能力

| 能力 | 用途 |
| --- | --- |
| `vcs.read` | 读取正在准备的提交（勾选的文件及其 diff）、仓库根/分支，以及最近五条提交标题作为风格参考；不读其他内容。 |
| `vcs.write` | 把生成结果写进提交信息框（`set_commit_message`）。插件不提交、不暂存、不 fetch、不 push。 |
| `secrets` | API Key 存在系统钥匙串，由 IDE 在发请求时填入；插件代码读不到值。 |
| `storage` | 记住某个端点拒绝过的请求参数，下次不再重复询问。卸载插件时一并删除。 |
| `events` | 订阅 `settings-changed`，让按钮显隐立刻生效；不订阅其他事件。 |
| `net:<host>`（可选） | 把 prompt 和连接测试发给你配置的 LLM 端点。每个预设服务商一个可选主机，只授你实际用的那个；`127.0.0.1`/`localhost` 给本地服务（Magpie、Ollama）。 |
| `net:ask`（可选） | 连接上面列表以外的 HTTPS 端点。命令第一次连接某个新主机时 IDE 会询问；每个主机单独允许或拒绝，之后也可以撤销。 |
| `clipboard`（可选） | 复制 **Restore Default** 的内置模板。未授权时会在通知里显示模板。 |

不申请 `process:`、`fs:`、`ide.commands`，也没有遥测。

## 支持的协议

- **OpenAI Chat Completions** — `POST {base}/chat/completions`，`Authorization: Bearer`。
- **OpenAI Responses** — `POST {base}/responses`，`instructions` + `input`。
- **Anthropic Messages** — `POST {base}/v1/messages`，`x-api-key` + `anthropic-version`。

Base URL 补全规则：无路径的 URL 补 `/v1`；已有路径的直接追加协议后缀（`/chat/completions`、`/responses`、`/messages`）；已带后缀的原样使用。

Chat Completions 发送 `max_tokens`，Responses 发送 `max_output_tokens`，Anthropic 发送 `max_tokens`。模型返回 4xx 明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或拒绝 `temperature` 时，插件只去掉该参数重试；其他失败（密钥错误、服务端错误）直接报告，不重试。最多三次尝试，共享 50 秒预算。被拒绝的参数按端点、模型、协议、思考映射和温度配置记录在插件存储里，下次直接按已接受的请求形状发送，不再重复试探；修改温度后会重新发送新值，请求诊断会写明命中的缓存。校验后的思考片段会合并进同一个请求体；服务端明确拒绝思考字段时直接报告，不重试。

## 隐私

- 只把本次提交包含文件的状态和 diff，以及仓库最近五条提交标题发送到你配置的端点；不发送所选变更之外的工作区 diff，也不发送其他仓库数据。
- API Key 由 IDE 在发出请求时从系统钥匙串填入，插件代码读不到。
- 无遥测，无其他网络请求。

## 开发

```sh
cargo test                                     # 单元测试，SDK mock host
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip2
```

目录：

- `src/config.rs` — 设置、服务商预设与所选模板。
- `src/git.rs` — 待提交内容上下文和 diff 裁剪。
- `src/prompt.rs` — prompt 构建、JSON 解析、模板渲染（内置 Default 与命名模板）。
- `src/thinking.rs` — 思考等级、预设映射、校验、可用等级与 token budget。
- `src/llm.rs` — 三种协议客户端（`host::fetch`）、兼容重试、思考片段、请求诊断与连接测试。
- `src/lib.rs` — 插件命令；`plugin.toml` — manifest、能力、贡献点。

## 许可证

Apache-2.0，见 [LICENSE](LICENSE)。
