# Changelog

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
