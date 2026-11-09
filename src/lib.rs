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
use openidea_plugin::{Error, Invocation, NotificationLevel, Plugin, host, register_plugin};

const TITLE: &str = "Git Commit Message Helper";

#[derive(Default)]
pub struct CommitMessageHelper;

impl CommitMessageHelper {
    fn generate(&mut self, ctx: &Invocation) -> Result<String, Error> {
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

        // The message field is the only place the Commit panel can show that the
        // command runs: plugin API 1.3 has no other slot next to the button.
        let progress = Progress::start(strings, &draft);
        let generated: Result<String, Error> = (|| {
            task.report(20, Some(100), Some(strings.generating));
            let project = ctx.project();
            let context = git::build_context(project.as_ref(), &draft, config.max_diff_chars);
            let (system, user) = prompt::build_prompts(&config, &context);
            let raw = llm::generate(&config, &system, &user)?;
            if task.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let fields = prompt::parse_response(&raw, &config.commit_types)
                .ok_or_else(|| Error::Failed(strings.bad_response.to_owned()))?;
            Ok(prompt::render(&fields))
        })();
        let message = match generated {
            Ok(message) => message,
            Err(error) => {
                progress.clear();
                return Err(error);
            }
        };

        progress.finish(&message)?;
        task.report(100, Some(100), Some(strings.generated));
        Ok(message)
    }

    fn test_connection(&mut self) -> Result<String, Error> {
        let strings = ui::strings(UiLanguage::load());
        let task = host::begin_task("Test LLM Connection", true);
        task.report(0, None, Some(strings.generating));
        let config = Config::load();
        let result = llm::test(&config)?;
        task.report(100, Some(100), Some(strings.test_ok));
        Ok(result)
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
            "generate" => match self.generate(ctx) {
                Ok(message) => {
                    host::notify(
                        NotificationLevel::Info,
                        TITLE,
                        &format!("{}\n\n{}", strings.generated, ui::clip(&message, 800)),
                    );
                    Ok(None)
                }
                Err(Error::Cancelled) => {
                    host::notify(NotificationLevel::Info, TITLE, strings.canceled);
                    Ok(None)
                }
                Err(Error::Stale) => {
                    host::notify(NotificationLevel::Warning, TITLE, strings.stale);
                    Ok(None)
                }
                Err(error) => {
                    host::notify(
                        NotificationLevel::Error,
                        TITLE,
                        &format!("{}: {error}", strings.error_prefix),
                    );
                    Ok(None)
                }
            },
            "test" => match self.test_connection() {
                Ok(message) => {
                    host::notify(
                        NotificationLevel::Info,
                        TITLE,
                        &format!("{}\n{message}", strings.test_ok),
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
