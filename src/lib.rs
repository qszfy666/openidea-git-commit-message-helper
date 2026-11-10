//! Git Commit Message Helper: generate a Conventional Commits message for the
//! commit being prepared in OpenIDEA's native Commit tool window or Commit
//! dialog.
//!
//! The command is contributed to `commit.message`, so it appears above the
//! message field. It reads exactly the included files and changed blocks
//! through `host::pending_commit()` and writes the generated text back with
//! `host::set_commit_message()`; the revision check leaves text typed during
//! generation untouched.

mod config;
mod git;
mod llm;
mod prompt;
mod thinking;
mod ui;

use config::{Config, UiLanguage};
use openidea_plugin::{
    Error, Invocation, NotificationLevel, PickItem, Plugin, host, register_plugin,
};
use prompt::Task;

const TITLE: &str = "Git Commit Message Helper";

#[derive(Default)]
pub struct CommitMessageHelper;

/// What one run asks the model for. The draft is read once, inside
/// `generate`, so `Format` sees the message it is about to replace.
enum Action {
    Generate,
    WithRequirements(String),
    Format,
}

/// A generated message and the request that produced it.
struct Generated {
    message: String,
    diagnostics: llm::Diagnostics,
    /// Why the built-in Default had to be used instead of a named template.
    template_error: Option<String>,
}

impl Generated {
    /// The balloon: the message, then what the request carried.
    fn notification(&self, strings: &ui::Strings) -> String {
        let mut body = format!(
            "{}\n\n{}\n\n{}",
            strings.generated,
            ui::clip(&self.message, 800),
            self.diagnostics.summary()
        );
        if let Some(error) = &self.template_error {
            body.push_str(&format!("\n\n{} {error}", strings.template_fallback));
        }
        body
    }
}

impl CommitMessageHelper {
    fn generate(&mut self, ctx: &Invocation, action: Action) -> Result<Generated, Error> {
        let strings = ui::strings(UiLanguage::load());
        let config = Config::load();
        if !host::secret_is_set(&config.secret_slot) {
            return Err(Error::Failed(format!(
                "{} {}: {}",
                strings.not_configured, strings.key_slot, config.secret_slot
            )));
        }
        let task = host::begin_task(TITLE, true);
        task.report(0, None, Some(strings.collecting));

        let draft = host::pending_commit()?
            .ok_or_else(|| Error::Failed(strings.no_repository.to_owned()))?;
        if draft.changes.is_empty() {
            return Err(Error::Failed(strings.no_changes.to_owned()));
        }
        let asked = match &action {
            Action::Generate => Task::Generate,
            Action::WithRequirements(requirements) => Task::Requirements(requirements),
            Action::Format if draft.message.trim().is_empty() => {
                return Err(Error::Failed(strings.no_draft.to_owned()));
            }
            Action::Format => Task::Format(&draft.message),
        };

        // The message field is the only place the Commit panel can show that the
        // command runs: plugin API 1.4 has no other slot next to the button.
        let progress = Progress::start(strings, &draft);
        let generated: Result<Generated, Error> = (|| {
            task.report(20, Some(100), Some(strings.generating));
            let project = ctx.project();
            let context = git::build_context(project.as_ref(), &draft, config.max_diff_chars);
            let (system, user) = prompt::build_prompts(&config, &context, asked);
            let answer = llm::generate(&config, &system, &user)?;
            if task.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let allowed_types: Vec<String> = config
                .commit_types
                .iter()
                .map(|item| item.title.clone())
                .collect();
            let fields = prompt::parse_response(&answer.text, &allowed_types)
                .ok_or_else(|| Error::Failed(strings.bad_response.to_owned()))?;
            let template = config
                .template
                .as_deref()
                .unwrap_or(prompt::DEFAULT_TEMPLATE);
            let (message, template_error) = match prompt::render_template(&fields, template) {
                Ok(message) => (message, None),
                // A broken template must not lose the generated message.
                Err(error) => (prompt::render(&fields), Some(error)),
            };
            Ok(Generated {
                message,
                diagnostics: answer.diagnostics,
                template_error,
            })
        })();
        let generated = match generated {
            Ok(generated) => generated,
            Err(error) => {
                progress.clear();
                return Err(error);
            }
        };

        progress.finish(&generated.message)?;
        task.report(100, Some(100), Some(strings.generated));
        Ok(generated)
    }

    fn test_connection(&mut self) -> Result<(String, llm::Diagnostics), Error> {
        let strings = ui::strings(UiLanguage::load());
        let task = host::begin_task("Test LLM Connection", true);
        task.report(0, None, Some(strings.generating));
        let config = Config::load();
        let answer = llm::test(&config)?;
        task.report(100, Some(100), Some(strings.test_ok));
        Ok((
            format!("HTTP {} · {}", answer.status, config.effective_model()),
            answer.diagnostics,
        ))
    }

