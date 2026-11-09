//! The changes the Commit panel would commit, as prompt context.
//!
//! OpenIDEA (plugin API 1.3) supplies the pending commit through
//! `host::pending_commit()`: the included files and changed blocks only, with
//! their unified diffs. The plugin no longer runs `git` itself, so it needs no
//! `process:git` grant.

use openidea_plugin::{CommitChange, CommitDraft, Project};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitContext {
    pub root: String,
    pub branch: String,
    pub status_lines: Vec<String>,
    pub diff: String,
}

fn relative_path<'a>(path: &'a str, repository: &str) -> &'a str {
    path.strip_prefix(repository.trim_end_matches('/'))
        .map(|path| path.trim_start_matches('/'))
        .unwrap_or(path)
}

fn change_label(change: &CommitChange) -> String {
    let mut label = format!(
        "[{}] {}",
        change.status,
        relative_path(&change.path, &change.repository)
    );
    if let Some(old) = &change.old_path {
        label.push_str(" ← ");
        label.push_str(relative_path(old, &change.repository));
    }
    if change.truncated {
        label.push_str(" (diff truncated by the IDE)");
    }
    label
}

/// Builds the prompt context from the commit panel's pending draft.
pub fn build_context(
    project: Option<&Project>,
    draft: &CommitDraft,
    max_diff_chars: usize,
) -> GitContext {
    let status = project.and_then(|project| project.vcs_status().ok().flatten());
    let root = status
        .as_ref()
        .map(|status| status.root.clone())
        .filter(|root| !root.is_empty())
        .unwrap_or_else(|| {
            let mut roots: Vec<String> = Vec::new();
            for change in &draft.changes {
                if !roots.contains(&change.repository) {
                    roots.push(change.repository.clone());
                }
            }
            roots.join(", ")
        });
    let branch = status
        .as_ref()
        .and_then(|status| status.branch.clone())
        .unwrap_or_default();
    let status_lines = draft.changes.iter().map(change_label).collect();

    let mut diff = String::new();
    for change in &draft.changes {
        if let Some(text) = &change.diff
            && !text.trim().is_empty()
        {
            diff.push_str("### ");
            diff.push_str(relative_path(&change.path, &change.repository));
            diff.push('\n');
            diff.push_str(text.trim_end());
            diff.push('\n');
        } else if change.status != "deleted" {
            diff.push_str("### ");
            diff.push_str(relative_path(&change.path, &change.repository));
            diff.push_str(" (no textual diff)\n");
        }
    }

    GitContext {
        root,
        branch,
        status_lines,
        diff: trim_diff(&diff, max_diff_chars),
    }
}

fn is_diff_header(line: &str) -> bool {
    line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
        || line.starts_with("new file mode")
        || line.starts_with("deleted file mode")
        || line.starts_with("old mode")
        || line.starts_with("new mode")
        || line.starts_with("similarity index")
        || line.starts_with("rename from")
        || line.starts_with("rename to")
        || line.starts_with("### ")
}

/// Caps a unified diff: every file header is kept, but each hunk keeps at most
/// 24 changed lines and 4 context lines, and the whole text stops at
/// `max_chars` with an explicit "omitted" marker. This mirrors the reference
/// plugin's behaviour and keeps prompts affordable on large diffs.
pub fn trim_diff(diff: &str, max_chars: usize) -> String {
    if diff.chars().count() <= max_chars {
        return diff.trim_end().to_owned();
    }

    let mut out = String::new();
    let mut omitted = 0usize;
    let mut changed_in_hunk = 0usize;
    let mut context_in_hunk = 0usize;

    for line in diff.lines() {
        let keep = if line.starts_with("@@") {
            changed_in_hunk = 0;
            context_in_hunk = 0;
            true
        } else if is_diff_header(line) {
            true
        } else if line.starts_with('+') || line.starts_with('-') {
            if changed_in_hunk < 24 {
                changed_in_hunk += 1;
                true
            } else {
                omitted += 1;
                false
            }
        } else if context_in_hunk < 4 {
            context_in_hunk += 1;
            true
        } else {
            omitted += 1;
            false
        };

        if keep {
            if out.chars().count() + line.chars().count() + 1 > max_chars {
                omitted += 1;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
    }

    if omitted > 0 {
        let note = format!("...[diff summarized: omitted {omitted} lines]");
        if out.chars().count() + note.chars().count() < max_chars {
            out.push_str(&note);
            out.push('\n');
        }
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::RepositoryStatus;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn builds_context_from_the_pending_commit() {
        let host = MockHost::install().with_capabilities(&["vcs.read"]);
        host.set_vcs_status(Some(RepositoryStatus {
            root: "/repo".into(),
            branch: Some("main".into()),
            ..RepositoryStatus::default()
        }));
        let draft = CommitDraft {
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
        };

        let project = host.invocation().project().unwrap();
        let context = build_context(Some(&project), &draft, 12_000);
        assert_eq!(context.root, "/repo");
        assert_eq!(context.branch, "main");
        assert_eq!(context.status_lines, vec!["[modified] src/a.rs"]);
        assert!(context.diff.contains("### src/a.rs"));
        assert!(context.diff.contains("+new"));
    }

    #[test]
    fn keeps_small_diffs_whole() {
        let diff = "diff --git a/a b/a\nindex 1..2\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n";
        assert_eq!(trim_diff(diff, 1000), diff.trim_end());
    }

    #[test]
    fn summarizes_large_hunks() {
        let mut diff = String::from("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,100 +1,100 @@\n");
        for i in 0..100 {
            diff.push_str(&format!("-line {i}\n+changed {i}\n"));
        }
        let trimmed = trim_diff(&diff, 600);
        assert!(trimmed.contains("@@"));
        assert!(trimmed.contains("diff --git"));
        assert!(trimmed.contains("omitted"));
        assert!(trimmed.len() <= 600, "got {} chars", trimmed.len());
        assert!(trimmed.lines().filter(|l| l.starts_with('-')).count() <= 24);
    }
}
