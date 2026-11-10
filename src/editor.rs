//! Checklist 3: the manual structured editor.
//!
//! Plugin API 1.4 has no dialog with several fields, so the editor is a
//! centred list of the commit fields and their current values (`host::pick`).
//! Choosing a field edits it in its own dialog and comes back to the list.
//! Write renders the fields locally with the active template and writes the
//! result into the Commit panel's message field, under the same `revision`
//! check the generation path uses, so text the user typed meanwhile is never
//! overwritten. Every edit is kept per repository: closing the list loses
//! nothing.
//!
//! Checklists 16–18 shape the list: which fields it shows, how the type
//! choice orders the allowed types, and the Skip CI markers with their
//! default.

use crate::config::{Config, EditorSettings};
use crate::prompt::{self, CommitFields};
use crate::ui::{self, Strings};
use openidea_plugin::{Error, NotificationLevel, PickItem, host};
use serde::{Deserialize, Serialize};

const APPLY: &str = "apply";
const CLEAR: &str = "clear";
/// The Skip CI choice that writes no marker; a preset is `marker:<index>`.
const SKIP_CI_OFF: &str = "off";
const MARKER: &str = "marker:";
/// How many characters of a value the list shows.
const PREVIEW: usize = 60;

/// What checklist 5 keeps for the next time the editor opens.
#[derive(Serialize, Deserialize, Default)]
struct Stored {
    fields: CommitFields,
    skip_ci_marker: String,
    skip_ci_on: bool,
}

/// The fields the user is editing and the last thing that happened.
#[derive(Default)]
pub struct Editor {
    /// The repository the fields belong to, so a draft is per repository.
    project: String,
    fields: CommitFields,
    /// The chosen Skip CI marker.
    skip_ci_marker: String,
    /// Whether the marker is written.
    skip_ci_on: bool,
    /// The settings' defaults or the stored draft apply once per project.
    initialised: bool,
    /// Why the last Write wrote nothing; the list shows it as its title.
    status: Option<String>,
}

impl Editor {
    /// Shows the field list until the fields are written, or until the list
    /// is closed (Cancel / Escape), which keeps them for next time.
    pub fn show(&mut self, project: &str, strings: &Strings) -> Result<(), Error> {
        let config = Config::load();
        self.open(project, &config.editor);
        loop {
            self.settle(&config);
            let (title, items) = self.menu(&config, strings);
            let Some(key) = host::pick(&title, &items)? else {
                return Ok(());
            };
            match key.as_str() {
                APPLY => {
                    if self.apply(strings)? {
                        self.initialised = false;
                        return Ok(());
                    }
                }
                CLEAR => self.clear(&config.editor),
                field => self.edit(field, &config, strings)?,
            }
        }
    }

    /// A type or marker the settings no longer offer falls back to the first
    /// one offered.
    fn settle(&mut self, config: &Config) {
        let types = type_options(config);
        if !types.contains(&self.fields.commit_type) {
            self.fields.commit_type = types.first().cloned().unwrap_or_default();
        }
        let presets = &config.editor.skip_ci_presets;
        if !presets
            .iter()
            .any(|preset| preset.marker == self.skip_ci_marker)
            && let Some(first) = presets.first()
        {
            self.skip_ci_marker = first.marker.clone();
        }
    }

    /// The list's title and rows: each shown field with its value, then the
    /// actions.
    fn menu(&self, config: &Config, strings: &Strings) -> (String, Vec<PickItem>) {
        let settings = &config.editor;
        let row = |key: &str, label: &str, value: &str| {
            PickItem::new(key, label).description(&preview(value, strings))
        };
        let mut items = Vec::new();
        if settings.show_type {
            items.push(row("type", strings.field_type, &self.fields.commit_type));
        }
        if settings.show_scope {
            items.push(row("scope", strings.field_scope, &self.fields.scope));
        }
        // The subject is the one field Commit cannot do without, so it is
        // always shown.
        items.push(row("subject", strings.field_subject, &self.fields.subject));
        if settings.show_body {
            items.push(row("body", strings.field_body, &self.fields.body));
        }
        if settings.show_changes {
            items.push(row("changes", strings.field_changes, &self.fields.changes));
        }
        if settings.show_closes {
            items.push(row("closes", strings.field_closes, &self.fields.closes));
        }
        if settings.show_skip_ci {
            let value = if self.skip_ci_on {
                preview(&self.skip_ci_marker, strings)
            } else {
                strings.skip_ci_off.to_owned()
            };
            items.push(PickItem::new("skipCi", strings.field_skip_ci).description(&value));
        }
        items.push(PickItem::new(APPLY, strings.editor_apply));
        items.push(PickItem::new(CLEAR, strings.editor_clear));
        let title = self
            .status
            .clone()
            .unwrap_or_else(|| crate::TITLE.to_owned());
        (title, items)
    }