    /// Lists the endpoint's models and copies the chosen id: settings are
    /// read-only for a plugin, so the user pastes it into the Model field.
    fn list_models(&mut self, strings: &ui::Strings) -> Result<String, Error> {
        let config = Config::load();
        let models = {
            let task = host::begin_task("List LLM Models", true);
            task.report(0, None, Some(strings.collecting));
            llm::models(&config)?
        };
        let items: Vec<PickItem> = models.iter().map(|id| PickItem::new(id, id)).collect();
        let Some(id) = host::pick(strings.pick_model, &items)? else {
            return Err(Error::Cancelled);
        };
        let hint = if host::copy_to_clipboard(&id).is_ok() {
            strings.model_hint
        } else {
            strings.model_copy_failed
        };
        Ok(format!("{id}\n\n{hint}"))
    }

    fn show_endpoint(&mut self, strings: &ui::Strings) -> Result<String, Error> {
        let config = Config::load();
        Ok(format!(
            "{}\n{}",
            strings.request_url,
            llm::endpoint(&config)?
        ))
    }

    /// Checklist 11: renders the template the user is editing. A declarative
    /// settings table has no edit events for the plugin, so the action opens a
    /// multiline editor pre-filled with the active template, and loops: edit,
    /// confirm, see the rendered result. Example fields are picked once with
    /// `pick_many` so conditional sections can be tried with values or empty.
    fn preview_template(&mut self, strings: &ui::Strings) -> Result<Option<String>, Error> {
        let config = Config::load();
        let items: Vec<PickItem> = prompt::EXAMPLE_FIELDS
            .iter()
            .copied()
            .map(|name| PickItem::new(name, name))
            .collect();
        let Some(selected) = host::pick_many(strings.preview_fields_prompt, &items)? else {
            return Self::report(strings, Err(Error::Cancelled));
        };
        let fields = prompt::example_fields(&selected);
        let mut template = config
            .template
            .unwrap_or_else(|| prompt::DEFAULT_TEMPLATE.to_owned());
        while let Some(text) =
            host::input_multiline(strings.preview_template_prompt, Some(&template))?
        {
            template = text;
            match prompt::render_template(&fields, &template) {
                Ok(rendered) => host::notify(
                    NotificationLevel::Info,
                    TITLE,
                    &format!("{}\n\n{}", strings.preview_result, ui::clip(&rendered, 800)),
                ),
                Err(error) => host::notify(
                    NotificationLevel::Error,
                    TITLE,
                    &format!("{}: {error}", strings.preview_error),
                ),
            }
        }
        Ok(None)
    }

    /// Checklist 12: the built-in template cannot be written into the settings
    /// by a plugin, so the restore action copies it for pasting instead.
    fn restore_default(&mut self, strings: &ui::Strings) -> Result<String, Error> {
        let hint = if host::copy_to_clipboard(prompt::DEFAULT_TEMPLATE).is_ok() {
            strings.copy_ok
        } else {
            strings.copy_failed
        };
        Ok(format!("{hint}\n\n{}", prompt::DEFAULT_TEMPLATE))
    }

    /// Checklist 27: declarative settings cannot filter an enum column, so
    /// this page action shows the levels the current preset/mapping offers and
    /// warns when the selected level is not among them.
    fn thinking_levels(&mut self, strings: &ui::Strings) -> Result<String, Error> {
        let config = Config::load();
        let (levels, error) =
            thinking::available_levels(&config.thinking, config.protocol, config.max_tokens);
        let names = levels
            .iter()
            .map(|level| level.key())
            .collect::<Vec<_>>()
            .join(", ");
        let mut body = format!(
            "{}: {}\n{}: {}\n{}: {names}",
            strings.thinking_preset,
            config.thinking.preset.key(),
            strings.thinking_selected,
            config.thinking.level.key(),
            strings.thinking_available,
        );
        if !levels.contains(&config.thinking.level) {
            body.push_str(&format!("\n{}", strings.thinking_unavailable));
        }
        if let Some(error) = error {
            body.push_str(&format!("\n{}: {error}", strings.mapping_error));
        }
        Ok(body)
    }

    /// Checklist 26/12: mapping format, validation rules and the restore entry.
    /// The preset template is copied so it can be pasted into the mapping
    /// column; clearing that column restores it without any plugin write.
    fn thinking_help(&mut self, strings: &ui::Strings) -> Result<String, Error> {
        let config = Config::load();
        let template = thinking::preset_template(config.thinking.preset, config.protocol);
        let effective = thinking::mapping_text(&config.thinking, config.protocol);
        let mut body = strings.thinking_help.to_owned();
        if let Err(error) = thinking::validate(&effective) {
            body.push_str(&format!("\n\n{}: {error}", strings.mapping_error));
        }
        let hint = if host::copy_to_clipboard(&template).is_ok() {
            strings.copy_ok
        } else {
            strings.copy_failed
        };
        Ok(format!("{body}\n\n{hint}\n{}", ui::clip(&template, 1200)))
    }

