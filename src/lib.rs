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

use config::{Actions, Config, UiLanguage};
use openidea_plugin::{Error, Invocation, NotificationLevel, Plugin, host, register_plugin};
use prompt::Task;

const TITLE: &str = "Git Commit Message Helper";

/// Shows a notification in the window of the invoking project. A settings
/// page action runs in the settings window, which shows no plugin
/// notifications (OpenIDEA 0.1.41), so the project's own window gets it;
/// without a project it goes to the invoking window.
fn notify(ctx: &Invocation, level: NotificationLevel, body: &str) {
    if let Some(project) = ctx.project()
        && project.notify(level, TITLE, body, &[]).is_ok()
    {
        return;
    }
    host::notify(level, TITLE, body);
}

/// Writes the two button settings into the context keys `plugin.toml`'s `when`
/// clauses read (checklist 6). Called at activation and whenever a setting
/// changes, so a hidden button is hidden before the first command runs too.
fn sync_action_context() {
    for (key, visible) in Actions::load().context_keys() {
        // A key is `plugin.<id>.<key>`; without a window it applies to all.
        let _ = host::set_context(key, visible);
    }
}

#[derive(Default)]
pub struct CommitMessageHelper;

/// What one run asks the model for.
enum Action {
    Generate,
    WithRequirements(String),
}

/// A generated message and the request that produced it.
struct Generated {
    message: String,
    diagnostics: llm::Diagnostics,
    /// Why the built-in Default had to be used instead of a named template.
    template_error: Option<String>,
    /// What the local quality check found (checklist 32); reported, never used
    /// to rewrite the message.
    quality: Vec<prompt::QualityIssue>,
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
        if !self.quality.is_empty() {
            let found = self
                .quality
                .iter()
                .map(|issue| issue.text(strings))
                .collect::<Vec<_>>()
                .join(strings.list_separator);
            body.push_str(&format!("\n\n{}: {found}", strings.quality_prefix));
        }
        if let Some(error) = &self.template_error {
            body.push_str(&format!("\n\n{} {error}", strings.template_fallback));
        }
        body
    }
}

