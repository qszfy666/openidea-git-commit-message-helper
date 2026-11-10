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
}

impl Generated {
    /// The balloon: the message, then what the request carried.
    fn notification(&self, strings: &ui::Strings) -> String {
        format!(
            "{}\n\n{}\n\n{}",
            strings.generated,
            ui::clip(&self.message, 800),
            self.diagnostics.summary()
        )
    }
}

impl CommitMessageHelper {
    fn generate(&mut self, ctx: &Invocation, action: Action) -> Result<Generated, Error> {
        let strings = ui::strings(UiLanguage::load());
        if !host::secret_is_set("api-key") {
            return Err(Error::Failed(strings.not_configured.to_owned()));
        }
        let config = Config::load();
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

            let fields = prompt::parse_response(&answer.text, &config.commit_types)
                .ok_or_else(|| Error::Failed(strings.bad_response.to_owned()))?;
            Ok(Generated {
                message: prompt::render(&fields),
                diagnostics: answer.diagnostics,
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
}