    /// Shows the outcome of a command that reports through notifications.
    fn report(
        strings: &ui::Strings,
        result: Result<String, Error>,
    ) -> Result<Option<String>, Error> {
        match result {
            Ok(body) => host::notify(NotificationLevel::Info, TITLE, &body),
            Err(Error::Cancelled) => host::notify(NotificationLevel::Info, TITLE, strings.canceled),
            Err(Error::Stale) => host::notify(NotificationLevel::Warning, TITLE, strings.stale),
            Err(error) => host::notify(
                NotificationLevel::Error,
                TITLE,
                &format!("{}: {error}", strings.error_prefix),
            ),
        }
        Ok(None)
    }
}

/// The text written into an empty message field while the model answers, so
/// the Commit panel shows the command was taken; every path replaces or clears
/// it again, and a newer revision (typing) is never overwritten.
struct Progress {
    revision: u64,
    text: &'static str,
    shown: bool,
}

impl Progress {
    fn start(strings: &ui::Strings, draft: &openidea_plugin::CommitDraft) -> Self {
        let mut progress = Self {
            revision: draft.revision,
            text: strings.generating_now,
            shown: false,
        };
        if !draft.message.trim().is_empty() {
            return progress;
        }
        if host::set_commit_message(progress.revision, progress.text).is_err() {
            return progress;
        }
        progress.shown = true;
        // Our own write moved the revision on; the result goes to the new one.
        if let Ok(Some(draft)) = host::pending_commit() {
            progress.revision = draft.revision;
        }
        progress
    }

    /// Replaces the placeholder with the generated message.
    fn finish(&self, message: &str) -> Result<(), Error> {
        match host::set_commit_message(self.revision, message) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.clear();
                Err(error)
            }
        }
    }

    /// Takes the placeholder back out, but only while the field still holds it.
    fn clear(&self) {
        if !self.shown {
            return;
        }
        if host::set_commit_message(self.revision, "").is_ok() {
            return;
        }
        if let Ok(Some(draft)) = host::pending_commit()
            && draft.message.trim() == self.text
        {
            let _ = host::set_commit_message(draft.revision, "");
        }
    }
}

impl Plugin for CommitMessageHelper {
    fn run_command(
        &mut self,
        id: &str,
        ctx: &Invocation,
        _arg: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let strings = ui::strings(UiLanguage::load());
        match id {
            "generate" => Self::report(
                strings,
                self.generate(ctx, Action::Generate)
                    .map(|g| g.notification(strings)),
            ),
            "generate-with-context" => {
                // The dialog is what differs from `generate`: requirements the
                // author writes for this commit only.
                let requirements = match host::input_multiline(strings.requirements_prompt, None) {
                    Ok(Some(text)) if !text.trim().is_empty() => text,
                    Ok(_) => return Self::report(strings, Err(Error::Cancelled)),
                    Err(error) => return Self::report(strings, Err(error)),
                };
                Self::report(
                    strings,
                    self.generate(ctx, Action::WithRequirements(requirements))
                        .map(|g| g.notification(strings)),
                )
            }
            "format" => Self::report(
                strings,
                self.generate(ctx, Action::Format)
                    .map(|g| g.notification(strings)),
            ),
            "models" => Self::report(strings, self.list_models(strings)),
            "endpoint" => Self::report(strings, self.show_endpoint(strings)),
            "template-preview" => self.preview_template(strings),
            "template-help" => Self::report(strings, Ok(strings.template_help.to_owned())),
            "template-default" => Self::report(strings, self.restore_default(strings)),
            "thinking-levels" => Self::report(strings, self.thinking_levels(strings)),
            "thinking-help" => Self::report(strings, self.thinking_help(strings)),
            "test" => match self.test_connection() {
                Ok((message, diagnostics)) => {
                    host::notify(
                        NotificationLevel::Info,
                        TITLE,
                        &format!(
                            "{}\n{message}\n\n{}",
                            strings.test_ok,
                            diagnostics.summary()
                        ),
                    );
                    Ok(None)
                }
                Err(error) => Err(error),
            },
            _ => Err(Error::NotFound(format!("unknown command {id}"))),
        }
    }
}

