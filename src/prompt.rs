//! Prompt construction, response parsing and local rendering of the commit
//! message.
//!
//! Like the reference IntelliJ plugin, the model returns *fields* as JSON and
//! the plugin renders the final Conventional Commit text itself. That keeps
//! the formatting consistent regardless of the model's mood.

use crate::config::Config;
use crate::git::GitContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitFields {
    #[serde(rename = "type", default)]
    pub commit_type: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub changes: String,
    #[serde(default)]
    pub closes: String,
    #[serde(rename = "skipCi", default)]
    pub skip_ci: String,
}

const SYSTEM_PROMPT: &str = "You are a senior engineer and Git maintainer. Analyze the selected changes carefully, identify the primary intent, and fill the commit template fields. Keep your analysis internal and return only the requested JSON.";

/// What the command asks the model to do with the diff and the author's own
/// text.
#[derive(Clone, Copy, Debug)]
pub enum Task<'a> {
    /// Write the fields from the selected changes alone.
    Generate,
    /// Write the fields, following requirements the author typed for this
    /// commit (business background, a bug report, an issue, Skip CI).
    Requirements(&'a str),
    /// Rewrite the draft message already in the commit field into the fields.
    Format(&'a str),
}

const ANALYSIS_INSTRUCTIONS: &str = "Internal Analysis Instructions (do not output this analysis):\n\
1. Inspect the selected file status, the diff hunks and the tests.\n\
2. Identify the primary user-visible or technical intent, not just the changed filenames.\n\
3. Separate the main change from incidental formatting, generated files, lockfiles, and mechanical churn.\n\
4. Choose one type from the allowed types below.\n\
5. Keep `scope` a short lowercase identifier for the affected area; leave it empty when the change is not confined to one.";

fn output_contract(config: &Config, language: &str) -> String {
    format!(
        "JSON Output Contract:\n\
1. Return valid JSON only; no markdown fences, labels, explanation, or analysis.\n\
2. Use this exact shape: {{\"type\":\"\",\"scope\":\"\",\"subject\":\"\",\"body\":\"\",\"changes\":\"\",\"closes\":\"\",\"skipCi\":\"\"}}.\n\
3. The plugin renders these fields with a Conventional Commits template; do not include template syntax in the values.\n\
4. Keep `type` and `scope` as lowercase English identifiers; write `subject`, `body` and `changes` in {language}.\n\
5. `subject` must be concrete, imperative, under 72 characters, and must not end with punctuation.\n\
6. Do not use vague subjects like \"update files\", \"modify code\", \"change logic\", \"adjust configuration\", or a filename alone.\n\
7. `body` is the detailed description. Use an empty string only when the subject is sufficient.\n\
8. When `body` has more than one detail, format it as Markdown bullet lines that each start with \"- \". One changed behaviour or impact per bullet; no long paragraph.\n\
9. `changes` contains only breaking-change text without the \"BREAKING CHANGE:\" prefix.\n\
10. `closes` contains only issue references without the \"Closes\" prefix.\n\
11. `skipCi` is empty unless the selected changes clearly require a skip-ci marker.\n\
12. Allowed types: {}.",
        allowed_types(config)
    )
}

/// Allowed types with their descriptions, so the model can pick the right one
/// instead of guessing from a bare title.
fn allowed_types(config: &Config) -> String {
    config
        .commit_types
        .iter()
        .map(|item| {
            if item.description.is_empty() {
                item.title.clone()
            } else {
                format!("{} ({})", item.title, item.description)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn build_prompts(config: &Config, context: &GitContext, task: Task<'_>) -> (String, String) {
    let language = config.language.prompt_language();
    let mut system = SYSTEM_PROMPT.to_owned();
    if !config.extra_prompt.is_empty() {
        system.push_str("\n\nPersistent User Preferences:\n");
        system.push_str(&config.extra_prompt);
        system.push_str(
            "\n\nFollow these preferences when they do not conflict with the required JSON shape, the allowed types, the git diff, or the output constraints.",
        );
    }

    let preamble = match task {
        Task::Generate => "Generate git commit fields for this project.",
        Task::Requirements(_) => {
            "Generate git commit fields for this project, including the author's additional requirements."
        }
        Task::Format(_) => {
            "Rewrite the draft commit message below as git commit fields for this project."
        }
    };
    let mut contract = output_contract(config, language);
    if let Task::Format(_) = task {
        contract.push_str("\n\nRewrite rules: keep every fact the draft states (the change, its scope, issue numbers, breaking changes); fix the type, the subject shape, the body bullets and the language. Use the diff only to correct the draft, not to add changes it does not describe.");
    }

    let status = if context.status_lines.is_empty() {
        "(none)".to_owned()
    } else {
        context.status_lines.join("\n")
    };
    let diff = if context.diff.trim().is_empty() {
        "(no textual diff)"
    } else {
        context.diff.as_str()
    };

    let mut sections = vec![
        preamble.to_owned(),
        ANALYSIS_INSTRUCTIONS.to_owned(),
        contract,
    ];
    match task {
        Task::Generate => {}
        Task::Requirements(requirements) => sections.push(format!(
            "Author's Additional Requirements (written for this commit; follow them unless they contradict the diff or the JSON contract):\n{}",
            requirements.trim()
        )),
        Task::Format(message) => sections.push(format!(
            "Existing Draft Commit Message (preserve its meaning, details and language):\n{}",
            message.trim()
        )),
    }
    sections.push(format!("Repository Root:\n{}", context.root));
    sections.push(format!("Branch:\n{}", context.branch));
    sections.push(format!("Selected Files Status:\n{status}"));
    if !context.recent_subjects.is_empty() {
        // The scope sentence the checklist asks for: history is a style hint,
        // the selected changes stay the source of the content.
        sections.push(format!(
            "Recent Commit Subjects (the repository's own style; match their language, tone and scope usage, never copy them):\n{}",
            context.recent_subjects.join("\n")
        ));
    }
    sections.push(format!("Selected Changes Diff:\n{diff}"));
    (system, sections.join("\n\n"))
}

/// Removes reasoning wrappers (`<think…>`, `<thinking…>`) and Markdown code
/// fences around a JSON payload.
pub fn unwrap_json_text(text: &str) -> String {
    let mut value = text.trim();
    // Common reasoning tags used by local models.
    for close in ["</think>", "</thinking>", "</reasoning>"] {
        if let Some(pos) = value.rfind(close) {
            value = value[pos + close.len()..].trim();
        }
    }
    if let Some(rest) = value.strip_prefix("```json") {
        value = rest.trim();
    } else if let Some(rest) = value.strip_prefix("```") {
        value = rest.trim();
    }
    if let Some(rest) = value.strip_suffix("```") {
        value = rest.trim();
    }
    value.to_owned()
}

fn text_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) if key == "body" => {
            let mut parts = Vec::new();
            for item in items {
                if let Value::String(text) = item {
                    parts.push(format!("- {}", text.trim()));
                }
            }
            parts.join("\n")
        }
        Some(_) | None => String::new(),
    }
}

/// Parses the model response into fields. Tolerates reasoning tags, code
/// fences, arrays in `body`, and (as a last resort) a plain Conventional
/// Commit line.
pub fn parse_response(text: &str, allowed_types: &[String]) -> Option<CommitFields> {
    let fields = parse_fields(text)?;
    if fields.commit_type.is_empty()
        || !allowed_types.contains(&fields.commit_type)
        || fields
            .commit_type
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '(' | ')' | ':'))
        || fields.subject.trim().is_empty()
        || fields.subject.chars().any(char::is_control)
        || fields
            .scope
            .chars()
            .any(|c| c.is_control() || matches!(c, '(' | ')'))
    {
        return None;
    }
    Some(fields)
}

