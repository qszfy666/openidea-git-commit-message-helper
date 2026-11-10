//! The changes the Commit panel would commit, as prompt context.
//!
//! OpenIDEA (plugin API 1.4) supplies the pending commit through
//! `host::pending_commit()`: the included files and changed blocks only, with
//! their unified diffs. The plugin no longer runs `git` itself, so it needs no
//! `process:git` grant.

use openidea_plugin::{CommitChange, CommitDraft, Project};

/// How many subjects of the repository's own history the prompt reads as a
/// style reference.
const RECENT_SUBJECTS: u32 = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitContext {
    pub root: String,
    pub branch: String,
    pub status_lines: Vec<String>,
    /// Newest first, for the model to match the repository's own style.
    pub recent_subjects: Vec<String>,
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
    // A missing log (no commit yet, an untrusted project) only costs the style
    // reference; the generation itself does not depend on it.
    let recent_subjects = project
        .and_then(|project| project.vcs_log(None, RECENT_SUBJECTS).ok())
        .map(|commits| {
            commits
                .into_iter()
                .map(|commit| commit.subject.trim().to_owned())
                .filter(|subject| !subject.is_empty())
                .collect()
        })
        .unwrap_or_default();

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
        recent_subjects,
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

/// Summarizes diffs exceeding `max_chars`, reserving room for an omission marker.
/// Each hunk keeps up to 12 added lines, 12 removed lines and 4 context lines.
/// File and hunk headers are retained while the character budget permits.
pub fn trim_diff(diff: &str, max_chars: usize) -> String {
    if diff.chars().count() <= max_chars {
        return diff.trim_end().to_owned();
    }

    let note_budget = format!(
        "...[diff summarized: omitted {} lines]",
        diff.lines().count()
    )
    .len();
    let content_budget = max_chars.saturating_sub(note_budget);
    let mut out = String::new();
    let mut omitted = 0usize;
    let mut added_in_hunk = 0usize;
    let mut removed_in_hunk = 0usize;
    let mut context_in_hunk = 0usize;

    for line in diff.lines() {
        let keep = if line.starts_with("@@") {
            added_in_hunk = 0;
            removed_in_hunk = 0;
            context_in_hunk = 0;
            true
        } else if is_diff_header(line) {
            true
        } else if line.starts_with('+') || line.starts_with('-') {
            let changed_in_hunk = if line.starts_with('+') {
                &mut added_in_hunk
            } else {
                &mut removed_in_hunk
            };
            if *changed_in_hunk < 12 {
                *changed_in_hunk += 1;
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
            if out.chars().count() + line.chars().count() + 1 > content_budget {
                omitted += 1;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
    }

    if omitted > 0 {
        let mut note = format!("...[diff summarized: omitted {omitted} lines]");
        note.truncate(max_chars);
        out.push_str(&note);
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::RepositoryStatus;
    use openidea_plugin::mock::MockHost;

    fn commit(subject: &str) -> openidea_plugin::CommitInfo {
        openidea_plugin::CommitInfo {
            id: format!("id-{subject}"),
            short_id: "1234567".into(),
            author: "Ann".into(),
            author_email: "ann@example.com".into(),
            date: 0,
            subject: subject.into(),
            body: String::new(),
            parents: Vec::new(),
        }
    }

    #[test]
    fn builds_context_from_the_pending_commit() {
        let host = MockHost::install().with_capabilities(&["vcs.read"]);
        host.set_vcs_status(Some(RepositoryStatus {
            root: "/repo".into(),
            branch: Some("main".into()),
            ..RepositoryStatus::default()
        }));
        host.set_vcs_reads(
            vec![
                commit("feat(parser): read the whole file"),
                commit("docs: explain the flags"),
            ],
            "/repo/src/a.rs",
            Vec::new(),
            None,
        );
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
        assert_eq!(
            context.recent_subjects,
            vec![
                "feat(parser): read the whole file",
                "docs: explain the flags"
            ]
        );
        assert!(context.diff.contains("### src/a.rs"));
        assert!(context.diff.contains("+new"));
    }

    #[test]
    fn a_restricted_project_still_builds_the_context_without_history() {
        let host = MockHost::install().with_capabilities(&["vcs.read"]);
        host.set_restricted(true);
        host.set_vcs_reads(vec![commit("feat: hidden")], "/repo/a.rs", Vec::new(), None);
        let draft = CommitDraft {
            revision: 1,
            message: String::new(),
            amend: false,
            changes: vec![CommitChange {
                repository: "/repo".into(),
                path: "/repo/a.rs".into(),
                old_path: None,
                status: "modified".into(),
                diff: Some("@@ -1 +1 @@\n-a\n+b\n".into()),
                truncated: false,
            }],
        };

        let project = host.invocation().project().unwrap();
        let context = build_context(Some(&project), &draft, 12_000);
        assert!(context.recent_subjects.is_empty());
        assert!(context.diff.contains("+b"), "the diff is still sent");
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

    #[test]
    fn large_replacements_keep_both_removed_and_added_lines() {
        let mut diff = String::from("diff --git a/a b/a\n--- a/a\n+++ b/a\n");
        for hunk in 0..2 {
            diff.push_str(&format!(
                "@@ -{},50 +{},50 @@\n",
                hunk * 50 + 1,
                hunk * 50 + 1
            ));
            for i in 0..50 {
                diff.push_str(&format!("-old_{hunk}_{i} calls old_api\n"));
            }
            for i in 0..50 {
                diff.push_str(&format!("+new_{hunk}_{i} calls new_api\n"));
            }
        }
        let trimmed = trim_diff(&diff, 3000);
        for hunk in 0..2 {
            assert_eq!(
                trimmed
                    .lines()
                    .filter(|line| line.starts_with(&format!("-old_{hunk}_")))
                    .count(),
                12
            );
            assert_eq!(
                trimmed
                    .lines()
                    .filter(|line| line.starts_with(&format!("+new_{hunk}_")))
                    .count(),
                12
            );
        }
        assert!(trimmed.contains("omitted"));
        assert!(trimmed.chars().count() <= 3000);
    }

    #[test]
    fn reserves_space_for_the_omission_marker() {
        for character in ['x', '新'] {
            let mut diff = String::from(
                "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -0,0 +1,12 @@\n",
            );
            for i in 0..11 {
                diff.push_str(&format!(
                    "+line_{i:02} {}\n",
                    character.to_string().repeat(164)
                ));
            }
            diff.push_str(&format!("+{}\n", character.to_string().repeat(500)));
            let trimmed = trim_diff(&diff, 2000);
            assert!(trimmed.contains("omitted"), "truncation must be marked");
            assert!(trimmed.ends_with("lines]"));
            assert!(trimmed.chars().count() <= 2000);
        }
    }
}
