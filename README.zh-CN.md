# Git Commit Message Helper（OpenIDEA 插件）

在你正在准备的提交上，用 LLM 生成规范、清晰的 **Conventional Commits** 提交信息——直接在 OpenIDEA 的原生提交面板里完成。

这是 OpenIDEA 的 WebAssembly 插件（Rust SDK，插件 API 1.4），产品行为参考 IntelliJ 插件 [git-commit-message-helper](https://github.com/AutismSuperman/git-commit-message-helper)。

## 功能

- 在 Commit 工具窗和 Commit 对话框的提交信息工具栏中增加三个按钮（`commit.message`）：**Generate Commit Message**、**Generate Commit Message with Requirements**、**Format Commit Message**。
- **With Requirements** 弹出多行输入框，填写本次的业务背景、Bug 说明、issue 引用或 Skip CI 要求，只对这一次生成生效。**Format Commit Message** 把提交框里已有的草稿重写成规范字段，保留原有事实、issue 号和语言。
- 通过 `host::pending_commit()` 读取本次提交实际包含的文件和代码块（包括部分 hunk 选择）；读取前会像 Commit 一样保存相关文件的未保存编辑器。
- 把这些文件的状态和实际包含的 diff（按 hunk 裁剪并受字符预算限制）发送给 LLM，另附 `Project::vcs_log()` 取到的最近五条提交标题作为风格参考。历史只作风格提示：不使用工作区的 `vcs_diff()`，生成内容仍以所选变更为准。
- 模型返回结构化字段（`type`、`scope`、`subject`、`body`、`changes`、`closes`、`skipCi`）；插件在本地渲染成最终提交信息，保证格式一致。
- 点击后能看出正在执行：提交信息框为空时会先写入「正在生成提交信息…」，生成完再替换成结果（你已经输入过内容的框不会被动），状态栏也有进度任务。
- 通过 `host::set_commit_message()` 把生成结果写回原生提交信息框。如果生成期间你修改了提交信息或勾选内容，版本号校验会拒绝写入，当前文本不会被覆盖。
- 模型明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或 `temperature` 时，只去掉该参数再重试一次；所有尝试共享 50 秒预算，处于宿主 60 秒后台任务窗口内。**Send temperature** 设置也可以一开始就省略温度。
- 成功通知与 **Test** 结果都会列出实际发送的请求地址、token 字段、温度、尝试次数和兼容调整。
- 在提交面板里检查后直接提交。命令也可以在 **Find Action** 和快捷键（`⌥⌘G` / `Ctrl+Alt+G`）中使用。

## 环境要求

- OpenIDEA 0.1.40 或更新（插件 API 1.4）。
- 一个 OpenAI 兼容 / OpenAI Responses / Anthropic Messages 端点。API Key 存在系统钥匙串（插件 secret `api-key`），不写入设置或插件存储。

不需要 `git` 命令：待提交内容和 diff 由 IDE 提供。

## 安装

**Settings › Plugins › Available** 里搜索 **Git Commit Message Helper**（市场 id：`qszfy666.commit-message-helper`，通过[社区 registry](https://github.com/l0ng-ai/openidea-plugins) 收录），安装并按提示授予能力。

离线时自己构建：

```sh
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin pack . --out dist/qszfy666.commit-message-helper-0.6.0.oiplugin   # 未签名，本地安装用
```

然后：

- **Settings › Plugins › Install Plugin from Disk…** 选择那个 `.oiplugin` 文件或本目录；或
- 开发时 `openidea-plugin link /Users/liuzhiqiang/WorkProjects/Rust/openidea-plugin/git-commit-message-helper`。

打开 **Settings › Plugins** 加载变更，插件名为 **Git Commit Message Helper**。

## 配置

**Settings › Plugins › Git Commit Message Helper** 现在有三页：

| 页面 | 设置 |
| --- | --- |
| Commit | 提交信息语言（`auto` 跟随 IDE 界面语言，或指定中文/英文）、diff 字符上限、允许的提交类型（可增删的列表）。 |
| Prompts | 额外的系统提示词，用于长期偏好。 |
| LLM Settings | Provider 预设、API 协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens；以及三个页面动作：**Test**（最小请求，并给出请求诊断）、**Models**（列出端点的模型 id）、**Request URL**（显示生成请求的实际地址）。三个动作都会先应用待保存设置。 |

API Key 是插件 secret，不在这三个页面里：**Settings › Plugins › 已安装 里选中 Git Commit Message Helper › 右侧「密钥」› LLM API key › 保存**。它存在系统钥匙串，不写入设置文件。

**Models** 调用 `GET {base}/models`，选中后把模型 id 复制到剪贴板。宿主的声明式设置对插件是只读的，因此请粘贴到 **Model**；该字段始终可以手动填写。**Request URL** 使用与生成完全相同的 Base URL／协议规则，显示下一次请求真正会发往的地址。

插件声明的可选网络能力（按需授权）：

`api.openai.com`、`api.anthropic.com`、`api.deepseek.com`、`dashscope.aliyuncs.com`、`api.moonshot.cn`、`open.bigmodel.cn`、`api.minimax.chat`、`api.siliconflow.cn`、`ark.cn-beijing.volces.com`、`*.openai.azure.com`、`openrouter.ai`、`api.groq.com`、`api.mistral.ai`、`generativelanguage.googleapis.com`、`127.0.0.1`（Magpie、本地服务）、`localhost`（Ollama）。`clipboard`（可选）仅用于复制选中的模型 id。

Magpie 示例：Provider 选 **Magpie**，Base URL 填 `http://127.0.0.1:3425`，Model 填 Magpie 里对应的模型名。IDE 只允许对 loopback 使用明文 HTTP。

### 为什么需要这些能力

| 能力 | 用途 |
| --- | --- |
| `vcs.read` | 读取正在准备的提交（勾选的文件及其 diff）、仓库根/分支，以及最近五条提交标题作为风格参考；不读其他内容。 |
| `vcs.write` | 把生成结果写进提交信息框（`set_commit_message`）。插件不提交、不暂存、不 fetch、不 push。 |
| `secrets` | API Key 存在系统钥匙串，由 IDE 在发请求时填入；插件代码读不到值。 |
| `net:<host>`（可选） | 把 prompt、模型列表请求和连接测试发给你配置的 LLM 端点。每个预设服务商一个可选主机，只授你实际用的那个；`127.0.0.1`/`localhost` 给本地服务（Magpie、Ollama）。 |
| `clipboard`（可选） | 复制 **Models** 中选中的模型 id。未授权时会在通知里显示该 id。 |

不申请 `process:`、`fs:`、`ide.commands`，也没有遥测。

## 支持的协议

- **OpenAI Chat Completions** — `POST {base}/chat/completions`，`Authorization: Bearer`。
- **OpenAI Responses** — `POST {base}/responses`，`instructions` + `input`。
- **Anthropic Messages** — `POST {base}/v1/messages`，`x-api-key` + `anthropic-version`。

Base URL 补全规则：无路径的 URL 补 `/v1`；已有路径的直接追加协议后缀（`/chat/completions`、`/responses`、`/messages`）；已带后缀的原样使用。

Chat Completions 发送 `max_tokens`，Responses 发送 `max_output_tokens`，Anthropic 发送 `max_tokens`。模型返回 4xx 明确拒绝 `max_tokens`（要求改用 `max_completion_tokens`）或拒绝 `temperature` 时，插件只去掉该参数重试；其他失败（密钥错误、服务端错误）直接报告，不重试。最多三次尝试，共享 50 秒预算。

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

- `src/config.rs` — 设置与服务商预设。
- `src/git.rs` — 待提交内容上下文和 diff 裁剪。
- `src/prompt.rs` — prompt 构建、JSON 解析、Conventional Commits 渲染。
- `src/llm.rs` — 三种协议客户端（`host::fetch`）、模型列表、兼容重试、请求诊断与连接测试。
- `src/lib.rs` — 插件命令；`plugin.toml` — manifest、能力、贡献点。

## 许可证

Apache-2.0，见 [LICENSE](LICENSE)。
