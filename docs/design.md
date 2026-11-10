# 设计说明：OpenIDEA 提交信息生成插件

## 1. 结论先行

OpenIDEA 0.1.40（`openidea:plugin@1.4.0`）已经提供原生提交工作流：

- `[[contributes.menus]] menu = "commit.message"`：在 Commit 工具窗和 Commit 对话框的提交信息上方显示插件命令按钮；API 1.4 允许命令各自声明 `icon`；
- `host::pending_commit()`：返回当前窗口正在准备的提交，包括实际包含的文件、部分 hunk 的 diff、`amend` 和消息草稿；
- `host::set_commit_message(revision, text)`：把生成结果写入原生提交信息框，并按 `revision` 防止覆盖用户后来输入的内容；
- `Project::vcs_log()`、`ui.input-multiline`、`ui.pick` 与 `clipboard`：最近提交风格参考、附加要求输入、模型列表选择和复制。

因此插件不再需要自己的 Tool Window、文件勾选清单、剪贴板复制，也不再需要 `process:git`。插件从「工具窗 + 复制粘贴」改为「原生提交面板按钮 + 一键填入」；工具栏上的三个按钮分别对应生成、带附加要求生成与草稿格式化。

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
              ├─ prompt.rs 三种任务的 prompt + JSON 字段契约 + 本地 Conventional Commit 渲染
              ├─ llm.rs    host::fetch → 三种协议、模型列表、兼容重试、请求诊断
              ├─ config.rs 设置与服务商预设
              ├─ ui.rs     中英文通知与对话框文案
              └─ lib.rs    命令入口：generate / generate-with-context / format / models / endpoint / test
```

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

- **Commit**：提交信息语言（`auto` 跟随 IDE 界面语言）、最大 diff 字符数、允许的提交类型（字符串数组）；
- **Prompts**：额外系统提示词；
- **LLM Settings**：Provider、协议、Base URL、Model、Temperature、**Send temperature**、Max response tokens；页面的 **Test**、**Models**、**Request URL** 三个动作都会先应用待保存设置。**Test** 发起一次最小请求并给出诊断摘要，**Models** 请求 `GET {base}/models` 后选择并把 id 复制到剪贴板（声明式设置对插件只读，用户自行粘贴到 Model），**Request URL** 显示按 Base URL 和协议拼接出的实际请求地址。

网络白名单通过 `plugin.toml` 的 optional `net:` 能力声明；内置 OpenAI、Anthropic、DeepSeek、Qwen、Kimi、GLM、MiniMax、SiliconFlow、Volcengine、Azure OpenAI、OpenRouter、Groq、Mistral、Google Gemini、`127.0.0.1`（Magpie）与 `localhost`（Ollama）。`clipboard` 也是可选能力，只用于复制选中的模型 id。Magpie 示例：`provider = Magpie`、`baseUrl = http://127.0.0.1:3425`、`model = <Magpie 模型 id>`。

### 4.1 参数兼容、预算与诊断

一次命令的首次请求和兼容重试共享 50 秒预算，处于宿主 60 秒后台任务窗口内；重试前若剩余不足 8 秒则停止并报告，不再发请求。

| 协议 | token 字段 | 拒绝时的调整 |
| --- | --- | --- |
| Chat Completions | `max_tokens` | 4xx 明确要求 `max_completion_tokens` 时改用该字段 |
| Responses | `max_output_tokens` | 不调整 |
| Anthropic Messages | `max_tokens` | 不调整（Anthropic 本就接受该字段） |

`temperature` 可以用 **Send temperature** 主动省略；任何协议下，4xx 明确拒绝温度时也会去掉后重试。只调整插件自己选择的参数：密钥错误、5xx 和其他失败直接报告，不重试；最多三次尝试。成功通知和 **Test** 结果都列出实际请求地址、token 字段及其值、温度、尝试次数和已做的调整。

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

## 6. 交互流程

1. 用户打开 Commit 工具窗或 Commit 对话框，勾选本次要提交的文件/变更块。
2. 点击工具栏上的三个按钮之一（或用 Find Action / `⌥⌘G` 调用生成）：
   - **Generate Commit Message**：只用所选变更生成；
   - **Generate Commit Message with Requirements**：先用 `ui.input-multiline` 填写本次的业务背景、Bug 说明、issue 或 Skip CI 要求，取消或留空则不发请求，填写内容只进入这一次的 user prompt；
   - **Format Commit Message**：把提交框里已有的草稿连同所选 diff 交给模型，要求保留事实并重写为规范字段；草稿为空时直接提示，不发请求。
3. 插件在后台任务中调用 `pending_commit()`，按任务类型构建 prompt 并请求 LLM；同时如果提交信息框为空，先写入「正在生成提交信息…」作为执行中的可见反馈（1.4 仍没有按钮旁的 spinner，也没有输入框占位符 API），失败或取消时清掉，用户中途输入过则不动。
4. 模型返回后，插件解析并渲染 Conventional Commit 文本，再调用 `set_commit_message(draft.revision, message)`；通知里附上本次请求诊断。
5. 用户在原生提交框里检查/修改后手动提交；插件不会自动提交。

如果生成期间用户修改了消息或勾选内容，`draft.revision` 已变化，`set_commit_message` 返回 `Error::Stale`，插件只提示「提交面板内容已变化」，不覆盖当前文本。

## 7. 已知限制

1. `pending_commit()` 的 diff 总量由 IDE 限制为 1 MiB；插件再按 `maxDiffChars` 裁剪 prompt。
2. 单次命令受宿主 60 秒后台任务上限约束；插件把首次请求和兼容重试放进共享的 50 秒预算，单次请求超时随剩余时间递减且不低于 8 秒。无流式输出。
3. `pending_commit` / `set_commit_message` 只适用于可信项目，且必须从某个窗口的 command/event 调用。
4. 多仓库项目会在 `changes` 中返回多个仓库的文件；分支与最近提交来自当前项目对应的仓库状态。
5. 插件设置是 IDE 声明的多页设置，不支持任意 Web 设置页或条件字段，插件也不能写入设置值：模型列表选择通过剪贴板转交用户。
6. 诊断摘要目前列出 token 字段、温度、尝试次数和兼容调整；思考参数与兼容结果缓存属于清单第二／三批（24–28、39 项），尚未接入，因此摘要中不含这两项。

## 8. 验证

- `cargo test`：51 个单元测试（配置、CommitDraft 上下文、历史标题与受限项目降级、diff 裁剪、三种任务的 prompt、解析/渲染、三种协议响应解析、模型列表、兼容重试、请求诊断、生成并写回提交信息）。
- `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`。
- `cargo build --release --target wasm32-wasip2`。
- `openidea-plugin check .`（用 OpenIDEA 0.1.40 构建的 CLI）：manifest、贡献点、能力、wasm 组件全部通过，组件被识别为插件 API 1.4。
- 界面与真实端点的行为尚未实测：上述验证覆盖 SDK mock host 下的行为与包结构校验；在 OpenIDEA 0.1.40 + 本地 Magpie 或真实服务端点上的三个按钮、多行输入框与模型列表选择仍需人工验证。
