# OpenIDEA 插件联调报告（0.5.3 → 0.8.0）

- 日期：2026-11-10
- 宿主：本机已安装 OpenIDEA 0.1.40（`ai.l0ng.openidea`，插件 API 1.4）
- 插件：`qszfy666.commit-message-helper` 0.8.0（active = 0.8.0，packageHash `4ad89b00…`）
- 被测代码：本仓库 `main`（0.8.0，含本轮修复），wasm 为本报告重新构建的干净版本（425,268 B）
- 结论：核心生成链路与 0.6/0.7.1/0.8.0 的大部分新功能在真实 IDE 上可用。发现 1 个缺陷，已修复（D1）。模型档案不生效（D2）查明是手写的设置行缺列，被宿主整表丢弃；2026-11-11 用设置页表格建行复测通过，插件不需要修改。另有 1 个宿主行为问题（D3：插件弹框没人回答时，别的窗口里点这个插件的命令会静默排队，2026-11-11 已复现，插件不需要修改）、1 处文档与实现不一致（D4，已修正）和 1 项工具链版本问题（D5）。

## 1. 环境与前置

- 本机 OpenIDEA 为 0.1.40，插件目录：`~/Library/Application Support/ai.l0ng.openidea/plugins/qszfy666.commit-message-helper/0.8.0`。
- 已授权能力：`vcs.read`、`vcs.write`、`secrets`、`storage`、`events`、`net:127.0.0.1`；`clipboard` 被拒绝（因此“模型列表复制到剪贴板”会走通知回退路径）。
- 测试用 LLM：本地可控 mock 服务 `127.0.0.1:18787/v1`（脚本 `/Users/liuzhiqiang/.dsh/cmh-it/mock_llm.py`），可记录每次请求并可模拟 4xx：拒绝 `max_tokens`、拒绝 `temperature`。请求日志：`/Users/liuzhiqiang/.dsh/cmh-it/requests.jsonl`。
- 测试仓库：`/Users/liuzhiqiang/WorkProjects/Rust/commit-helper-fixture`（`src/lib.rs` 修改、`src/math.rs` 新增）。
- 说明：本机同时在使用企业微信/微信/ChatGPT/Zed 等应用，会抢占前台焦点；联调期间临时隐藏了抢焦点应用，结束后已恢复。IDE 窗口原为 2720×949（超出屏幕），为便于点击已临时调整为 1400×900，不影响插件行为。

## 2. 证据来源

- 真实 UI 操作：OpenIDEA 提交面板按钮、多行输入弹框、手动字段列表弹框（通过系统辅助功能坐标点击）。
- 插件包内通知文本（AX 可读）：生成结果 + `请求地址 / token 字段 / 温度 / attempts / adjusted / cached` 诊断。
- mock 服务请求日志：逐字段核对实际发出的请求体（模型、温度、token 字段、思考参数、system/user prompt）。
- `cargo test`：129 个单元测试 + 1 个本地化目录测试全部通过；`cargo fmt --check`、`cargo clippy -D warnings`、wasm release 构建通过。

## 3. 分版本结果

### 0.6.0（第一批）

| 功能 | 结果 | 证据 |
| --- | --- | --- |
| Generate（提交面板按钮） | ✅ 通过 | `POST /v1/chat/completions` 命中配置地址；提交信息框写入 `feat(parser): support streaming responses`；通知含 `http://127.0.0.1:18787/v1/chat/completions max_tokens 64000 · temperature 0.5 · 1 attempt` |
| Generate with Requirements | ✅ 通过 | 弹出多行输入；输入 `Please mention issue ABC-123 and skip CI` 后请求 user prompt 出现需求文本与 “including the author's additional requirements” |
| Format | ✅ 通过 | 先写入草稿 `update the math helpers`，格式化请求包含该草稿，prompt 为 “Rewrite the draft commit message below …” |
| 最近提交风格参考 | ✅ 通过 | 请求 user prompt 含 `Recent Commit Subjects` 段（fixture 的 `feat: initial fixture`） |
| 参数兼容：`max_tokens` 被拒 | ✅ 通过 | mock 返回 400 后自动重试为 `max_completion_tokens`；通知：`2 attempts · adjusted: token parameter` |
| 参数兼容：`temperature` 被拒 | ✅ 通过 | 第一次带 `temperature` 被拒，重试省略；通知：`temperature omitted · 2 attempts · adjusted: temperature` |
| 请求诊断 | ✅ 通过 | 通知含地址、token 字段与值、温度、attempts、adjusted/cached |
| Models / Request URL / Test | ⚠️ 未在本轮重跑 | 0.8.0 前一轮真实 IDE 通知已见 `HTTP 200 · codearts/… http://127.0.0.1:3425/v1/chat/completions max_tokens 16 · temperature 0.5 · 1 attempt` 与 `请求地址 …`；本轮未重复。`clipboard` 被拒时 Models 应按通知回退。 |
| 50 秒共享预算 | ⚠️ 未覆盖 | 未构造慢端点/超时场景 |

