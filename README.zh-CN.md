# Git Commit Message Helper（OpenIDEA 插件）

在你正在准备的提交上，用 LLM 生成规范、清晰的 **Conventional Commits** 提交信息——直接在 OpenIDEA 的原生提交面板里完成。

这是 OpenIDEA 的 WebAssembly 插件（Rust SDK，插件 API 1.4），产品行为参考 IntelliJ 插件 [git-commit-message-helper](https://github.com/AutismSuperman/git-commit-message-helper)。

## 功能

- 在 Commit 工具窗和 Commit 对话框的提交信息工具栏中增加三个按钮（`commit.message`）：**Generate Commit Message**、**Generate Commit Message with Requirements**、**Format Commit Message**。
- **With Requirements** 弹出多行输入框，填写本次的业务背景、Bug 说明、issue 引用或 Skip CI 要求，只对这一次生成生效。**Format Commit Message** 把提交框里已有的草稿重写成规范字段，保留原有事实、issue 号和语言。
- 通过 `host::pending_commit()` 读取本次提交实际包含的文件和代码块（包括部分 hunk 选择）；读取前会像 Commit 一样保存相关文件的未保存编辑器。
- 把这些文件的状态和实际包含的 diff（按 hunk 裁剪并受字符预算限制）发送给 LLM，另附 `Project::vcs_log()` 取到的最近五条提交标题作为风格参考。历史只作风格提示：不使用工作区的 `vcs_diff()`，生成内容仍以所选变更为准。
- 模型返回结构化字段（`type`、`scope`、`subject`、`body`、`changes`、`closes`、`skipCi`）；插件在本地渲染成最终提交信息，保证格式一致。
- 配置可复用：多组模型档案（各有自己的输出 token 上限、密钥槽和思考设置）、写作偏好档案和输出模板档案可随时切换；项目可以选择自己的默认档案，未配置时继承全局选择。
- 按模型配置思考参数：选择等级和预设（或自定义 JSON 映射），插件会校验映射、按响应 token 上限解析 `thinking.budget_tokens`，并在诊断里显示实际发送的内容。默认不发送思考参数，已有请求形态不变。
- 输出模板可先预览再保存：选择哪些示例字段有值，在对话框里编辑模板即可看到渲染结果；**Variables** 说明语法，**Restore Default** 复制内置模板。
- 点击后能看出正在执行：提交信息框为空时会先写入「正在生成提交信息…」，生成完再替换成结果（你已经输入过内容的框不会被动），状态栏也有进度任务。
- 通过 `host::set_commit_message()` 把生成结果写回原生提交信息框。如果生成期间你修改了提交信息或勾选内容，版本号校验会拒绝写入，当前文本不会被覆盖。
- 模型明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或 `temperature` 时，只去掉该参数再重试一次；所有尝试共享 50 秒预算，处于宿主 60 秒后台任务窗口内。**Send temperature** 设置也可以一开始就省略温度。
- 成功通知与 **Test** 结果都会列出实际发送的请求地址、token 字段、温度、尝试次数和兼容调整。
- 在提交面板里检查后直接提交。命令也可以在 **Find Action** 和快捷键（`⌥⌘G` / `Ctrl+Alt+G`）中使用。

## 环境要求

- OpenIDEA 0.1.40 或更新（插件 API 1.4）。
- 一个 OpenAI 兼容 / OpenAI Responses / Anthropic Messages 端点。API Key 存在系统钥匙串，是四个预声明的插件 secret（`api-key`、`api-key-2`、`api-key-3`、`api-key-4`），不写入设置或插件存储。

不需要 `git` 命令：待提交内容和 diff 由 IDE 提供。

## 安装

**Settings › Plugins › Available** 里搜索 **Git Commit Message Helper**（市场 id：`qszfy666.commit-message-helper`，通过[社区 registry](https://github.com/l0ng-ai/openidea-plugins) 收录），安装并按提示授予能力。

离线时自己构建：

```sh
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin pack . --out dist/qszfy666.commit-message-helper-0.7.1.oiplugin   # 未签名，本地安装用
```

然后：

- **Settings › Plugins › Install Plugin from Disk…** 选择那个 `.oiplugin` 文件或本目录；或
- 开发时 `openidea-plugin link /Users/liuzhiqiang/WorkProjects/Rust/openidea-plugin/git-commit-message-helper`。

打开 **Settings › Plugins** 加载变更，插件名为 **Git Commit Message Helper**。

## 配置

**Settings › Plugins › Git Commit Message Helper** 现在有四页：

| 页面 | 设置 |
| --- | --- |
| Commit | 提交信息语言（`auto` 跟随 IDE 界面语言，或指定中文/英文）、diff 字符上限、允许的提交类型（「标题＋描述」表格，描述会写进 prompt）。 |
| Prompts | 额外的系统提示词、提示词档案表格与 **Default prompt profile**。 |
| Commit Templates | 输出模板档案表格、**Default template**，以及三个动作：**Preview**（用所选示例字段渲染编辑中的模板）、**Variables**（变量说明、示例与错误）、**Restore Default**（复制内置模板）。 |
| LLM Settings | Provider 预设、API 协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens、Thinking 组（等级、预设、自定义映射）、模型档案表格与 **Active model profile**；以及五个页面动作：**Test**、**Models**、**Request URL**、**Thinking Levels**（映射实际提供的等级）、**Thinking Help**（映射规则与预设模板）。五个动作都会先应用待保存设置。 |

API Key 是插件 secret，不在这四页里：**Settings › Plugins › 已安装里选中 Git Commit Message Helper › 右侧「密钥」› LLM API key › 保存**。共四个密钥槽（`api-key`、`api-key-2`、`api-key-3`、`api-key-4`），模型档案在 **Key slot** 列选择其中一个。它们存在系统钥匙串，不写入设置文件。

**Models** 调用 `GET {base}/models`，选中后把模型 id 复制到剪贴板。宿主的声明式设置对插件是只读的，因此请粘贴到 **Model**；该字段始终可以手动填写。**Request URL** 使用与生成完全相同的 Base URL／协议规则，显示下一次请求真正会发往的地址。

### 命名档案、模板与项目默认

三个声明式表格可以保存多组配置。宿主的声明式设置对插件只读，所以档案的新建、改名、编辑、复制、删除和排序都在 IDE 设置界面完成。

| 表格 | 页面 | 选择项 | 未选择时 |
| --- | --- | --- | --- |
| **Model profiles** | LLM Settings | **Active model profile** | 表格上方的单份 LLM 设置 |
| **Prompt profiles** | Prompts | **Default prompt profile** | 单份 **Extra system prompt** |
| **Commit templates** | Commit Templates | **Default template** | 内置 **Default** 模板 |

模型档案独立保存 Provider、协议、Base URL、Model、Temperature、**Max response tokens** 和密钥槽；三个页面动作（Test / Models / Request URL）读取的也是活动档案。选择项留空时继续使用 0.6.x 的单份设置，已有配置无需迁移。

输出模板用变量 `{{type}}`、`{{scope}}`、`{{subject}}`、`{{body}}`、`{{changes}}`、`{{closes}}`、`{{skipCi}}` 和 `{{header}}`（即 `type(scope): subject`）控制最终文本；条件段 `{{#body}}…{{/body}}` 只在该字段非空时输出，因此字段顺序、分隔符和可选段落都由模板决定；变量之外的文本原样保留，footer 就是最后一行。例如下面这个模板保留标题，仅在有正文时输出正文段，并总是追加 footer（模板里的空行就是真实换行）：

```text
{{header}}{{#body}}

{{body}}{{/body}}

Signed-off-by: Team
```

内置 **Default** 始终存在，同名表格行无法覆盖它。不支持嵌套条件段、循环和表达式；模板无效时回退内置 Default，并在通知里说明原因。完整 Velocity 兼容单独评估。

三个选择项都是普通设置键，宿主按 project over global 解析：项目层有值就用项目值，项目里留空即继承全局选择；档案表格本身也按同一规则分层，两层档案库不合并。

### 模板预览与帮助

Commit Templates 页有三个动作。**Preview** 先用多选决定哪些示例字段有值，再打开预填活动模板的多行输入框：确认即看到渲染结果，可反复修改，取消结束；模板错误会直接显示，不必等生成时才回退。**Variables** 列出八个变量、条件段、示例、错误类型和恢复入口。**Restore Default** 把内置 Default 模板复制到剪贴板（剪贴板未授权时在通知里显示），粘贴到 Template 列或清空 **Default template** 即可恢复。声明式设置对插件只读，也没有表格编辑事件，所以预览是「编辑 → 预览 → 再编辑」的命令循环，不是设置页内的随改随刷。

### 思考参数

LLM Settings 的 Thinking 组（模型档案表格也有同三列）包含 **Thinking level**、**Thinking preset** 和 **Thinking mapping**。等级为 `DEFAULT`、`DISABLED`、`ENABLED`、`LOW`、`MEDIUM`、`HIGH`、`MAX`；预设为 `model-default`（不发送）、`auto`（按档案协议选标准字段）、`openai-effort`、`nested-effort`、`thinking-object`、`enable-thinking`、`anthropic-budget`、`anthropic-adaptive` 和 `custom`。默认值保持 0.7.0 的请求体不变。

映射是按等级名作键的 JSON 对象，值为请求片段，例如 `{"HIGH":{"reasoning_effort":"high"}}`；填写后覆盖预设模板，清空即恢复。插件拒绝非法 JSON、未知等级、隐式 `DEFAULT`、空片段、null、保留请求字段（`model`、`messages`、`temperature`、各 token 字段等）、`thinking.budget_tokens` 之外的 `$thinking_budget`、超过 12 层嵌套和超过 64 KiB 的映射。`$thinking_budget` 按等级占 **Max response tokens** 的 25/50/75/90%，不小于 1024、小于上限，并为答案预留 256 token；显式数字同样校验。

声明式 enum 列无法条件化，**Thinking Levels** 显示映射实际提供的等级；等级没有映射、映射无效或预算不可用时按 Model default 发送，请求诊断写明原因。**Thinking Help** 说明格式、显示当前校验结果并复制预设模板。服务端以 4xx 明确拒绝思考字段时直接报告，不重试（思考参数的兼容重试与缓存不在本批）。

插件声明的可选网络能力（按需授权）：

`api.openai.com`、`api.anthropic.com`、`api.deepseek.com`、`dashscope.aliyuncs.com`、`api.moonshot.cn`、`open.bigmodel.cn`、`api.minimax.chat`、`api.siliconflow.cn`、`ark.cn-beijing.volces.com`、`*.openai.azure.com`、`openrouter.ai`、`api.groq.com`、`api.mistral.ai`、`generativelanguage.googleapis.com`、`127.0.0.1`（Magpie、本地服务）、`localhost`（Ollama）。`clipboard`（可选）用于复制选中的模型 id、内置模板和预设思考映射。

Magpie 示例：Provider 选 **Magpie**，Base URL 填 `http://127.0.0.1:3425`，Model 填 Magpie 里对应的模型名。IDE 只允许对 loopback 使用明文 HTTP。

### 为什么需要这些能力

| 能力 | 用途 |
| --- | --- |
| `vcs.read` | 读取正在准备的提交（勾选的文件及其 diff）、仓库根/分支，以及最近五条提交标题作为风格参考；不读其他内容。 |
| `vcs.write` | 把生成结果写进提交信息框（`set_commit_message`）。插件不提交、不暂存、不 fetch、不 push。 |
| `secrets` | API Key 存在系统钥匙串，由 IDE 在发请求时填入；插件代码读不到值。 |
| `net:<host>`（可选） | 把 prompt、模型列表请求和连接测试发给你配置的 LLM 端点。每个预设服务商一个可选主机，只授你实际用的那个；`127.0.0.1`/`localhost` 给本地服务（Magpie、Ollama）。 |
| `clipboard`（可选） | 复制 **Models** 中选中的模型 id、**Restore Default** 的内置模板和 **Thinking Help** 的预设映射。未授权时会在通知里显示文本。 |

不申请 `process:`、`fs:`、`ide.commands`，也没有遥测。

## 支持的协议

- **OpenAI Chat Completions** — `POST {base}/chat/completions`，`Authorization: Bearer`。
- **OpenAI Responses** — `POST {base}/responses`，`instructions` + `input`。
- **Anthropic Messages** — `POST {base}/v1/messages`，`x-api-key` + `anthropic-version`。

Base URL 补全规则：无路径的 URL 补 `/v1`；已有路径的直接追加协议后缀（`/chat/completions`、`/responses`、`/messages`）；已带后缀的原样使用。

Chat Completions 发送 `max_tokens`，Responses 发送 `max_output_tokens`，Anthropic 发送 `max_tokens`。模型返回 4xx 明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或拒绝 `temperature` 时，插件只去掉该参数重试；其他失败（密钥错误、服务端错误）直接报告，不重试。最多三次尝试，共享 50 秒预算。校验后的思考片段会合并进同一个请求体；服务端明确拒绝思考字段时直接报告，不重试。

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

- `src/config.rs` — 设置、服务商预设、命名档案解析与项目默认选择。
- `src/git.rs` — 待提交内容上下文和 diff 裁剪。
- `src/prompt.rs` — prompt 构建、JSON 解析、模板渲染（内置 Default 与命名模板）、预览示例字段。
- `src/thinking.rs` — 思考等级、预设映射、校验、可用等级与 token budget。
- `src/llm.rs` — 三种协议客户端（`host::fetch`）、密钥槽、模型列表、兼容重试、思考片段、请求诊断与连接测试。
- `src/lib.rs` — 插件命令；`plugin.toml` — manifest、能力、贡献点。

## 许可证

Apache-2.0，见 [LICENSE](LICENSE)。