fn parse_fields(text: &str) -> Option<CommitFields> {
    let unwrapped = unwrap_json_text(text);
    if let (Some(start), Some(end)) = (unwrapped.find('{'), unwrapped.rfind('}'))
        && start < end
    {
        let slice = &unwrapped[start..=end];
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(slice) {
            let value = Value::Object(map);
            let fields = CommitFields {
                commit_type: text_field(&value, "type").trim().to_owned(),
                scope: text_field(&value, "scope").trim().to_owned(),
                subject: text_field(&value, "subject").trim().to_owned(),
                body: text_field(&value, "body").trim().to_owned(),
                changes: text_field(&value, "changes").trim().to_owned(),
                closes: text_field(&value, "closes").trim().to_owned(),
                skip_ci: text_field(&value, "skipCi").trim().to_owned(),
            };
            if !fields.subject.is_empty() || !fields.commit_type.is_empty() {
                return Some(fields);
            }
        }
    }

    // Lenient fallback: pull `"key": "value"` pairs one by one. Models that
    // emit an unterminated final string still give us the earlier fields.
    let mut fields = CommitFields::default();
    for key in [
        "type", "scope", "subject", "body", "changes", "closes", "skipCi",
    ] {
        if let Some(value) = lenient_string_field(&unwrapped, key) {
            match key {
                "type" => fields.commit_type = value,
                "scope" => fields.scope = value,
                "subject" => fields.subject = value,
                "body" => fields.body = value,
                "changes" => fields.changes = value,
                "closes" => fields.closes = value,
                "skipCi" => fields.skip_ci = value,
                _ => {}
            }
        }
    }
    if !fields.subject.is_empty() {
        return Some(fields);
    }

    // Last resort: a plain `type(scope): subject` line, with the rest as body.
    let mut lines = unwrapped.lines();
    let first = lines.next()?.trim();
    let mut parsed = parse_conventional_line(first)?;
    let rest: Vec<&str> = lines.collect();
    if !rest.is_empty() {
        parsed.body = rest.join("\n").trim().to_owned();
    }
    Some(parsed)
}