/// The file names and stems the commit touches: a subject that only repeats one
/// of them says nothing about the change.
fn changed_names(draft: &openidea_plugin::CommitDraft) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for change in &draft.changes {
        let path = change
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&change.path);
        let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
        for name in [path, stem] {
            if !name.is_empty() && !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

impl CommitMessageHelper {
    fn generate(&mut self, ctx: &Invocation, action: Action) -> Result<Generated, Error> {
        let strings = ui::strings(UiLanguage::load());
        let config = Config::load();
        if !host::secret_is_set(config::SECRET) {
            return Err(Error::Failed(format!(
                "{} {}: {}",
                strings.not_configured,
                strings.key_slot,
                config::SECRET
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
                quality: prompt::quality_issues(&fields, &changed_names(&draft)),
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

    /// Shows the outcome of a command that reports through notifications.
    fn report(
        ctx: &Invocation,
        strings: &ui::Strings,
        result: Result<String, Error>,
    ) -> Result<Option<String>, Error> {
        match result {
            Ok(body) => notify(ctx, NotificationLevel::Info, &body),
            Err(Error::Cancelled) => notify(ctx, NotificationLevel::Info, strings.canceled),
            Err(Error::Stale) => notify(ctx, NotificationLevel::Warning, strings.stale),
            Err(error) => notify(
                ctx,
                NotificationLevel::Error,
                &format!("{}: {error}", strings.error_prefix),
            ),
        }
        Ok(None)
    }
}

/// The text written into the message field while the model answers, so the
/// Commit panel shows the command was taken. The result replaces it; a failed
/// or cancelled run puts back what the field held before. A newer revision
/// (typing during the run) is never overwritten.
struct Progress {
    revision: u64,
    text: &'static str,
    /// What the field held before the placeholder.
    original: String,
    shown: bool,
}

impl Progress {
    fn start(strings: &ui::Strings, draft: &openidea_plugin::CommitDraft) -> Self {
        let mut progress = Self {
            revision: draft.revision,
            text: strings.generating_now,
            original: draft.message.clone(),
            shown: false,
        };
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

    /// Puts the earlier text back, but only while the field still holds the
    /// placeholder.
    fn clear(&self) {
        if !self.shown {
            return;
        }
        if host::set_commit_message(self.revision, &self.original).is_ok() {
            return;
        }
        if let Ok(Some(draft)) = host::pending_commit()
            && draft.message.trim() == self.text
        {
            let _ = host::set_commit_message(draft.revision, &self.original);
        }
    }
}

impl Plugin for CommitMessageHelper {
    fn activate(&mut self, ctx: &openidea_plugin::Context) -> Result<(), Error> {
        let _ = ctx;
        // The buttons are gated by context keys, so a settings change has to
        // reach a live instance; `on-startup-finished` sets them once.
        host::subscribe(openidea_plugin::EventKind::SettingsChanged)?;
        sync_action_context();
        Ok(())
    }

    fn handle_events(&mut self, events: &[openidea_plugin::Event], _ctx: &Invocation) {
        let settings_changed = events.iter().any(|event| {
            matches!(event, openidea_plugin::Event::Other { kind, .. } if kind == "settings-changed")
        });
        if settings_changed {
            sync_action_context();
        }
    }

    fn run_command(
        &mut self,
        id: &str,
        ctx: &Invocation,
        _arg: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let strings = ui::strings(UiLanguage::load());
        match id {
            "generate" => Self::report(
                ctx,
                strings,
                self.generate(ctx, Action::Generate)
                    .map(|g| g.notification(strings)),
            ),
            "generate-with-context" => {
                // The dialog is what differs from `generate`: requirements the
                // author writes for this commit only.
                let requirements = match host::input_multiline(strings.requirements_prompt, None) {
                    Ok(Some(text)) if !text.trim().is_empty() => text,
                    Ok(_) => return Self::report(ctx, strings, Err(Error::Cancelled)),
                    Err(error) => return Self::report(ctx, strings, Err(error)),
                };
                Self::report(
                    ctx,
                    strings,
                    self.generate(ctx, Action::WithRequirements(requirements))
                        .map(|g| g.notification(strings)),
                )
            }
            "template-help" => Self::report(ctx, strings, Ok(strings.template_help.to_owned())),
            "template-default" => Self::report(ctx, strings, self.restore_default(strings)),
            "test" => match self.test_connection() {
                Ok((message, diagnostics)) => {
                    notify(
                        ctx,
                        NotificationLevel::Info,
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

    /// The notifications the project's window got, oldest first.
    fn shown_in_project(host: &MockHost) -> Vec<openidea_plugin::mock::Notification> {
        host.project_notifications()
            .into_iter()
            .map(|(_, sent)| sent.notification)
            .collect()
    }

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
    fn the_two_buttons_follow_their_settings() {
        let host = MockHost::install().with_capabilities(&["events"]);
        let mut plugin = CommitMessageHelper;
        plugin
            .activate(&host.context("qszfy666.commit-message-helper"))
            .expect("activation subscribes to settings changes");
        let keys = host.context_keys();
        for key in ["actionGenerate", "actionGenerateWithContext"] {
            assert_eq!(
                keys.get(key),
                Some(&openidea_plugin::ContextValue::Flag(true)),
                "{key} is visible by default"
            );
        }

        host.set_setting("actionGenerateWithContext", "false");
        plugin.handle_events(
            &[openidea_plugin::Event::Other {
                kind: "settings-changed".into(),
                data: r#"["actionGenerateWithContext"]"#.into(),
            }],
            &host.invocation(),
        );
        let keys = host.context_keys();
        assert_eq!(
            keys.get("actionGenerateWithContext"),
            Some(&openidea_plugin::ContextValue::Flag(false)),
            "the setting reaches the context key the menu reads"
        );
        assert_eq!(
            keys.get("actionGenerate"),
            Some(&openidea_plugin::ContextValue::Flag(true)),
            "the other button stays"
        );
    }

    #[test]
    fn removed_commands_are_unknown() {
        let host = MockHost::install();
        for id in [
            "format",
            "manual",
            "models",
            "endpoint",
            "thinking-levels",
            "thinking-help",
            "template-preview",
        ] {
            let result = CommitMessageHelper.run_command(id, &host.invocation(), None);
            assert!(
                matches!(result, Err(Error::NotFound(_))),
                "{id}: {result:?}"
            );
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
            updated.revision, 9,
            "the placeholder and the result each move the revision once"
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
    fn progress_replaces_a_filled_field_and_restores_it_when_cleared() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(draft_with_message("my own words")));
        let draft = host.commit_draft().expect("the draft is there");

        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "Generating commit message…",
            "the placeholder shows the command runs, whatever the field held"
        );
        progress.clear();
        assert_eq!(
            host.commit_draft().expect("still there").message,
            "my own words",
            "a failed run gives the earlier text back"
        );

        let draft = host.commit_draft().expect("the draft is there");
        let progress = Progress::start(ui::strings(UiLanguage::En), &draft);
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
    fn a_failed_generation_gives_back_the_text_that_was_there() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_commit_draft(Some(draft_with_message("feat(math): earlier result")));
        host.on_fetch(|_| Err(Error::Failed("network down".into())));

        let result = CommitMessageHelper.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "feat(math): earlier result"
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
                    shown_in_project(&host).last().unwrap().level,
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
        let shown = shown_in_project(&host);
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

    /// A settings page action runs in the settings window, which shows no
    /// plugin notifications: the result goes to the project's own window.
    #[test]
    fn the_test_result_reaches_the_project_window() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_project_root(Some("/repo"));
        host.set_secret("api-key", "test-key");
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"OK"}}]}"#.to_vec(),
            })
        });

        let result = CommitMessageHelper.run_command("test", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let sent = host.project_notifications();
        let (root, notice) = sent.last().expect("the project window gets the result");
        assert_eq!(root, "/repo");
        assert!(
            notice.notification.body.contains("HTTP 200"),
            "{}",
            notice.notification.body
        );
        assert!(
            host.notifications().is_empty(),
            "nothing goes to the invoking window"
        );
    }

    #[test]
    fn page_action_results_reach_the_project_window() {
        let host = MockHost::install();
        host.set_project_root(Some("/repo"));
        let result = CommitMessageHelper.run_command("template-help", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(host.project_notifications().len(), 1);
        assert!(host.notifications().is_empty());
    }

    #[test]
    fn without_a_project_the_invoking_window_gets_the_result() {
        let host = MockHost::install();
        host.set_project_root(None);
        let result = CommitMessageHelper.run_command("template-help", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert!(host.project_notifications().is_empty());
        assert_eq!(host.notifications().len(), 1);
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
            shown_in_project(&host).last().expect("it reports").level,
            NotificationLevel::Info
        );
    }

    #[test]
    fn a_weak_subject_is_reported_but_still_written() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("commitTypes", r#"["fix"]"#);
        host.set_setting("language", "\"English\"");
        host.set_locale("en");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"{\"type\":\"fix\",\"subject\":\"fix bug\"}"}}]}"#.to_vec(),
            })
        });

        let mut plugin = CommitMessageHelper;
        let result = plugin.run_command("generate", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            host.commit_draft()
                .expect("the draft is still there")
                .message,
            "fix: fix bug",
            "the quality check reports, it does not hold the message back"
        );
        let shown = shown_in_project(&host);
        let body = &shown.last().expect("it reports").body;
        assert!(
            body.contains("Quality check") && body.contains("shorter than 12 columns"),
            "the finding reaches the balloon: {body}"
        );
        assert_eq!(
            host.requests().len(),
            1,
            "no second request: rewriting a weak subject is checklist 33"
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
        let shown = shown_in_project(&host);
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
        let shown = shown_in_project(&host);
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
        let shown = shown_in_project(&host);
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
        let shown = shown_in_project(&host);
        let body = &shown.last().expect("it reports").body;
        assert!(body.contains("bad key"), "{body}");
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
            shown_in_project(&host).last().unwrap().level,
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

    /// The LLM settings own the endpoint, model, sampling and token limit.
    #[test]
    fn the_llm_settings_drive_the_request() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
            "net:llm.example.com",
        ]);
        host.set_secret("api-key", "the-key");
        host.set_setting("provider", "\"Custom\"");
        host.set_setting("protocol", "\"openai-chat\"");
        host.set_setting("baseUrl", "\"https://llm.example.com/v1\"");
        host.set_setting("model", "\"work-model\"");
        host.set_setting("temperature", "0.1");
        host.set_setting("maxTokens", "2048");
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            assert_eq!(
                request.url,
                "https://llm.example.com/v1/chat/completions",
                "the base URL and protocol are used"
            );
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["model"], "work-model");
            assert_eq!(body["max_tokens"], 2048);
            assert_eq!(body["temperature"], 0.1);
            // The mock host fills the secret placeholder on the way out, as
            // the IDE does.
            assert!(
                request.headers.iter().any(|(name, value)| {
                    name == "Authorization" && value == "Bearer the-key"
                }),
                "{:?}",
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
        assert_eq!(host.requests().len(), 1);
        assert_eq!(host.commit_draft().unwrap().message, "fix: fix crash");
    }

    #[test]
    fn the_extra_prompt_reaches_the_system_prompt() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting(
            "extraPrompt",
            "\"Always mention the ticket id in the body.\"",
        );
        host.set_commit_draft(Some(draft_with_message("")));
        host.on_fetch(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            assert!(
                body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Always mention the ticket id in the body."),
                "{body}"
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
        let shown = shown_in_project(&host);
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
    fn an_unknown_template_name_falls_back_to_the_built_in_default() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
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

    /// Checklist 12: variable help names every variable, the conditional
    /// syntax, the errors and the restore entry.
    #[test]
    fn template_help_lists_variables_and_the_restore_entry() {
        let host = MockHost::install();
        let result = CommitMessageHelper.run_command("template-help", &host.invocation(), None);
        assert!(result.is_ok(), "{result:?}");
        let shown = shown_in_project(&host);
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

    /// Checklist 24/25: the thinking level and preset reach the body.
    #[test]
    fn generate_sends_the_thinking_fragment() {
        let host = MockHost::install().with_capabilities(&[
            "vcs.read",
            "vcs.write",
            "secrets",
            "net:api.deepseek.com",
        ]);
        host.set_secret("api-key", "test-key");
        host.set_setting("thinkingLevel", "\"HIGH\"");
        host.set_setting("thinkingPreset", "\"openai-effort\"");
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
        let shown = shown_in_project(&host);
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
        host.set_setting("thinkingLevel", "\"LOW\"");
        host.set_setting("thinkingPreset", "\"custom\"");
        host.set_setting(
            "thinkingMapping",
            &serde_json::to_string(r#"{"ENABLED":{"enable_thinking":true}}"#).unwrap(),
        );
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
        let shown = shown_in_project(&host);
        let notice = shown.last().expect("the diagnostics are shown");
        assert!(notice.body.contains("LOW"), "{}", notice.body);
    }

    #[test]
    fn a_connection_test_keeps_a_valid_thinking_budget() {
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
            let shown = shown_in_project(&host);
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
        let shown = shown_in_project(&host);
        let notice = shown.last().expect("the template is shown");
        assert!(notice.body.contains("{{header}}"), "{}", notice.body);
    }
}
