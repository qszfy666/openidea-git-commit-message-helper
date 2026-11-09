# 设计说明：OpenIDEA 提交信息生成插件

## 1. 结论先行

OpenIDEA 0.1.39（`openidea:plugin@1.3.0`）已经提供原生提交工作流：

- `[[contributes.menus]] menu = "commit.message"`：在 Commit 工具窗和 Commit 对话框的提交信息上方显示插件命令按钮；
- `host::pending_commit()`：返回当前窗口正在准备的提交，包括实际包含的文件、部分 hunk 的 diff、`amend` 和消息草稿；
- `host::set_commit_message(revision, text)`：把生成结果写入原生提交信息框，并按 `revision` 防止覆盖用户后来输入的内容。

因此插件不再需要自己的 Tool Window、文件勾选清单、剪贴板复制，也不再需要 `process:git`。插件从「工具窗 + 复制粘贴」改为「原生提交面板按钮 + 一键填入」。

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
        │
        └─ WASM 插件（Rust SDK 1.3）
              ├─ git.rs    从 CommitDraft 构建 prompt 上下文，裁剪 diff
              ├─ prompt.rs prompt + JSON 字段契约 + 本地 Conventional Commit 渲染
              ├─ llm.rs    host::fetch → OpenAI Chat / OpenAI Responses / Anthropic Messages
              ├─ config.rs 设置与服务商预设
              ├─ ui.rs     中英文通知文案
              └─ lib.rs    命令入口：generate / test
```

## 3. 数据采集

- `host::pending_commit()` 读取当前窗口的 Commit draft；没有仓库时返回 `None`。
- 只包含用户在 Commit 面板中勾选的文件和变更块；部分 hunk 选择也按实际包含范围提供 diff。
- `CommitChange::diff` 为相对 HEAD 的 unified diff；二进制内容为 `None`；所有变更的 diff 合计最多 1 MiB，超过后 `truncated = true`。
- 读取前会保存相关文件的未保存编辑器，与用户直接点击 Commit 时的行为一致；不会暂存或提交任何内容。
- `Project::vcs_status()` 提供分支和仓库根目录；插件 API 1.3 没有读取提交历史的接口（`vcs.log` 是 1.4），prompt 里不含历史提交。
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
- **LLM Settings**：Provider、协议、Base URL、Model、Temperature、Max response tokens；页面的 **Test** 动作会先应用设置，再发起一次最小请求验证连接。

网络白名单通过 `plugin.toml` 的 optional `net:` 能力声明；内置 OpenAI、Anthropic、DeepSeek、Qwen、Kimi、GLM、MiniMax、SiliconFlow、Volcengine、Azure OpenAI、OpenRouter、Groq、Mistral、Google Gemini、`127.0.0.1`（Magpie）与 `localhost`（Ollama）。Magpie 示例：`provider = Magpie`、`baseUrl = http://127.0.0.1:3425`、`model = <Magpie 模型 id>`。

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
2. 点击提交信息上方的 **Generate Commit Message**（或使用 Find Action / `⌥⌘G`）。
3. 插件在后台任务中调用 `pending_commit()`，生成 prompt 并请求 LLM；同时如果提交信息框为空，先写入「正在生成提交信息…」作为执行中的可见反馈（1.3 没有按钮旁的 spinner，也没有输入框占位符 API），失败或取消时清掉，用户中途输入过则不动。
4. 模型返回后，插件解析并渲染 Conventional Commit 文本，再调用 `set_commit_message(draft.revision, message)`。
5. 用户在原生提交框里检查/修改后手动提交；插件不会自动提交。

如果生成期间用户修改了消息或勾选内容，`draft.revision` 已变化，`set_commit_message` 返回 `Error::Stale`，插件只提示「提交面板内容已变化」，不覆盖当前文本。

## 7. 已知限制

1. `pending_commit()` 的 diff 总量由 IDE 限制为 1 MiB；插件再按 `maxDiffChars` 裁剪 prompt。
2. 单次生成受 60 秒后台任务上限约束，LLM 请求超时为 55 秒；无流式输出。
3. `pending_commit` / `set_commit_message` 只适用于可信项目，且必须从某个窗口的 command/event 调用。
4. 多仓库项目会在 `changes` 中返回多个仓库的文件；分支来自当前项目对应的仓库状态。
5. 插件设置是 IDE 声明的多页设置，不支持任意 Web 设置页或条件字段。

## 8. 验证

- `cargo test`：17 个单元测试（配置、CommitDraft 上下文、diff 裁剪、prompt/解析/渲染、三种协议响应解析、生成并写回提交信息）。
- `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`。
- `cargo build --release --target wasm32-wasip2`。
- `openidea-plugin check .`：manifest、贡献点、能力、wasm 组件全部通过。
- 本地实测：OpenIDEA 0.1.39 + 本地 Magpie（`http://127.0.0.1:3425`，`deepseek/deepseek-flash`），在 Commit 面板点击生成后提交信息直接填入，撤销/重新输入不会被旧结果覆盖。