fn parse_conventional_line(line: &str) -> Option<CommitFields> {
    let (head, subject) = line.split_once(':')?;
    let subject = subject.trim();
    if subject.is_empty() {
        return None;
    }
    let head = head.trim();
    let (commit_type, scope) = match head.split_once('(') {
        Some((kind, rest)) if rest.ends_with(')') => (kind.trim(), rest[..rest.len() - 1].trim()),
        _ => (head, ""),
    };
    if commit_type.is_empty() || commit_type.contains(char::is_whitespace) {
        return None;
    }
    Some(CommitFields {
        commit_type: commit_type.to_owned(),
        scope: scope.to_owned(),
        subject: subject.to_owned(),
        ..CommitFields::default()
    })
}

fn lenient_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = text.find(&needle)?;
    let after = &text[at + needle.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut value = String::new();
    let mut escaped = false;
    for c in rest.chars() {
        if escaped {
            value.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(value);
        } else {
            value.push(c);
        }
    }
    // Unterminated string: use what is there.
    if !value.is_empty() { Some(value) } else { None }
}

fn normalize_body(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() <= 1 {
        return body.trim().to_owned();
    }
    if lines
        .iter()
        .all(|line| line.trim_start().starts_with('-') || line.trim_start().starts_with('*'))
    {
        return lines
            .iter()
            .map(|line| line.trim_start().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
    }
    lines
        .iter()
        .map(|line| {
            let line = line.trim();
            if line.starts_with('-') || line.starts_with('*') {
                line.to_owned()
            } else {
                format!("- {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The built-in Default template. Named templates in the settings table use
/// the same variables and conditional sections; full Velocity compatibility
/// (checklist item 46) is a separate evaluation.
pub const DEFAULT_TEMPLATE: &str = "{{header}}{{#body}}\n\n{{body}}{{/body}}{{#changes}}\n\nBREAKING CHANGE: {{changes}}{{/changes}}{{#closes}}\n\nCloses {{closes}}{{/closes}}{{#skipCi}}\n\n{{skipCi}}{{/skipCi}}";

const TEMPLATE_FIELDS: [&str; 8] = [
    "type", "scope", "subject", "body", "changes", "closes", "skipCi", "header",
];

fn template_header(fields: &CommitFields) -> String {
    let mut out = String::new();
    if !fields.commit_type.is_empty() || !fields.scope.is_empty() {
        out.push_str(fields.commit_type.trim());
        if !fields.scope.is_empty() {
            out.push('(');
            out.push_str(fields.scope.trim());
            out.push(')');
        }
        out.push_str(": ");
    }
    out.push_str(fields.subject.trim());
    out
}

fn template_value(fields: &CommitFields, name: &str) -> String {
    match name {
        "type" => fields.commit_type.trim().to_owned(),
        "scope" => fields.scope.trim().to_owned(),
        "subject" => fields.subject.trim().to_owned(),
        "body" => normalize_body(&fields.body),
        "changes" => fields.changes.trim().to_owned(),
        "closes" => fields.closes.trim().to_owned(),
        "skipCi" => fields.skip_ci.trim().to_owned(),
        "header" => template_header(fields),
        _ => String::new(),
    }
}

fn push_template_text(out: &mut String, block: &mut Option<(&str, String)>, text: &str) {
    if let Some((_, content)) = block {
        content.push_str(text);
    } else {
        out.push_str(text);
    }
}

/// Renders fields with a named template. `{{field}}` inserts a value;
/// `{{#field}}...{{/field}}` emits its contents only when that field is
/// non-empty. Plain text (including a footer) is kept as written. Nesting,
/// loops and expressions are rejected rather than silently mis-rendered.
pub fn render_template(fields: &CommitFields, template: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut block: Option<(&str, String)> = None;
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (text, tail) = rest.split_at(start);
        push_template_text(&mut out, &mut block, text);
        let after = &tail[2..];
        let Some(end) = after.find("}}") else {
            return Err("unclosed '{{' in the template".to_owned());
        };
        let tag = after[..end].trim();
        rest = &after[end + 2..];
        if let Some(name) = tag.strip_prefix('#') {
            if block.is_some() {
                return Err("nested conditional sections are not supported".to_owned());
            }
            if !TEMPLATE_FIELDS.contains(&name) {
                return Err(format!("unknown template variable '{name}'"));
            }
            block = Some((name, String::new()));
        } else if let Some(name) = tag.strip_prefix('/') {
            match block.take() {
                Some((open, content)) if open == name => {
                    if !template_value(fields, open).is_empty() {
                        out.push_str(&content);
                    }
                }
                Some((open, _)) => {
                    return Err(format!("section '{open}' is closed by '{name}'"));
                }
                None => return Err(format!("unexpected section end '{name}'")),
            }
        } else {
            if !TEMPLATE_FIELDS.contains(&tag) {
                return Err(format!("unknown template variable '{tag}'"));
            }
            let value = template_value(fields, tag);
            push_template_text(&mut out, &mut block, &value);
        }
    }
    push_template_text(&mut out, &mut block, rest);
    if let Some((open, _)) = block {
        return Err(format!("section '{open}' is never closed"));
    }
    if out.trim().is_empty() {
        return Err("the template produced an empty commit message".to_owned());
    }
    Ok(out.trim_end().to_owned())
}

/// Renders with the built-in Default template.
pub fn render(fields: &CommitFields) -> String {
    render_template(fields, DEFAULT_TEMPLATE).expect("the built-in template is valid")
}

/// The fields the template preview lets the user switch on and off.
pub const EXAMPLE_FIELDS: [&str; 7] = [
    "type", "scope", "subject", "body", "changes", "closes", "skipCi",
];

/// Example values for a preview: a selected field gets a visible placeholder,
/// an unselected one stays empty so its conditional section is skipped.
/// `header` is derived from `type` and `subject` by the renderer.
pub fn example_fields(selected: &[String]) -> CommitFields {
    let value = |name: &str| {
        if selected.iter().any(|key| key == name) {
            format!("<{name}>")
        } else {
            String::new()
        }
    };
    CommitFields {
        commit_type: value("type"),
        scope: value("scope"),
        subject: value("subject"),
        body: value("body"),
        changes: value("changes"),
        closes: value("closes"),
        skip_ci: value("skipCi"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> GitContext {
        GitContext {
            root: "/tmp/repo".into(),
            branch: "main".into(),
            status_lines: vec!["[M] src/a.rs".into()],
            recent_subjects: vec!["fix: keep the retry".into()],
            diff: "diff --git a/src/a.rs b/src/a.rs\n@@ -1 +1 @@\n-old\n+new".into(),
        }
    }

    #[test]
    fn renders_conventional_message() {
        let fields = CommitFields {
            commit_type: "feat".into(),
            scope: "parser".into(),
            subject: "支持新的语法".into(),
            body: "第一行\n第二行".into(),
            changes: "旧接口被移除".into(),
            closes: "#12".into(),
            skip_ci: "[skip ci]".into(),
        };
        let message = render(&fields);
        assert_eq!(
            message,
            "feat(parser): 支持新的语法\n\n- 第一行\n- 第二行\n\nBREAKING CHANGE: 旧接口被移除\n\nCloses #12\n\n[skip ci]"
        );
    }

    #[test]
    fn parses_json_with_fences_and_think() {
        let text = "<think>hmm</think>\n```json\n{\"type\":\"fix\",\"scope\":\"\",\"subject\":\"修复空指针\",\"body\":[\"a\",\"b\"],\"changes\":\"\",\"closes\":\"\",\"skipCi\":\"\"}\n```";
        let fields = parse_response(text, &["fix".into()]).unwrap();
        assert_eq!(fields.commit_type, "fix");
        assert_eq!(fields.subject, "修复空指针");
        assert_eq!(fields.body, "- a\n- b");
    }

    #[test]
    fn parses_plain_conventional_line() {
        let fields = parse_response(
            "refactor(core): split the parser\n\nMoves the lexer out.",
            &["refactor".into()],
        )
        .unwrap();
        assert_eq!(fields.commit_type, "refactor");
        assert_eq!(fields.scope, "core");
        assert_eq!(fields.subject, "split the parser");
        assert_eq!(fields.body, "Moves the lexer out.");
    }

    #[test]
    fn prompt_mentions_types_and_language() {
        let host = openidea_plugin::mock::MockHost::install();
        host.set_setting("language", "\"Chinese\"");
        host.set_setting("commitTypes", "\"feat,fix\"");
        let config = Config::load();
        let (system, user) = build_prompts(&config, &sample_context(), Task::Generate);
        assert!(system.contains("senior engineer"));
        assert!(user.contains("feat, fix"));
        assert!(user.contains("Simplified Chinese"));
        assert!(user.contains("Selected Changes Diff"));
        assert!(
            user.contains("Recent Commit Subjects") && user.contains("fix: keep the retry"),
            "the repository's own style reaches the model: {user}"
        );
    }

    #[test]
    fn prompt_carries_the_authors_requirements() {
        let _host = openidea_plugin::mock::MockHost::install();
        let config = Config::load();
        let (_, user) = build_prompts(
            &config,
            &sample_context(),
            Task::Requirements("closes #42; the crash came from an empty queue"),
        );
        assert!(user.contains("closes #42; the crash came from an empty queue"));
        assert!(user.contains("Additional Requirements"));
        assert!(!user.contains("Existing Draft Commit Message"));
    }

    #[test]
    fn prompt_asks_to_rewrite_an_existing_draft() {
        let _host = openidea_plugin::mock::MockHost::install();
        let config = Config::load();
        let (_, user) = build_prompts(
            &config,
            &sample_context(),
            Task::Format("fixed stuff and closes #7"),
        );
        assert!(user.contains("Existing Draft Commit Message"));
        assert!(user.contains("fixed stuff and closes #7"));
        assert!(user.contains("Rewrite rules"));
        assert!(!user.contains("Additional Requirements"));
    }

    #[test]
    fn prompt_leaves_the_style_section_out_without_history() {
        let _host = openidea_plugin::mock::MockHost::install();
        let config = Config::load();
        let mut context = sample_context();
        context.recent_subjects.clear();
        let (_, user) = build_prompts(&config, &context, Task::Generate);
        assert!(!user.contains("Recent Commit Subjects"));
    }

    #[test]
    fn auto_language_follows_the_ide_locale() {
        let host = openidea_plugin::mock::MockHost::install();
        let config = Config::load();
        host.set_locale("zh-CN");
        assert_eq!(config.language.prompt_language(), "Simplified Chinese");
        host.set_locale("en");
        assert_eq!(config.language.prompt_language(), "English");
    }

    fn full_fields() -> CommitFields {
        CommitFields {
            commit_type: "feat".into(),
            scope: "parser".into(),
            subject: "支持新的语法".into(),
            body: "第一行\n第二行".into(),
            changes: "旧接口被移除".into(),
            closes: "#12".into(),
            skip_ci: "[skip ci]".into(),
        }
    }

    #[test]
    fn the_built_in_template_matches_the_default_render() {
        assert_eq!(
            render(&full_fields()),
            "feat(parser): 支持新的语法\n\n- 第一行\n- 第二行\n\nBREAKING CHANGE: 旧接口被移除\n\nCloses #12\n\n[skip ci]"
        );
    }

    #[test]
    fn a_custom_template_controls_order_sections_and_footer() {
        let template =
            "{{subject}} [{{type}}/{{scope}}]{{#body}}\n\n{{body}}{{/body}}\n\nSigned-off-by: Team";
        assert_eq!(
            render_template(&full_fields(), template).unwrap(),
            "支持新的语法 [feat/parser]\n\n- 第一行\n- 第二行\n\nSigned-off-by: Team"
        );
    }

    #[test]
    fn empty_sections_leave_no_blank_lines() {
        let fields = CommitFields {
            commit_type: "fix".into(),
            subject: "fix crash".into(),
            ..CommitFields::default()
        };
        assert_eq!(
            render_template(&fields, DEFAULT_TEMPLATE).unwrap(),
            "fix: fix crash"
        );
        assert_eq!(
            render_template(
                &fields,
                "{{header}}{{#changes}}\n\nBREAKING CHANGE: {{changes}}{{/changes}}\n\nfooter"
            )
            .unwrap(),
            "fix: fix crash\n\nfooter"
        );
    }

    #[test]
    fn invalid_templates_are_reported() {
        let fields = full_fields();
        assert!(
            render_template(&fields, "{{nope}}")
                .unwrap_err()
                .contains("unknown")
        );
        assert!(
            render_template(&fields, "{{#body}}open")
                .unwrap_err()
                .contains("never closed")
        );
        assert!(
            render_template(&fields, "{{#body}}{{#changes}}x{{/changes}}{{/body}}")
                .unwrap_err()
                .contains("nested")
        );
        assert!(
            render_template(&fields, "unclosed {{")
                .unwrap_err()
                .contains("unclosed")
        );
    }

    #[test]
    fn example_fields_only_fill_the_selected_ones() {
        let fields = example_fields(&["type".into(), "subject".into(), "body".into()]);
        assert_eq!(
            render_template(&fields, DEFAULT_TEMPLATE).unwrap(),
            "<type>: <subject>\n\n<body>"
        );
        assert!(fields.scope.is_empty());
        assert!(fields.closes.is_empty());
    }

    #[test]
    fn prompt_lists_type_descriptions() {
        let host = openidea_plugin::mock::MockHost::install();
        host.set_setting(
            "commitTypes",
            r#"[{"title":"security","description":"Security hardening"}]"#,
        );
        let config = Config::load();
        let (_, user) = build_prompts(&config, &sample_context(), Task::Generate);
        assert!(user.contains("security (Security hardening)"), "{user}");
    }
}