### 0.7.0（第二批子批次 1）

| 功能 | 结果 | 证据 |
| --- | --- | --- |
| 提示词档案 `promptProfiles` + `defaultPrompt` | ✅ 通过 | system prompt 中出现 `PROMPT-PROFILE-MARKER` |
| 提交模板 `templates` + `defaultTemplate` | ✅ 通过 | 自定义模板渲染出 `{{header}}` + `BODY=…` + `FOOTER-TEMPLATE-MARKER` |
| 提交类型描述 `commitTypes[].description` | ✅ 通过 | user prompt 的 Allowed types 段出现 `feat (TYPE-DESC-MARKER)` |
| 模型档案 `modelProfiles` + `activeModel` | ✅ 复测通过 | 首轮手写的行没有填满 12 列，宿主把整张表换成默认空表，请求仍用单项设置（见 D2）。复测时用设置页表格「添加」一行（12 列齐全），设为当前模型档案后点「测试」，请求为 `model=profile-A-model`、`temperature 0.5`，不是单项设置的 `codearts/deepseek-v4.1-flash-1017` |
| 每档案独立密钥槽 / Max tokens | ⚠️ 未单独验证 | 复测的档案用默认密钥槽 `api-key`，授权头存在（Bearer magpie…）；「测试」固定发 `max_tokens 16`，档案的 maxTokens 要在生成请求里核对 |
| 项目级覆盖全局 | ⚠️ 未覆盖 | 未在项目层设置验证 |

### 0.7.1（第二批子批次 2）

| 功能 | 结果 | 证据 |
| --- | --- | --- |
| 思考等级 + 预设 | ✅ 通过 | `thinkingLevel=HIGH`、`thinkingPreset=openai-effort` 时请求体出现 `reasoning_effort: "high"` |
| 思考映射/预算校验/等级收敛/Thinking Help/Levels | ⚠️ 仅单测 | 单元测试覆盖；未在真实设置页逐个点按 |
| 模板预览 / 变量帮助 / 恢复默认 | ⚠️ 仅单测 | 同上 |

### 0.8.0（第三批与按需条目）

| 功能 | 结果 | 证据 |
| --- | --- | --- |
| 手动结构化编辑器 | ⚠️ 部分 | 点“编辑提交信息字段”打开字段列表，含 类型/scope/标题/正文/破坏性变更/closes/Skip CI/写入提交信息/清空全部字段；类型选择弹框列出 11 个类型并带描述。完整“编辑→写回→草稿恢复”未在真实 IDE 走完（输入控件在焦点不稳定时易失焦），仅有单元测试覆盖 |
| 字段规范化 | ✅（含修复） | `FEAT/Math`→`feat/math`、标题去尾点、正文单行按句切分为 `- …` bullets、`BREAKING CHANGE:` 前缀剥离均正确；`Closes #12, Closes #12` 原实现输出 `Closes #12, Closes`，已修复（D1） |
| 生成质量检查 | ✅ 通过 | `update files` → 通知“质量提示：标题只重复了动作…”；`fix bug` → 同时提示“标题过短（不足 12 个字符宽）” |
| 兼容结果缓存 | ✅ 通过 | 第一次 400 后重试成功；第二次生成只发 1 次请求且直接使用 `max_completion_tokens`，诊断 `1 attempt · cached: token parameter`；温度拒绝也被记入缓存（`cached: token parameter, temperature`） |
| 自定义提交语言 | ✅ 通过 | `language="Klingon"` 时 user prompt 要求 “write … in Klingon” |
| 每动作独立图标 | ✅ 通过 | 提交工具栏四个按钮分别为 wand / 气泡 / 列表 / 铅笔四个不同图标 |
| 日/韩文案 | ⚠️ 仅单测 | `tests/locales.rs` 校验中日韩词条齐全；未切换 IDE 界面语言实测 |
| 每动作显隐 | ⚠️ 未覆盖 | 未点按隐藏后验证 Find Action 仍可用 |
| 字段显隐 / 类型排序 / Skip CI 预设 / 按项目草稿 | ⚠️ 未覆盖 | 单元测试覆盖，本轮未在真实 UI 验证 |