register_plugin!(CommitMessageHelper);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::{CommitChange, CommitDraft, HttpResponse};

    fn draft() -> CommitDraft {
        CommitDraft {
            revision: 7,
            message: "old message".into(),
            amend: false,
            changes: vec![CommitChange {
                repository: "/repo".into(),
                path: "/repo/src/a.rs".into(),
                old_path: None,
                status: "modified".into(),
                diff: Some("@@ -1 +1 @@\n-old\n+new\n".into()),
                truncated: false,
            }],
        }
    }

    #[test]
    fn generate_writes_the_pending_commit_message() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("commitTypes", r#"["feat","fix"]"#);
        host.set_setting("language", "\"English\"");
        host.set_commit_draft(Some(draft()));
        host.on_fetch(|request| {
            assert!(request.url.ends_with("/chat/completions"));
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let updated = host.commit_draft().expect("the draft is still there");
        assert_eq!(updated.message, "fix: fix crash");
        assert_eq!(
            updated.revision, 8,
            "setting the message moves the revision"
        );
    }

    fn draft_with_message(message: &str) -> CommitDraft {
        let mut draft = draft();
        draft.message = message.into();
        draft
    }

    #[test]
    fn progress_fills_an_empty_message_field_then_replaces_it() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(draft_with_message("")));
        let draft = host.commit_draft().expect("the draft is there");

        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "Generating commit message…"
        );

        progress.finish("fix: crash").expect("the field takes it");
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "fix: crash"
        );
    }

    #[test]
    fn progress_keeps_a_typed_message_until_the_result_arrives() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(draft_with_message("my own words")));
        let draft = host.commit_draft().expect("the draft is there");

        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "my own words",
            "a filled field is never replaced by the placeholder"
        );

        progress.finish("fix: crash").expect("the field takes it");
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "fix: crash"
        );
    }

    #[test]
    fn clearing_progress_removes_only_its_own_placeholder() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(draft_with_message("")));
        let draft = host.commit_draft().expect("the draft is there");

        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
        progress.clear();
        assert_eq!(host.commit_draft().expect("still there").message, "");

        let draft = host.commit_draft().expect("the draft is there");
        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "Generating commit message…"
        );
        host.edit_commit_message("typed meanwhile");
        progress.clear();
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "typed meanwhile",
            "text typed during generation stays"
        );
    }

    #[test]
    fn generate_fills_an_empty_message_field() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("commitTypes", r#"["feat","fix"]"#);
        host.set_setting("language", "\"English\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "fix: fix crash"
        );
    }

    #[test]
    fn a_failed_generation_leaves_no_placeholder() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| Err(Error::Failed("network down".into())));

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(
            result.is_ok(),
            "the command reports the failure instead: {result:?}"
        );
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            ""
        );
    }

    #[test]
    fn generation_validates_commit_headers_before_writing() {
        let cases = [
            (r#"{"type":"fix"}"#, None),
            (r#"{"subject":"fix crash"}"#, None),
            (r#"{"type":"fix","subject":"  "}"#, None),
            (r#"{"type":"banana","subject":"fix crash"}"#, None),
            ("banana: fix crash", None),
            (r#"{"type":"banana","subject":"fix crash"#, None),
            (r#"{"type":"fix","subject":42}"#, None),
            (r#"{"type":"fix","subject":["fix crash"]}"#, None),
            (r#"{"type":"fix","subject":"fix\ncrash"}"#, None),
            (
                r#"{"type":"fix","scope":"core)\n","subject":"fix crash"}"#,
                None,
            ),
            (
                r#"{"type":"security","subject":"restrict access"}"#,
                Some("security: restrict access"),
            ),
            ("fix(core): fix crash", Some("fix(core): fix crash")),
            (
                r#"{"type":"fix","subject":"fix crash"#,
                Some("fix: fix crash"),
            ),
        ];
        for (raw, expected) in cases {
            for initial in ["old message", ""] {
                let host = MockHost::install().with_capabilities(&[
                    "vcs.read",
                    "vcs.write",
                    "secrets",
                    "net:api.deepseek.com",
                ]);
                host.set_secret("api-key", "test-key");
                host.set_setting("commitTypes", r#"["fix","security"]"#);
                host.set_commit_draft(Some(draft_with_message(initial)));
                host.on_fetch(move |_| {
                    Ok(HttpResponse {
                        status: 200,
                        headers: Vec::new(),
                        body: serde_json::to_vec(&serde_json::json!({
                            "choices": [{"message": {"content": raw}}]
                        }))
                        .unwrap(),
                    })
                });

                let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
                assert!(result.is_ok(), "{raw}: {result:?}");
                assert_eq!(
                    host.commit_draft()
                        .expect("the draft is still there")
                        .message,
                    expected.unwrap_or(initial),
                    "response: {raw}, initial message: {initial:?}"
                );
                assert_eq!(
                    host.notifications().last().unwrap().level,
                    if expected.is_some() {
                        NotificationLevel::Info
                    } else {
                        NotificationLevel::Error
                    },
                    "response: {raw}"
                );
            }
        }
    }

    #[test]
    fn test_command_checks_the_configured_endpoint() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|request| {
            assert!(request.url.ends_with("/chat/completions"));
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"OK"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("test", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let body = &shown.last().expect("the test reports").body;
        assert!(
            body.contains("HTTP 200 · deepseek-chat"),
            "the status and model are reported: {body}"
        );
        assert!(
            body.contains("https://api.deepseek.com/v1/chat/completions")
                && body.contains("max_tokens 16")
                && body.contains("1 attempt"),
            "the request that was sent is reported: {body}"
        );
    }

    /// The body of the request the mock receives, as text.
    fn request_body(request: &openidea_plugin::HttpRequest) -> String {
        String::from_utf8_lossy(request.body.as_deref().unwrap_or_default()).into_owned()
    }

    #[test]
    fn generate_with_context_sends_the_authors_requirements() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("")));
        host.answer_next_prompt(Some(&["the crash came from an empty queue; closes #42"]));
        host.on_fetch(|request| {
            let body = request_body(request);
            assert!(
                body.contains("the crash came from an empty queue; closes #42"),
                "the requirements reach the model: {body}"
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"stop the empty-queue crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate-with-context", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "fix: stop the empty-queue crash"
        );
    }

    #[test]
    fn generate_with_context_stops_when_the_dialog_is_dismissed() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("my own words")));
        host.answer_next_prompt(None);

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate-with-context", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.requests().len(),
            0,
            "nothing is sent without requirements"
        );
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "my own words"
        );
        assert_eq!(
            host.notifications().last().expect("it reports").level,
            NotificationLevel::Info
        );
    }

    #[test]
    fn format_rewrites_the_draft_in_place() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("fixed stuff and closes #7")));
        host.on_fetch(|request| {
            let body = request_body(request);
            assert!(
                body.contains("fixed stuff and closes #7"),
                "the draft being rewritten reaches the model: {body}"
            );
            assert!(body.contains("Existing Draft Commit Message"));
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: serde_json::to_vec(&serde_json::json!({
                    "choices": [{"message": {"content": "{\"type\":\"fix\",\"subject\":\"fix the empty queue\",\"closes\":\"#7\"}"}}]
                }))
                .unwrap(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("format", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let updated = host.commit_draft().expect("the draft is still there");
        assert_eq!(updated.message, "fix: fix the empty queue\n\nCloses #7");
        assert_eq!(
            updated.revision, 8,
            "the draft is replaced once, never blanked first"
        );
    }

    #[test]
    fn format_needs_a_draft_to_rewrite() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("")));

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("format", &host.invocation(), None);
        assert!(result.is_ok(), "the command reports instead: {result:?}");
        assert_eq!(host.requests().len(), 0, "an empty draft is not sent");
        assert_eq!(
            host.notifications().last().expect("it reports").level,
            NotificationLevel::Error
        );
    }

    #[test]
    fn a_refused_max_tokens_is_retried_as_max_completion_tokens() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body = request_body(request);
            if body.contains("\"max_tokens\"") {
                return Ok(HttpResponse {
                    status: 400,
                    headers: Vec::new(),
                    body: br#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}}"#.to_vec(),
                });
            }
            assert!(body.contains("\"max_completion_tokens\""), "{body}");
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"stop the crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.requests().len(), 2, "one refusal, one retry");
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "fix: stop the crash"
        );
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(
            body.contains("max_completion_tokens") && body.contains("adjusted: token parameter"),
            "the balloon says what had to change: {body}"
        );
    }

    #[test]
    fn a_refused_temperature_is_retried_without_it() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body = request_body(request);
            if body.contains("\"temperature\"") {
                return Ok(HttpResponse {
                    status: 400,
                    headers: Vec::new(),
                    body: br#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.5 with this model."}}"#.to_vec(),
                });
            }
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"stop the crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.requests().len(), 2, "one refusal, one retry");
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(
            body.contains("temperature omitted") && body.contains("adjusted: temperature"),
            "the balloon says what had to change: {body}"
        );
    }

    #[test]
    fn turning_temperature_off_in_settings_leaves_it_out_of_the_first_request() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("sendTemperature", "false");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            assert!(
                !request_body(request).contains("\"temperature\""),
                "a model that rejects temperature can be configured up front"
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"stop the crash\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.requests().len(), 1, "no refusal, so no retry");
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(body.contains("temperature omitted"), "{body}");
        assert!(!body.contains("adjusted:"), "{body}");
    }

    #[test]
    fn a_failure_the_request_cannot_adjust_is_not_retried() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("my own words")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 401,
                headers: Vec::new(),
                body: br#"{"error":{"message":"bad key"}}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "the command reports instead: {result:?}");
        assert_eq!(
            host.requests().len(),
            1,
            "an auth failure has nothing to adjust"
        );
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(body.contains("bad key"), "{body}");
    }

    fn models_host() -> MockHost {
        let host = MockHost::install().with_capabilities(&[
            "secrets",
            "clipboard",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|request| {
            assert_eq!(request.method, "GET");
            assert!(request.url.ends_with("/v1/models"), "{}", request.url);
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"data":[{"id":"deepseek-reasoner"},{"id":"deepseek-chat"}]}"#.to_vec(),
            })
        });
        host
    }

    #[test]
    fn the_models_command_copies_the_chosen_id_for_the_model_setting() {
        let host = models_host();
        host.answer_next_prompt(Some(&["deepseek-reasoner"]));

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("models", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.clipboard().as_deref(),
            Some("deepseek-reasoner"),
            "settings are read-only for a plugin, so the id is copied"
        );
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(body.contains("deepseek-reasoner"), "{body}");
    }

    #[test]
    fn a_dismissed_model_picker_changes_nothing() {
        let host = models_host();
        host.answer_next_prompt(None);

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("models", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.clipboard(), None);
        assert_eq!(
            host.notifications().last().expect("it reports").level,
            NotificationLevel::Info
        );
    }

    #[test]
    fn the_endpoint_command_shows_the_url_the_request_goes_to() {
        let host = MockHost::install().with_capabilities(&[]);

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("endpoint", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let body = &shown.last().expect("it reports").body;
        assert!(
            body.contains("https://api.deepseek.com/v1/chat/completions"),
            "the effective URL is shown: {body}"
        );
    }

    #[test]
    fn test_command_accepts_an_answer_truncated_by_the_token_budget() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        // A reasoning model spends the test's 16-token budget thinking and has
        // no content left: the connection, key and model still work.
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: serde_json::to_vec(&serde_json::json!({
                    "choices": [{
                        "finish_reason": "length",
                        "index": 0,
                        "message": {
                            "content": null,
                            "reasoning_content": "The user just said ping.",
                            "role": "assistant"
                        }
                    }],
                    "id": "chatcmpl-test",
                    "model": "a-reasoning-model",
                    "object": "chat.completion",
                    "usage": {"completion_tokens": 16}
                }))
                .unwrap(),
            })
        });

        let result = CommitMessageHelper.run_command("test", &host.invocation(), None);
        assert!(result.is_ok(), "the connection works: {result:?}");
        assert_eq!(
            host.notifications().last().unwrap().level,
            NotificationLevel::Info
        );
    }

    #[test]
    fn test_command_rejects_an_answer_that_is_not_json() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: b"<html>gateway</html>".to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("test", &host.invocation(), None);
        assert!(result.is_err(), "a non-JSON answer is not a passing test");
    }

    /// A model profile owns the endpoint, model, sampling, token limit and key
    /// slot; the single LLM settings above it are only the fallback.
    #[test]
    fn the_active_model_profile_drives_the_request() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
            "net:llm.example.com",
        ]);
        host.set_secret("api-key", "the-default-key");
        host.set_secret("api-key-2", "the-second-key");
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Work","provider":"Custom","protocol":"openai-chat","baseUrl":"https://llm.example.com/v1","model":"work-model","temperature":0.1,"sendTemperature":true,"maxTokens":2048,"secretSlot":"api-key-2"}]"#,
        );
        host.set_setting("activeModel", "\"Work\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            assert_eq!(
                request.url,
                "https://llm.example.com/v1/chat/completions",
                "the profile's base URL and protocol are used"
            );
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["model"], "work-model");
            assert_eq!(body["max_tokens"], 2048, "the profile's own token limit");
            assert_eq!(body["temperature"], 0.1);
            // The mock host fills the secret placeholder on the way out, as
            // the IDE does: this is slot 2's key, not the default slot's.
            assert!(
                request.headers.iter().any(|(name, value)| {
                    name == "Authorization" && value == "Bearer the-second-key"
                }),
                "the profile's own key slot is used: {:?}",
                request.headers
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.requests().len(), 1, "one request to the profile");
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
    }

    #[test]
    fn a_profile_needs_only_its_own_key_slot() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
            "net:llm.example.com",
        ]);
        // Deliberately no `api-key`: the active profile uses slot 2.
        host.set_secret("api-key-2", "the-second-key");
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Work","provider":"Custom","protocol":"openai-chat","baseUrl":"https://llm.example.com/v1","model":"work-model","temperature":0.5,"sendTemperature":true,"maxTokens":2048,"secretSlot":"api-key-2"}]"#,
        );
        host.set_setting("activeModel", "\"Work\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.requests().len(), 1, "the profile's key is enough");
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
    }

    #[test]
    fn a_named_prompt_profile_reaches_the_system_prompt() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "promptProfiles",
            r#"[{"name":"Team","text":"Always mention the ticket id in the body."}]"#,
        );
        host.set_setting("defaultPrompt", "\"Team\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert!(
                body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Always mention the ticket id in the body."),
                "the selected profile replaces the single extra prompt: {body}"
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
    }

    #[test]
    fn a_named_template_renders_the_message() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "templates",
            r#"[{"name":"Team","template":"{{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\n\nSigned-off-by: Team"}]"#,
        );
        host.set_setting("defaultTemplate", "\"Team\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: serde_json::to_vec(&serde_json::json!({
                    "choices": [{"message": {"content": "{\"type\":\"fix\",\"subject\":\"fix crash\",\"body\":\"one\\ntwo\",\"closes\":\"#7\"}"}}]
                }))
                .unwrap(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft().unwrap().message,
            "fix: fix crash\n\n- one\n- two\n\nCloses #7\n\nSigned-off-by: Team"
        );
    }

    #[test]
    fn an_invalid_template_falls_back_and_says_why() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("templates", r#"[{"name":"Team","template":"{{unknown}}"}]"#);
        host.set_setting("defaultTemplate", "\"Team\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
        let shown = host.notifications();
        let body = &shown.last().unwrap().body;
        assert!(body.contains("unknown template variable"), "{body}");
    }

    #[test]
    fn the_built_in_default_template_cannot_be_replaced() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "templates",
            r#"[{"name":"Default","template":"HACKED {{subject}}"}]"#,
        );
        host.set_setting("defaultTemplate", "\"Default\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let message = host.commit_draft().unwrap().message;
        assert_eq!(message, "fix: fix crash");
        assert!(!message.contains("HACKED"));
    }

    #[test]
    fn commit_type_descriptions_reach_the_prompt_and_validate() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "commitTypes",
            r#"[{"title":"security","description":"Security hardening"}]"#,
        );
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body = request_body(request);
            assert!(
                body.contains("Security hardening"),
                "the description helps the model choose: {body}"
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"security\",\"subject\":\"restrict access\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft().unwrap().message,
            "security: restrict access",
            "only the title is a valid type"
        );
    }

    #[test]
    fn an_unknown_profile_name_falls_back_to_the_single_settings() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("activeModel", "\"Does not exist\"");
        host.set_setting("defaultPrompt", "\"Does not exist\"");
        host.set_setting("defaultTemplate", "\"Does not exist\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            assert!(request.url.ends_with("/chat/completions"), "{}", request.url);
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
    }

    /// Checklist 11: the Preview action renders the template the user is
    /// editing, with example fields they can switch on and off.
    #[test]
    fn template_preview_renders_the_edited_template_with_chosen_fields() {
        let host = MockHost::install();
        host.set_setting(
            "templates",
            r#"[{"name":"Team","template":"{{header}} -- {{subject}}"}]"#,
        );
        host.set_setting("defaultTemplate", "\"Team\"");
        // Which example fields have values, then the edited template, then
        // dismiss the editor to leave the preview loop.
        host.answer_next_prompt(Some(&["type", "subject", "body"]));
        host.answer_next_prompt(Some(&[
            "{{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\n\nfooter",
        ]));
        host.answer_next_prompt(None);

        let result = CommitMessageHelper.run_command("template-preview", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("a preview is shown");
        assert_eq!(notice.level, NotificationLevel::Info, "{notice:?}");
        assert!(notice.body.contains("<type>: <subject>"), "{}", notice.body);
        assert!(notice.body.contains("<body>"), "{}", notice.body);
        assert!(notice.body.contains("footer"), "{}", notice.body);
        assert!(
            !notice.body.contains("Closes"),
            "an unselected field leaves its conditional section out: {}",
            notice.body
        );
    }

    /// Checklist 12: a broken template is reported while previewing, before
    /// any generation request falls back to Default.
    #[test]
    fn template_preview_reports_the_template_error() {
        let host = MockHost::install();
        host.answer_next_prompt(Some(&["type", "subject"]));
        host.answer_next_prompt(Some(&["{{unknown}}"]));
        host.answer_next_prompt(None);

        let result = CommitMessageHelper.run_command("template-preview", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the error is shown");
        assert_eq!(notice.level, NotificationLevel::Error, "{notice:?}");
        assert!(
            notice.body.contains("unknown template variable"),
            "{}",
            notice.body
        );
    }

    /// Checklist 12: variable help names every variable, the conditional
    /// syntax, the errors and the restore entry.
    #[test]
    fn template_help_lists_variables_and_the_restore_entry() {
        let host = MockHost::install();
        let result = CommitMessageHelper.run_command("template-help", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the help is shown");
        assert_eq!(notice.level, NotificationLevel::Info, "{notice:?}");
        for expected in [
            "{{type}}",
            "{{scope}}",
            "{{subject}}",
            "{{body}}",
            "{{changes}}",
            "{{closes}}",
            "{{skipCi}}",
            "{{header}}",
            "{{#body}}",
            "{{/body}}",
            "Restore Default",
        ] {
            assert!(
                notice.body.contains(expected),
                "{expected}: {}",
                notice.body
            );
        }
    }

    /// Checklist 27: the levels action reflects the mapping, not the full
    /// enum in the settings table.
    #[test]
    fn thinking_levels_action_lists_what_the_mapping_offers() {
        let host = MockHost::install();
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Reasoner","thinkingLevel":"ENABLED","thinkingPreset":"custom","thinkingMapping":"{\"ENABLED\":{\"enable_thinking\":true}}"}]"#,
        );
        host.set_setting("activeModel", "\"Reasoner\"");
        let result = CommitMessageHelper.run_command("thinking-levels", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the levels are shown");
        assert!(notice.body.contains("DEFAULT, ENABLED"), "{}", notice.body);
        assert!(!notice.body.contains(", LOW"), "{}", notice.body);
    }

    #[test]
    fn thinking_levels_action_warns_about_an_unavailable_level() {
        let host = MockHost::install();
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Reasoner","thinkingLevel":"LOW","thinkingPreset":"custom","thinkingMapping":"{\"ENABLED\":{\"enable_thinking\":true}}"}]"#,
        );
        host.set_setting("activeModel", "\"Reasoner\"");
        let result = CommitMessageHelper.run_command("thinking-levels", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the warning is shown");
        assert!(notice.body.contains("Model default"), "{}", notice.body);
    }

    /// Checklist 26: help explains the mapping, validates the current one and
    /// copies the preset template for restoring it.
    #[test]
    fn thinking_help_action_shows_the_template_and_the_mapping_error() {
        let host = MockHost::install().with_capabilities(&["clipboard"]);
        host.set_setting("thinkingPreset", "\"openai-effort\"");
        host.set_setting("thinkingMapping", "\"[]\"");
        let result = CommitMessageHelper.run_command("thinking-help", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert!(
            host.clipboard().unwrap().contains("reasoning_effort"),
            "the preset template is copied: {:?}",
            host.clipboard()
        );
        let shown = host.notifications();
        let notice = shown.last().expect("the help is shown");
        assert!(notice.body.contains("$thinking_budget"), "{}", notice.body);
        assert!(notice.body.contains("Mapping error"), "{}", notice.body);
    }

    /// Checklist 24/25: the active profile's level and preset reach the body.
    #[test]
    fn generate_sends_the_profiles_thinking_fragment() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Reasoner","thinkingLevel":"HIGH","thinkingPreset":"openai-effort"}]"#,
        );
        host.set_setting("activeModel", "\"Reasoner\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["reasoning_effort"], "high", "{body}");
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the diagnostics are shown");
        assert!(notice.body.contains("thinking: HIGH"), "{}", notice.body);
    }

    /// Checklist 27: a level the mapping does not define sends nothing.
    #[test]
    fn a_level_outside_the_mapping_sends_no_thinking_parameters() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Reasoner","thinkingLevel":"LOW","thinkingPreset":"custom","thinkingMapping":"{\"ENABLED\":{\"enable_thinking\":true}}"}]"#,
        );
        host.set_setting("activeModel", "\"Reasoner\"");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert!(body.get("enable_thinking").is_none(), "{body}");
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = host.notifications();
        let notice = shown.last().expect("the diagnostics are shown");
        assert!(notice.body.contains("LOW"), "{}", notice.body);
    }

    #[test]
    fn a_connection_test_keeps_the_profiles_valid_thinking_budget() {
        for (preset, mapping) in [
            ("anthropic-budget", None),
            ("auto", None),
            (
                "custom",
                Some(r#"{"HIGH":{"thinking":{"type":"enabled","budget_tokens":48000}}}"#),
            ),
        ] {
            let host = MockHost::install().with_capabilities(&["secrets", "net:api.anthropic.com"]);
            host.set_secret("api-key", "test-key");
            host.set_setting("provider", "\"Anthropic\"");
            host.set_setting("thinkingLevel", "\"HIGH\"");
            host.set_setting("thinkingPreset", &serde_json::to_string(preset).unwrap());
            if let Some(mapping) = mapping {
                host.set_setting("thinkingMapping", &serde_json::to_string(mapping).unwrap());
            }
            host.on_fetch(|request| {
                let body: serde_json::Value =
                    serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
                assert_eq!(body["thinking"]["budget_tokens"], 48_000, "{body}");
                assert_eq!(body["max_tokens"], 64_000, "{body}");
                Ok(HttpResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: br#"{"content":[]}"#.to_vec(),
                })
            });

            let result = CommitMessageHelper.run_command("test", &host.invocation(), None);
            assert!(result.is_ok(), "{preset}: {result:?}");
        }
    }

    #[test]
    fn an_empty_template_result_falls_back_to_a_generated_message() {
        for template in ["", " \n", "{{#closes}}{{closes}}{{/closes}}"] {
            let host = MockHost::install().with_capabilities(&[
                "vcs.read",
                "vcs.write",
                "secrets",
                "net:api.deepseek.com",
            ]);
            host.set_secret("api-key", "test-key");
            host.set_setting(
                "templates",
                &serde_json::json!([{"name": "Team", "template": template}]).to_string(),
            );
            host.set_setting("defaultTemplate", "\"Team\"");
            host.set_commit_draft(Some(draft()));
            host.on_fetch(|_| {
                Ok(HttpResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix crash\"}"}}]}"#.to_vec(),
                })
            });

            let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
            let shown = host.notifications();
            assert!(shown.last().unwrap().body.contains("Invalid template"));
        }
    }

    /// Checklist 12: Restore Default hands the built-in template to the user,
    /// because a plugin cannot write the declarative settings.
    #[test]
    fn restore_default_copies_the_built_in_template() {
        let host = MockHost::install().with_capabilities(&["clipboard"]);
        let result = CommitMessageHelper.run_command("template-default", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.clipboard().as_deref(), Some(prompt::DEFAULT_TEMPLATE));
        let shown = host.notifications();
        let notice = shown.last().expect("the template is shown");
        assert!(notice.body.contains("{{header}}"), "{}", notice.body);
    }
}