    /// Edits one field in its own dialog; Cancel leaves it as it was.
    fn edit(&mut self, key: &str, config: &Config, strings: &Strings) -> Result<(), Error> {
        let answer = match key {
            "type" => {
                let items: Vec<PickItem> = type_options(config)
                    .iter()
                    .map(|title| type_item(config, title))
                    .collect();
                host::pick(strings.field_type, &items)?
            }
            "scope" => host::input(strings.field_scope, Some(&self.fields.scope))?,
            "subject" => host::input(strings.field_subject, Some(&self.fields.subject))?,
            "closes" => host::input(strings.field_closes, Some(&self.fields.closes))?,
            "body" => host::input_multiline(strings.field_body, Some(&self.fields.body))?,
            "changes" => host::input_multiline(strings.field_changes, Some(&self.fields.changes))?,
            "skipCi" => return self.edit_skip_ci(config, strings),
            _ => None,
        };
        if let Some(value) = answer {
            self.set_field(key, &value);
        }
        Ok(())
    }

    /// One choice for the marker and whether it is written.
    fn edit_skip_ci(&mut self, config: &Config, strings: &Strings) -> Result<(), Error> {
        let presets = &config.editor.skip_ci_presets;
        let mut items = vec![PickItem::new(SKIP_CI_OFF, strings.skip_ci_off)];
        items.extend(presets.iter().enumerate().map(|(index, preset)| {
            PickItem::new(&format!("{MARKER}{index}"), &preset.marker).description(&preset.name)
        }));
        let Some(key) = host::pick(strings.field_skip_ci, &items)? else {
            return Ok(());
        };
        if key == SKIP_CI_OFF {
            self.set_field("skipCiOn", "false");
        } else if let Some(preset) = key
            .strip_prefix(MARKER)
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| presets.get(index))
        {
            self.set_field("skipCi", &preset.marker);
            self.set_field("skipCiOn", "true");
        }
        Ok(())
    }

    /// Loads what the user left behind for this project (checklist 5), or the
    /// settings' defaults (checklist 18) the first time.
    fn open(&mut self, project: &str, settings: &EditorSettings) {
        if self.initialised && self.project == project {
            return;
        }
        self.project = project.to_owned();
        self.initialised = true;
        self.status = None;
        self.fields = CommitFields::default();
        self.skip_ci_marker = settings.skip_ci_default.clone();
        self.skip_ci_on = settings.skip_ci_checked;
        if let Some(stored) = self.stored() {
            self.fields = stored.fields;
            self.skip_ci_marker = stored.skip_ci_marker;
            self.skip_ci_on = stored.skip_ci_on;
        }
    }

    fn storage_key(&self) -> String {
        format!("editor:{}", self.project)
    }

    /// The draft kept for this project, if the list was closed mid-edit.
    fn stored(&self) -> Option<Stored> {
        if self.project.is_empty() {
            return None;
        }
        let raw = host::storage::get(&self.storage_key()).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    }

    /// Keeps the current fields for the next time, in this process or after a
    /// restart: the IDE drops the Wasm instance when it is idle.
    fn keep(&self) {
        if self.project.is_empty() {
            return;
        }
        let stored = Stored {
            fields: self.fields.clone(),
            skip_ci_marker: self.skip_ci_marker.clone(),
            skip_ci_on: self.skip_ci_on,
        };
        if let Ok(text) = serde_json::to_string(&stored) {
            let _ = host::storage::set(&self.storage_key(), &text);
        }
    }

    /// Applying or clearing ends the draft: there is nothing left to restore.
    fn forget(&self) {
        if !self.project.is_empty() {
            let _ = host::storage::delete(&self.storage_key());
        }
    }

    /// A field the user changed. Nothing is written to the commit panel until
    /// Write.
    fn set_field(&mut self, key: &str, value: &str) {
        match key {
            "type" => self.fields.commit_type = value.to_owned(),
            "scope" => self.fields.scope = value.to_owned(),
            "subject" => self.fields.subject = value.to_owned(),
            "body" => self.fields.body = value.to_owned(),
            "changes" => self.fields.changes = value.to_owned(),
            "closes" => self.fields.closes = value.to_owned(),
            "skipCi" => self.skip_ci_marker = value.to_owned(),
            "skipCiOn" => self.skip_ci_on = value == "true",
            _ => return,
        }
        self.status = None;
        self.keep();
    }

    /// The fields as they are written: the marker only when it is chosen.
    fn composed(&self) -> CommitFields {
        let mut fields = self.fields.clone();
        fields.skip_ci = if self.skip_ci_on {
            self.skip_ci_marker.trim().to_owned()
        } else {
            String::new()
        };
        fields
    }

    /// Renders the fields with the active template and writes them into the
    /// Commit panel's message field. `false`: nothing was written, and
    /// `status` says why.
    fn apply(&mut self, strings: &Strings) -> Result<bool, Error> {
        let config = Config::load();
        if self.fields.subject.trim().is_empty() {
            self.status = Some(strings.editor_no_subject.to_owned());
            host::notify(
                NotificationLevel::Warning,
                crate::TITLE,
                strings.editor_no_subject,
            );
            return Ok(false);
        }
        let fields = self.composed();
        let allowed_types = type_options(&config);
        if !prompt::valid_header(&fields, &allowed_types) {
            self.status = Some(strings.editor_invalid_header.to_owned());
            host::notify(
                NotificationLevel::Warning,
                crate::TITLE,
                strings.editor_invalid_header,
            );
            return Ok(false);
        }
        let template = config
            .template
            .as_deref()
            .unwrap_or(prompt::DEFAULT_TEMPLATE);
        let (message, template_error) = match prompt::render_template(&fields, template) {
            Ok(message) => (message, None),
            // A broken template must not lose what the user typed.
            Err(error) => (prompt::render(&fields), Some(error)),
        };
        let Some(draft) = host::pending_commit()? else {
            self.status = Some(strings.no_repository.to_owned());
            host::notify(
                NotificationLevel::Warning,
                crate::TITLE,
                strings.no_repository,
            );
            return Ok(false);
        };
        match host::set_commit_message(draft.revision, &message) {
            Ok(()) => {
                self.status = None;
                self.forget();
                let mut body = format!("{}\n\n{}", strings.editor_applied, ui::clip(&message, 800));
                if let Some(error) = template_error {
                    body.push_str(&format!("\n\n{} {error}", strings.template_fallback));
                }
                host::notify(NotificationLevel::Info, crate::TITLE, &body);
                Ok(true)
            }
            // The user typed in the commit field between the two calls: the
            // fields stay so they can be written again.
            Err(Error::Stale) => {
                self.status = Some(strings.stale.to_owned());
                host::notify(NotificationLevel::Warning, crate::TITLE, strings.stale);
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Empties the fields and puts the settings' defaults back.
    fn clear(&mut self, settings: &EditorSettings) {
        self.fields = CommitFields::default();
        self.skip_ci_marker = settings.skip_ci_default.clone();
        self.skip_ci_on = settings.skip_ci_checked;
        self.status = None;
        self.forget();
    }
}

/// The allowed types in the order the editor offers them.
fn type_options(config: &Config) -> Vec<String> {
    let types: Vec<String> = config
        .commit_types
        .iter()
        .map(|item| item.title.clone())
        .filter(|title| !title.trim().is_empty())
        .collect();
    config.editor.type_options(&types)
}

/// A type with its description from the settings table, if it has one.
fn type_item(config: &Config, title: &str) -> PickItem {
    let item = PickItem::new(title, title);
    match config
        .commit_types
        .iter()
        .find(|commit_type| commit_type.title == title)
        .map(|commit_type| commit_type.description.as_str())
        .filter(|description| !description.is_empty())
    {
        Some(description) => item.description(description),
        None => item,
    }
}

/// A value as the list shows it: its first line, clipped, or a placeholder.
fn preview(value: &str, strings: &Strings) -> String {
    let mut lines = value.trim().lines();
    let Some(first) = lines.next() else {
        return strings.field_empty.to_owned();
    };
    let first = ui::clip(first, PREVIEW);
    if lines.next().is_some() {
        format!("{first} …")
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PROJECT: &str = "/repo";

    fn strings() -> &'static Strings {
        ui::strings(crate::config::UiLanguage::En)
    }

    fn keys(items: &[PickItem]) -> Vec<&str> {
        items.iter().map(|item| item.key.as_str()).collect()
    }

    fn description<'a>(items: &'a [PickItem], key: &str) -> &'a str {
        items
            .iter()
            .find(|item| item.key == key)
            .and_then(|item| item.description.as_deref())
            .unwrap_or_else(|| panic!("no described row {key} in {:?}", keys(items)))
    }

    /// Answers the editor's dialogs in order: a row or choice key, a typed
    /// text, or `None` for Cancel / Escape.
    fn answer(host: &MockHost, answers: &[Option<&str>]) {
        for answer in answers {
            match answer {
                Some(text) => host.answer_next_prompt(Some(&[*text])),
                None => host.answer_next_prompt(None),
            }
        }
    }

    fn draft() -> openidea_plugin::CommitDraft {
        openidea_plugin::CommitDraft {
            revision: 4,
            message: String::new(),
            amend: false,
            changes: Vec::new(),
        }
    }

    fn storage_host() -> MockHost {
        MockHost::install().with_capabilities(&["vcs.read", "vcs.write", "storage"])
    }

    /// The list as the next `show` would present it.
    fn shown(editor: &mut Editor, project: &str) -> (String, Vec<PickItem>) {
        let config = Config::load();
        editor.open(project, &config.editor);
        editor.settle(&config);
        editor.menu(&config, strings())
    }

    #[test]
    fn the_list_shows_every_field_with_its_value_then_the_actions() {
        let _host = MockHost::install();
        let (title, items) = shown(&mut Editor::default(), PROJECT);
        assert_eq!(title, crate::TITLE);
        assert_eq!(
            keys(&items),
            vec![
                "type", "scope", "subject", "body", "changes", "closes", "skipCi", APPLY, CLEAR
            ]
        );
        assert_eq!(
            description(&items, "type"),
            "feat",
            "the first allowed type"
        );
        assert_eq!(description(&items, "subject"), "(empty)");
        assert_eq!(description(&items, "skipCi"), "Don't write a marker");
    }

    #[test]
    fn hidden_fields_are_not_listed() {
        let host = MockHost::install();
        host.set_setting("editorShowScope", "false");
        host.set_setting("editorShowChanges", "false");
        host.set_setting("editorShowSkipCi", "false");
        let (_, items) = shown(&mut Editor::default(), PROJECT);
        assert_eq!(
            keys(&items),
            vec!["type", "subject", "body", "closes", APPLY, CLEAR]
        );
    }

    #[test]
    fn a_long_body_shows_its_first_line() {
        let _host = storage_host();
        let mut editor = Editor::default();
        editor.open(PROJECT, &Config::load().editor);
        editor.set_field("body", "first line\nsecond line");
        let (_, items) = shown(&mut editor, PROJECT);
        assert_eq!(description(&items, "body"), "first line …");
    }

    #[test]
    fn the_type_choice_puts_the_common_types_first() {
        let host = MockHost::install();
        host.set_setting("commitTypes", r#"["zap","fix","docs","amper","chore"]"#);
        host.set_setting("editorTypeStyle", "\"common-first\"");
        host.set_setting("editorCommonTypes", "2");
        assert_eq!(
            type_options(&Config::load()),
            vec!["zap", "fix", "amper", "chore", "docs"],
            "the first two rows stay on top, the rest is alphabetical"
        );
        host.set_setting("editorTypeStyle", "\"table\"");
        assert_eq!(
            type_options(&Config::load()),
            vec!["zap", "fix", "docs", "amper", "chore"]
        );
    }

    #[test]
    fn the_skip_ci_default_writes_its_marker() {
        let host = storage_host();
        host.set_setting(
            "skipCiPresets",
            r#"[{"name":"skip ci","marker":"[skip ci]"},{"name":"ci skip","marker":"[ci skip]"}]"#,
        );
        host.set_setting("skipCiDefault", "\"ci skip\"");
        host.set_setting("skipCiDefaultChecked", "true");
        host.set_commit_draft(Some(draft()));
        answer(
            &host,
            &[
                Some("subject"),
                Some("stop the empty-queue crash"),
                Some(APPLY),
            ],
        );
        Editor::default().show(PROJECT, strings()).unwrap();
        assert_eq!(
            host.commit_draft().unwrap().message,
            "feat: stop the empty-queue crash\n\n[ci skip]",
            "the default type and marker come from the settings"
        );
    }

    #[test]
    fn choosing_no_marker_keeps_skip_ci_out() {
        let host = storage_host();
        host.set_setting("skipCiDefaultChecked", "true");
        host.set_commit_draft(Some(draft()));
        answer(
            &host,
            &[
                Some("subject"),
                Some("stop the empty-queue crash"),
                Some("skipCi"),
                Some(SKIP_CI_OFF),
                Some(APPLY),
            ],
        );
        Editor::default().show(PROJECT, strings()).unwrap();
        assert_eq!(
            host.commit_draft().unwrap().message,
            "feat: stop the empty-queue crash"
        );
    }

    #[test]
    fn clearing_restores_the_defaults_and_drops_the_draft() {
        let host = storage_host();
        answer(
            &host,
            &[
                Some("subject"),
                Some("x"),
                Some("skipCi"),
                Some("marker:0"),
                Some(CLEAR),
                None,
            ],
        );
        let mut editor = Editor::default();
        editor.show(PROJECT, strings()).unwrap();
        assert!(!host.storage().contains_key("editor:/repo"));
        let (_, items) = shown(&mut editor, PROJECT);
        assert_eq!(description(&items, "subject"), "(empty)");
        assert_eq!(
            description(&items, "skipCi"),
            "Don't write a marker",
            "the default is unticked unless the settings say otherwise"
        );
    }

    #[test]
    fn a_draft_belongs_to_its_repository() {
        let host = storage_host();
        answer(&host, &[Some("subject"), Some("first repository"), None]);
        let mut editor = Editor::default();
        editor.show(PROJECT, strings()).unwrap();
        assert!(host.storage().contains_key("editor:/repo"));

        let (_, items) = shown(&mut editor, "/other-repo");
        assert_eq!(
            description(&items, "subject"),
            "(empty)",
            "another repository starts empty"
        );
        let (_, items) = shown(&mut editor, PROJECT);
        assert_eq!(
            description(&items, "subject"),
            "first repository",
            "going back restores the first repository's draft"
        );
    }

    #[test]
    fn writing_without_a_subject_writes_nothing() {
        let host = storage_host();
        host.set_commit_draft(Some(openidea_plugin::CommitDraft {
            message: "mine".into(),
            ..draft()
        }));
        answer(&host, &[Some(APPLY), None]);
        Editor::default().show(PROJECT, strings()).unwrap();
        assert_eq!(host.commit_draft().unwrap().message, "mine");
        assert_eq!(
            host.notifications().last().expect("it reports").level,
            NotificationLevel::Warning
        );
    }

    #[test]
    fn no_repository_is_reported_without_writing() {
        let host = storage_host();
        answer(
            &host,
            &[Some("subject"), Some("stop the crash"), Some(APPLY), None],
        );
        let mut editor = Editor::default();
        editor.show(PROJECT, strings()).unwrap();
        assert!(
            host.notifications()
                .last()
                .expect("it reports")
                .body
                .contains("No Git repository"),
        );
        assert!(shown(&mut editor, PROJECT).0.contains("No Git repository"));
    }

    #[test]
    fn invalid_headers_keep_the_commit_and_show_the_reason_on_the_list() {
        for (key, value) in [
            ("scope", "core)"),
            ("scope", "core\t"),
            ("subject", "first\nsecond"),
        ] {
            let host = storage_host();
            host.set_commit_draft(Some(openidea_plugin::CommitDraft {
                message: "keep my draft".into(),
                ..draft()
            }));
            answer(
                &host,
                &[
                    Some("subject"),
                    Some("stop the crash"),
                    Some(key),
                    Some(value),
                    Some(APPLY),
                    None,
                ],
            );
            let mut editor = Editor::default();
            editor.show(PROJECT, strings()).unwrap();
            assert_eq!(
                host.commit_draft().unwrap().message,
                "keep my draft",
                "invalid {key}"
            );
            assert_eq!(
                host.notifications().last().unwrap().level,
                NotificationLevel::Warning
            );
            assert!(host.storage().contains_key("editor:/repo"));
            let (title, items) = shown(&mut editor, PROJECT);
            assert_eq!(title, strings().editor_invalid_header);
            assert_eq!(
                description(&items, "subject"),
                if key == "subject" {
                    "first …"
                } else {
                    "stop the crash"
                }
            );
        }
    }
}