## 4. 缺陷与问题

### D1（已修复）`normalize_closes` 未剥离重复的 `Closes` 前缀

- 现象：模型返回 `Closes #12, Closes #12` 时，渲染结果为 `Closes #12, Closes`（多出一个 `Closes` 残词）。
- 原因：`normalize_closes` 只用 `strip_label` 剥离开头一次前缀，第二个 `Closes` 被当成了 issue 引用。
- 修复：改为按逗号/空白分词，丢弃任意位置的 `closes/closes:/fixes/…` 标签词后再去重；新增回归测试 `normalize_strips_a_label_on_every_reference`。
- 补充：复核时发现这个修复让 `Closes:#12`（标签和引用之间没有空格）退化了。旧实现会剥成 `#12`，新实现原样保留，渲染出来是 `Closes Closes:#12`。现在带冒号的标签粘在引用上时只去掉标签，并新增回归测试 `normalize_strips_a_label_written_without_a_space`。两个回归测试都先在缺陷代码上失败（`"#12, Closes"`、`"Closes:#12, fixes:#13"`），修复后通过。
- 状态：`src/prompt.rs` 已修改；129 个单元测试 + 1 个词条覆盖测试、fmt、clippy 通过。补充修复之后已重新构建 wasm，并通过 0.1.40 CLI 的 `check`，重新打成调试包 `dist/qszfy666.commit-message-helper-0.8.0-debug.oiplugin`（未签名）。这个包还没有装进 IDE。

### D2（已查明，复测通过）`modelProfiles` / `activeModel` 在真实 IDE 未生效

- 现象：`promptProfiles`、`templates`、`thinking`、`language` 均从同一份 settings.toml 生效，但 `activeModel="A"` 的档案（model/profile-A-model、temperature 0.2、maxTokens 8000）没有进入请求；请求仍用全局 `model=base-model`、0.5、64000。
- 原因：宿主 0.1.40 只把通过 schema 校验的设置值交给插件，校验不过就换成默认值（`crates/ide-host/src/plugins/api.rs` 的 `setting`：`user.get(key).filter(|v| setting.accepts(v))…unwrap_or(setting.default)`）。表格行的校验要求列数和 schema 完全一致，每一列都要在、类型和枚举都要对（`crates/ide-host/src/plugins/ui_points.rs` 的 `accepts`：`o.len() == self.properties.len() && …all(…)`），只要有一行不合格，整张表都不通过。`modelProfiles` 有 12 列，手写的行只填了 name、model、temperature、maxTokens 等几列，所以插件读到的是默认空表，`selected()` 找不到档案，回退到单项设置。`promptProfiles`（name、text）和 `templates`（name、template）只有两列，手写时容易填全，所以它们生效。设置页本身也用同一套校验（`src/features/plugins/settings.tsx` 的 `accepts`），存了不合格值时表格显示为空。
- 插件侧：`config::selected()` 的逻辑没有问题，不需要改。设置页「Add」用所有列的默认值新建一行（`structuredClone(item.default)`），所以通过界面建的行是完整的。
- 复测（2026-11-11，OpenIDEA 0.1.40，同一个 mock）：
  - 在 设置 › 插件 › Git Commit Message Helper › LLM 设置（全局）的「模型档案」表格点「添加」，新行 12 列都带默认值。填名称 `A`、基础 URL `http://127.0.0.1:18787/v1`、模型 `profile-A-model`，其余列保持默认（温度 0.5、最大响应 token 数 64000、密钥槽 `api-key`），「当前模型档案」填 `A`，点「应用」。
  - 全局 settings.toml 里写入的行有 12 列（name、provider、protocol、baseUrl、model、temperature、sendTemperature、maxTokens、secretSlot、thinkingLevel、thinkingPreset、thinkingMapping），并有 `activeModel = "A"`。
  - 点同页的「测试」，mock 收到 `POST /v1/chat/completions`，请求体 `model=profile-A-model`、`temperature=0.5`、`max_tokens=16`。单项设置的模型是 `codearts/deepseek-v4.1-flash-1017`，所以请求用的是档案。`max_tokens 16` 是「测试」固定的 `TEST_TOKENS`（`src/llm.rs`）。「测试」和「生成」都通过 `Config::load()` 解析档案（`src/lib.rs` 的 `test_connection` 和 `generate`），本轮没有再点「生成」。
  - 温度和最大 token 数没有改成首轮的 0.2 和 8000：数字输入框只能靠按键清空，而本机输入法（拼音模式）会截获自动发送的按键，所以保持默认值。模型名已能区分档案和单项设置。
  - 复测后已在设置页删除这一行、清空「当前模型档案」并应用，再用测试前的备份覆盖 settings.toml，与测试前逐字节一致。
