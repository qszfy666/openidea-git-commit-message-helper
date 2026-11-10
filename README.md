# Git Commit Message Helper for OpenIDEA

Generate a clear, consistent **Conventional Commits** message for the commit you are preparing, with an LLM — directly from OpenIDEA's native Commit panel.

This is an OpenIDEA WebAssembly plugin (Rust SDK, plugin API 1.4). It is inspired by the IntelliJ plugin [git-commit-message-helper](https://github.com/AutismSuperman/git-commit-message-helper), but uses OpenIDEA's plugin model.

## What it does

- Adds three buttons to the message toolbar of the Commit tool window and the Commit dialog (`commit.message`): **Generate Commit Message**, **Generate Commit Message with Requirements**, and **Format Commit Message**.
- **With Requirements** opens a multi-line dialog for business background, a bug description, issue references or a Skip CI request; the text goes into this one generation only. **Format Commit Message** rewrites the draft already in the message field into the structured fields, preserving its facts, issue numbers and language.
- Reads exactly the files and changed blocks included in the commit through `host::pending_commit()` — including partial hunk selections. Unsaved editors of those files are saved first, as Commit does.
- Sends their status and included diffs (trimmed per hunk and by a character budget) to your LLM, plus the last five commit subjects from `Project::vcs_log()` as a style reference. History is only a style hint: the working-tree `vcs_diff()` is never used, and the generated content stays grounded in the selected changes.
- The model returns structured fields (`type`, `scope`, `subject`, `body`, `changes`, `closes`, `skipCi`); the plugin renders the final Conventional Commit locally, so formatting stays consistent.
- Reusable named profiles: keep several model configurations (each with its own token limit and API key slot), writing preferences and output templates, and switch between them. A project can pick its own defaults and inherit the global ones.
- Shows that it is running: an empty message field holds `Generating commit message…` until the result replaces it (a field you already typed in stays untouched), with the progress task in the status bar either way.
- Writes the generated message into the native commit message field with `host::set_commit_message()`. If you type or change the included changes while the model is answering, the revision check rejects the write and your current text is preserved.
- Retries a request a model refuses because of `max_tokens` (switching to `max_completion_tokens`) or `temperature` (dropping it). All attempts share one 50 s budget inside the host's 60 s task window; a **Send temperature** setting can also omit the parameter up front.
- Reports what was actually sent — request URL, token parameter, temperature, attempt count and any compatibility adjustment — in the success notification and in the **Test** result.
- Review the message in the Commit panel and commit when ready. The same command is available from **Find Action** and the keymap (`⌥⌘G` / `Ctrl+Alt+G`).

## Requirements

- OpenIDEA 0.1.40 or newer (plugin API 1.4).
- An OpenAI-compatible, OpenAI Responses, or Anthropic Messages endpoint. API keys are stored in the system keychain as four declared plugin secrets (`api-key`, `api-key-2`, `api-key-3`, `api-key-4`), never in settings or storage.

No `git` binary is needed: the IDE supplies the pending commit and its diffs.

## Install

**Settings › Plugins › Available**, search for **Git Commit Message Helper** (published as
`qszfy666.commit-message-helper` through the
[community registry](https://github.com/l0ng-ai/openidea-plugins)), install it and grant the
capabilities the install page asks for.

Offline, build the package yourself:

```sh
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin pack . --out dist/qszfy666.commit-message-helper-0.7.0.oiplugin   # unsigned, for local install
```

Then either:

- **Settings › Plugins › Install Plugin from Disk…** and pick that `.oiplugin` file or this folder, or
- link it during development:

```sh
openidea-plugin link /Users/liuzhiqiang/WorkProjects/Rust/openidea-plugin/git-commit-message-helper
```

Open **Settings › Plugins** to load the change. The plugin appears as **Git Commit Message Helper**.

## Configure

**Settings › Plugins › Git Commit Message Helper** has four pages:

| Page | Settings |
| --- | --- |
| Commit | Commit message language (`auto` follows the IDE interface language, or Chinese/English), maximum diff characters, allowed commit types (a title + description table; the description is included in the prompt). |
| Prompts | Extra system prompt for persistent preferences, prompt profile table and **Default prompt profile**. |
| Commit Templates | Output template table and **Default template**. |
| LLM Settings | Provider preset, API protocol, Base URL, Model, Temperature, **Send temperature**, Max response tokens, model profile table and **Active model profile**; and three page actions: **Test** (a minimal request, with request diagnostics), **Models** (lists the endpoint's model ids), **Request URL** (shows the exact URL a generation posts to). Every action applies pending settings first. |

API keys are plugin secrets, not fields on those pages: **Settings › Plugins › Installed › select Git Commit Message Helper › Secrets › LLM API key › Save**. Four slots are declared (`api-key`, `api-key-2`, `api-key-3`, `api-key-4`); a model profile chooses one in its **Key slot** column. They are kept in the system keychain, never in the settings files.

**Models** calls `GET {base}/models`, lets you pick an id and copies it to the clipboard. The host's declarative settings are read-only to plugins, so paste it into **Model**; the field is always editable by hand. **Request URL** uses the same Base URL/protocol rules as generation, so it shows what the next request will actually hit.

### Named profiles, templates and project defaults

Three declarative tables let you keep several configurations. Plugin settings are read-only to plugins, so add, rename, edit, copy and remove rows directly in the IDE settings page.

| Table | Page | Selector | Nothing selected |
| --- | --- | --- | --- |
| **Model profiles** | LLM Settings | **Active model profile** | The single LLM settings above the table |
| **Prompt profiles** | Prompts | **Default prompt profile** | The single **Extra system prompt** |
| **Commit templates** | Commit Templates | **Default template** | The built-in **Default** template |

A model profile carries its own provider, protocol, Base URL, model, temperature, **Max response tokens** and key slot. The three page actions (Test / Models / Request URL) use the active profile too; leaving the selector blank keeps the 0.6.x single settings in charge, so existing configurations need no migration.

An output template controls the final text with variables `{{type}}`, `{{scope}}`, `{{subject}}`, `{{body}}`, `{{changes}}`, `{{closes}}`, `{{skipCi}}` and `{{header}}` (`type(scope): subject`). A conditional section `{{#body}}…{{/body}}` is emitted only when that field is non-empty, which lets you choose field order, separators and optional paragraphs; text outside variables is kept verbatim, so a footer is just a last line. For example, this template keeps the header, emits the body section only when there is one, and always adds a footer (the blank lines are real newlines in the text area):

```text
{{header}}{{#body}}

{{body}}{{/body}}

Signed-off-by: Team
```

The built-in **Default** always exists and cannot be shadowed by a row named `Default`. Nested sections, loops and expressions are rejected; an invalid template falls back to Default and the notification explains why. Full Velocity compatibility is a separate evaluation.

Each selector is an ordinary setting key. The IDE resolves settings as project over global: a value set for this project wins, leaving it blank in a project inherits the global choice. The profile tables themselves follow the same rule; the two layers are not merged.

The plugin declares these optional network capabilities; grant the ones you use:

`api.openai.com`, `api.anthropic.com`, `api.deepseek.com`, `dashscope.aliyuncs.com`, `api.moonshot.cn`, `open.bigmodel.cn`, `api.minimax.chat`, `api.siliconflow.cn`, `ark.cn-beijing.volces.com`, `*.openai.azure.com`, `openrouter.ai`, `api.groq.com`, `api.mistral.ai`, `generativelanguage.googleapis.com`, `127.0.0.1` (Magpie, local servers), `localhost` (Ollama). `clipboard` (optional) is used only to copy the chosen model id.

Magpie example: provider **Magpie**, Base URL `http://127.0.0.1:3425`, model your Magpie model name. Plain HTTP is allowed only to loopback by the IDE.

### Why these capabilities

| Capability | Why the plugin needs it |
| --- | --- |
| `vcs.read` | Read the commit being prepared (the included files, their diffs), the repository root/branch, and the last five commit subjects as a style reference. Nothing else is read. |
| `vcs.write` | Write the generated text into the commit message field (`set_commit_message`). The plugin never commits, stages, fetches or pushes. |
| `secrets` | Keep the LLM API key in the OS keychain and let the IDE fill it into requests; plugin code never sees the value. |
| `net:<host>` (optional) | Send the prompt, the model-list request and the connection test to the LLM endpoint you configure. One optional host per preset provider; grant only the one you use. `127.0.0.1`/`localhost` are for local servers (Magpie, Ollama). |
| `clipboard` (optional) | Copy the model id chosen in the **Models** picker. Without it the id is shown in a notification instead. |

No `process:`, `fs:` or `ide.commands` capability is requested, and there is no telemetry.

## Supported protocols

- **OpenAI Chat Completions** — `POST {base}/chat/completions`, `Authorization: Bearer`.
- **OpenAI Responses** — `POST {base}/responses`, `instructions` + `input`.
- **Anthropic Messages** — `POST {base}/v1/messages`, `x-api-key` + `anthropic-version`.

The endpoint URL is completed from the Base URL: a path-less URL gets `/v1`; a URL that already has a path gets the protocol suffix (`/chat/completions`, `/responses`, `/messages`); a URL that already ends in the suffix is used as-is.

Chat Completions sends `max_tokens`, Responses `max_output_tokens`, Anthropic `max_tokens`. If a model answers 4xx refusing `max_tokens` in favour of `max_completion_tokens`, or refusing `temperature`, the plugin removes just that parameter and retries; other failures (bad key, server error) are reported, not retried. At most three attempts share the 50 s budget.

## Privacy

- The included files' status and diff, and the last five commit subjects of the repository, are sent to the endpoint you configure. No working-tree diff beyond the selected changes and no other repository data is sent.
- The API key is filled into the request by the IDE from the system keychain; plugin code never reads it.
- No telemetry, no other network calls.

## Development

```sh
cargo test                                     # unit tests, SDK mock host
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip2
```

Layout:

- `src/config.rs` — settings, provider presets, named-profile resolution and project defaults.
- `src/git.rs` — pending-commit context and diff trimming.
- `src/prompt.rs` — prompt construction, JSON parsing, template rendering (built-in Default and named templates).
- `src/llm.rs` — the three protocol clients over `host::fetch`, key slots, the model list, compatibility retries, request diagnostics and the connection test.
- `src/lib.rs` — plugin commands; `plugin.toml` — manifest, capabilities, contributions.

## License

Apache-2.0. See [LICENSE](LICENSE).