- 遗留风险：以后给表格加列时，旧版本存下的行会因列数不一致被整表丢弃，档案会静默失效。0.7.0 和 0.7.1 没有打 tag 发布，目前没有用户持有旧格式的行，所以本轮不处理。

### D3（已复现，宿主行为）插件弹框没人回答时，这个插件的其它命令静默排队

- 原现象（2026-11-10）：先打开 `hospital`、再切回 `commit-helper-fixture` 后，点击“生成提交信息”无任何请求、无通知、插件日志无 `run`；同一时刻点击“提交/贮藏”页签等原生控件正常。完全退出并重启 OpenIDEA 后，首次点击即成功。
- 复现（2026-11-11，OpenIDEA 0.1.40，同一个 mock）：
  - 同一窗口切项目**不会**复现。主窗口从 hospital 切到 fixture 后点「生成」正常。再用「最近打开」菜单把主窗口切到一个临时仓库再切回 fixture（日志为 `open_project_window … new_window=false`、`in window main`），点「生成」也正常：mock 收到请求，提交消息框写入结果，通知显示。
  - 能复现的顺序：在主窗口（fixture）点「按补充要求生成提交信息」，弹框打开后不回答；在第二个窗口（`project-1`，临时仓库）点「生成」。10 秒内 mock 没有请求，提交消息框为空，没有通知，日志没有记录，与原现象一致。回到主窗口点弹框的「取消」，1 秒后排队的「生成」发出请求，并写入了 `project-1` 的提交消息框。
  - 弹框切项目后仍然显示。在同一窗口里它是模态的，挡住整个窗口，所以「按钮能点但没反应」只会出现在别的窗口。
- 原因（宿主 0.1.40）：
  - `crates/ide-host/src/plugins/api.rs` 的 `ask` 等弹框回答时没有超时，只有收到回答、取消令牌被触发、或者窗口关闭（`forget_client`，只在窗口销毁时调用）才结束。
  - 同一个插件的调用是串行的，后面的 `run_command` 在队列里等前一个调用返回。
  - 前端 `runPluginCommand`（`src/features/plugins/store.ts`）一直等待返回，期间没有忙碌提示。
- 原始证据的局限：
  - 当时 `plugins` 组通知是 `display = "none"`，命令失败的通知也属于这个组，所以「无通知」说明不了命令没报错。
  - 插件的 stderr 在调用返回后才写进日志（`crates/ide-plugin/src/runtime.rs` 的 `call`），卡住的调用不会留下 `run`。
  - 2026-11-10 从 11:24:30（UTC）插件激活到 12:24:34 重启，日志里没有一次完成的调用，mock 也没收到插件的请求，故障可能在切项目之前就开始了。当天开过 `main`、`project-1`、`project-2` 三个窗口，没有关窗记录，无法确认当时别的窗口里是否挂着弹框。只能说这个机制能产生同样的现象，不能确定昨天就是它。
- 插件侧：不需要修改。插件在等弹框回答时收不到别的窗口的点击，也没有接口知道有调用在排队。
- 建议宿主：弹框等待设上限，或者插件被弹框占用时，点击它的命令要给出提示，并把挂着弹框的窗口带到前面。已起草上游 issue，尚未提交。

### D4（已修正）0.8.0 CHANGELOG 描述的是“工具窗视图”，实现是“字段列表弹框”

- `CHANGELOG.md` 0.8.0 写“新增 Commit Message Editor 工具窗（`[[contributes.views]]`，本项目第一个视图）”，但 `plugin.toml` 无 `contributes.views`（grep 计数 0），最新提交 `3e81ef7 feat(editor): edit commit fields from a dialog list instead of a tool window` 已改为 `host::pick` 字段列表；`docs/modal-editor.md` 仍标注“状态：开发中”。
- 建议：更新 0.8.0 变更说明与 modal-editor 状态，避免验收按“工具窗”去核对。
- 处理：`CHANGELOG.md` 0.8.0 的 3、5、7、34、47 条和测试数已按现状改写；`docs/modal-editor.md` 状态改为已完成并补上本次联调结果；`docs/design.md` 的验证段改为引用本报告；`docs/migration-checklist.md` 末尾追加了 3、5、7 实现入口变更的记录。

### D5（工具链）本机 `openidea-plugin` CLI 版本过旧，包检查门禁无法执行

- `~/.cargo/bin/openidea-plugin` 为 **0.1.39（plugin API 1.3.0）**；对 `engines.openidea = "^1.4"` 的包执行 `openidea-plugin check .` 直接失败：
  `engines.openidea "^1.4" does not match this IDE's plugin API 1.3.0`。
- `build.sh` 的第五道门禁因此在本机跑不通；需要 0.1.40 的 CLI（本地源码 `/Users/liuzhiqiang/WorkProjects/Lab/OpenSource/openidea` 仍是 0.1.39，未含 0.1.40 补丁）。
- 本机已有 0.1.40 的 CLI：`~/.cargo/git/checkouts/openidea-7faf5f9c9e6a9886/37c2a60/target/release/openidea-plugin`（`--version` 输出 `0.1.40 (plugin API 1.4.0)`），用它执行 `check .` 和 `pack` 都通过。`~/.cargo/bin` 里的那个还是 0.1.39。

## 5. 本轮代码改动

- `src/prompt.rs`
  - `normalize_closes`：按词剥离每个 `Closes/Fixes/Resolves/Refs` 标签并去重（修复 D1）。
  - 新增回归测试 `normalize_strips_a_label_on_every_reference`。
  - 复核后补充：`Closes:#12` 这种不带空格的写法只去掉标签，新增回归测试 `normalize_strips_a_label_written_without_a_space`。
- 移除了此前会话遗留在 `src/lib.rs` 的 `CMHDBG` 调试 `eprintln!`（其中 `&body[..200]` 在多字节字符处会 panic，已在日志中观察到 `byte index 200 is not a char boundary` 导致的插件崩溃）。
- 重新构建并安装干净 0.8.0 wasm（无调试输出）。

## 6. 未覆盖项与建议下一步

1. D2 已复测通过（见 D2）。还可以在生成请求里核对档案的 maxTokens 和非默认密钥槽。
2. D3 已复现（见 D3），插件不需要修改；宿主侧的改进建议已起草为上游 issue，尚未提交。
3. 逐项走查仅单测覆盖的 UI 项：手动编辑器写回与按项目草稿、字段显隐/排序、Skip CI 预设、每动作显隐（Find Action 回退）、日韩界面语言、模板预览/变量帮助/恢复默认、思考映射校验与 Thinking Levels/Help。
4. 用真实服务端点补测 Models 列表、Request URL、Test，以及 50 秒共享预算/超时路径。
5. `check .` 和 `pack` 已用 0.1.40 CLI 补跑并通过（见 D5）；发布前还要签名打包。

## 7. 复现与产物

- mock 服务：`/Users/liuzhiqiang/.dsh/cmh-it/mock_llm.py`（`python3 mock_llm.py 18787`），模式文件 `/Users/liuzhiqiang/.dsh/cmh-it/mode.json`。
- 请求日志：`/Users/liuzhiqiang/.dsh/cmh-it/requests.jsonl`。
- 设置备份：`/Users/liuzhiqiang/.dsh/cmh-it/settings.toml.bak`（联调后已还原本机原设置）。
- 辅助工具（本轮临时使用，不在仓库内）：`/Users/liuzhiqiang/.dsh/cmh-it/tools/{axfind,axdump,axpress,clickpid}`。
