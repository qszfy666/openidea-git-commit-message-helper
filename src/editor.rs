//! Checklist 3: the manual structured editor.
//!
//! A tool-window form holds the commit fields. Applying renders them locally
//! with the active template and writes the result into the Commit panel's
//! message field, under the same `revision` check the generation path uses, so
//! text the user typed meanwhile is never overwritten.
//!
//! Checklists 16–18 shape the form: which fields it shows, how the type
//! control orders the allowed types, and the Skip CI markers with their
//! default.

use crate::config::{Config, EditorSettings};
use crate::prompt::{self, CommitFields};
use crate::ui::{self, Strings};
use openidea_plugin::views::{Body, Button, Field, FieldValue, View, ViewEvent};
use openidea_plugin::{Error, NotificationLevel, host};
use serde::{Deserialize, Serialize};

/// The view declared in `plugin.toml`.
pub const VIEW_ID: &str = "commit-editor";

const APPLY: &str = "apply";
const CLEAR: &str = "clear";

/// The form fields in the order the view shows them, for the settings defaults.
#[cfg(test)]
const FIELDS: [&str; 8] = [
    "type", "scope", "subject", "body", "changes", "closes", "skipCi", "skipCiOn",
];

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
    /// The project the fields belong to, so a draft is per project.
    project: String,
    fields: CommitFields,
    /// The Skip CI marker the choice shows.
    skip_ci_marker: String,
    /// Whether the Skip CI checkbox is ticked.
    skip_ci_on: bool,
    /// The settings' defaults or the stored draft apply once per project.
    initialised: bool,
    /// One line under the form: what the last action did.
    status: Option<String>,
}

impl Editor {
    /// The form, its toolbar and the status line. The values come from the
    /// editor's own state: the IDE sends every change back as a field event.
    pub fn render(&mut self, project: &str, config: &Config, strings: &Strings) -> View {
        let settings = &config.editor;
        self.open(project, settings);

        let options = type_options(config);
        let chosen = if options.contains(&self.fields.commit_type) {
            self.fields.commit_type.clone()
        } else {
            options.first().cloned().unwrap_or_default()
        };
        self.fields.commit_type = chosen.clone();

        let mut form = Vec::new();
        if settings.show_type {
            let value = if options.is_empty() {
                FieldValue::Text(chosen)
            } else {
                FieldValue::Choice(options, chosen)
            };
            form.push(Field::new("type", strings.field_type, value));
        }
        if settings.show_scope {
            form.push(Field::new(
                "scope",
                strings.field_scope,
                FieldValue::Text(self.fields.scope.clone()),
            ));
        }
        // The subject is the one field Commit cannot do without, so it is
        // always shown.
        form.push(Field::new(
            "subject",
            strings.field_subject,
            FieldValue::Text(self.fields.subject.clone()),
        ));
        if settings.show_body {
            form.push(Field::new(
                "body",
                strings.field_body,
                FieldValue::Multiline(self.fields.body.clone()),
            ));
        }
        if settings.show_changes {
            form.push(Field::new(
                "changes",
                strings.field_changes,
                FieldValue::Multiline(self.fields.changes.clone()),
            ));
        }
        if settings.show_closes {
            form.push(Field::new(
                "closes",
                strings.field_closes,
                FieldValue::Text(self.fields.closes.clone()),
            ));
        }
        if settings.show_skip_ci {
            let markers: Vec<String> = settings
                .skip_ci_presets
                .iter()
                .map(|preset| preset.marker.clone())
                .collect();
            let chosen = if markers.contains(&self.skip_ci_marker) {
                self.skip_ci_marker.clone()
            } else {
                markers.first().cloned().unwrap_or_default()
            };
            self.skip_ci_marker = chosen.clone();
            let value = if markers.is_empty() {
                FieldValue::Text(chosen)
            } else {
                FieldValue::Choice(markers, chosen)
            };
            form.push(Field::new("skipCi", strings.field_skip_ci, value));
            form.push(Field::new(
                "skipCiOn",
                strings.field_skip_ci_on,
                FieldValue::Checkbox(self.skip_ci_on),
            ));
        }

        View::new(Body::Form(form))
            .button(Button::new(APPLY, strings.editor_apply))
            .button(Button::new(CLEAR, strings.editor_clear))
            .status(
                self.status
                    .clone()
                    .unwrap_or_else(|| strings.editor_hint.to_owned())
                    .as_str(),
            )
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

    /// The draft kept for this project, if the panel was closed mid-edit.
    fn stored(&self) -> Option<Stored> {
        if self.project.is_empty() {
            return None;
        }
        let raw = host::storage::get(&self.storage_key()).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    }

    /// Keeps the current fields for the next render, in this process or after a
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
    /// Apply.
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

    /// The fields as they are written: the marker only when it is ticked.
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
    /// Commit panel's message field.
    pub fn apply(&mut self, strings: &Strings) -> Result<(), Error> {
        let config = Config::load();
        // A command can apply before the view was ever rendered: start from the
        // first allowed type, as the form does.
        if self.fields.commit_type.trim().is_empty() {
            self.fields.commit_type = type_options(&config).first().cloned().unwrap_or_default();
        }
        if self.fields.subject.trim().is_empty() {
            self.status = Some(strings.editor_no_subject.to_owned());
            host::notify(
                NotificationLevel::Warning,
                crate::TITLE,
                strings.editor_no_subject,
            );
            return Ok(());
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
            return Ok(());
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
            return Ok(());
        };
        match host::set_commit_message(draft.revision, &message) {
            Ok(()) => {
                self.status = Some(strings.editor_applied.to_owned());
                self.forget();
                let mut body = format!("{}\n\n{}", strings.editor_applied, ui::clip(&message, 800));
                if let Some(error) = template_error {
                    body.push_str(&format!("\n\n{} {error}", strings.template_fallback));
                }
                host::notify(NotificationLevel::Info, crate::TITLE, &body);
            }
            // The user typed in the commit field between the two calls: the
            // form keeps its values so they can be applied again.
            Err(Error::Stale) => {
                self.status = Some(strings.stale.to_owned());
                host::notify(NotificationLevel::Warning, crate::TITLE, strings.stale);
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Empties the form and puts the settings' defaults back.
    fn clear(&mut self, settings: &EditorSettings, strings: &Strings) {
        self.fields = CommitFields::default();
        self.skip_ci_marker = settings.skip_ci_default.clone();
        self.skip_ci_on = settings.skip_ci_checked;
        self.status = Some(strings.editor_cleared.to_owned());
        self.forget();
    }

    /// Routes one view event.
    pub fn handle(
        &mut self,
        project: &str,
        event: ViewEvent,
        config: &Config,
        strings: &Strings,
    ) -> Result<(), Error> {
        self.open(project, &config.editor);
        match event {
            // Text inputs keep local state and their caret. Choice and
            // checkbox controls need the updated view to show their value.
            ViewEvent::Field(key, value) => {
                self.set_field(&key, &value);
                if matches!(key.as_str(), "type" | "skipCi" | "skipCiOn") {
                    host::refresh_view(VIEW_ID);
                }
                return Ok(());
            }
            ViewEvent::Button(id) if id == APPLY => self.apply(strings)?,
            ViewEvent::Button(id) if id == CLEAR => self.clear(&config.editor, strings),
            _ => return Ok(()),
        }
        host::refresh_view(VIEW_ID);
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const PROJECT: &str = "/repo";

    fn strings() -> &'static Strings {
        ui::strings(crate::config::UiLanguage::En)
    }

    fn keys(view: &View) -> Vec<String> {
        let Body::Form(fields) = &view.body else {
            panic!("the editor renders a form: {:?}", view.body)
        };
        fields.iter().map(|field| field.key.clone()).collect()
    }

    fn value_of(view: &View, key: &str) -> FieldValue {
        let Body::Form(fields) = &view.body else {
            panic!("the editor renders a form")
        };
        fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.clone())
            .unwrap_or_else(|| panic!("no field {key} in {:?}", keys(view)))
    }

    fn draft() -> openidea_plugin::CommitDraft {
        openidea_plugin::CommitDraft {
            revision: 4,
            message: String::new(),
            amend: false,
            changes: Vec::new(),
        }
    }

    #[test]
    fn the_form_shows_every_field_by_default() {
        let host = MockHost::install();
        host.set_setting("commitTypes", r#"["feat","fix"]"#);
        let strings = strings();
        let view = Editor::default().render(PROJECT, &Config::load(), strings);
        assert_eq!(keys(&view), FIELDS.to_vec());
        assert_eq!(
            value_of(&view, "type"),
            FieldValue::Choice(vec!["feat".into(), "fix".into()], "feat".into()),
            "the type is a dropdown of the allowed types"
        );
        let toolbar: Vec<&str> = view.toolbar.iter().map(|id| id.id.as_str()).collect();
        assert_eq!(toolbar, vec![APPLY, CLEAR]);
    }

    #[test]
    fn hidden_fields_are_not_rendered() {
        let host = MockHost::install();
        host.set_setting("editorShowScope", "false");
        host.set_setting("editorShowChanges", "false");
        host.set_setting("editorShowSkipCi", "false");
        let strings = strings();
        let view = Editor::default().render(PROJECT, &Config::load(), strings);
        assert_eq!(keys(&view), vec!["type", "subject", "body", "closes"]);
    }

    #[test]
    fn the_type_control_puts_the_common_types_first() {
        let host = MockHost::install();
        host.set_setting("commitTypes", r#"["zap","fix","docs","amper","chore"]"#);
        host.set_setting("editorTypeStyle", "\"common-first\"");
        host.set_setting("editorCommonTypes", "2");
        let strings = strings();
        let view = Editor::default().render(PROJECT, &Config::load(), strings);
        assert_eq!(
            value_of(&view, "type"),
            FieldValue::Choice(
                vec![
                    "zap".to_owned(),
                    "fix".to_owned(),
                    "amper".to_owned(),
                    "chore".to_owned(),
                    "docs".to_owned(),
                ],
                "zap".to_owned()
            ),
            "the first two rows stay on top, the rest is alphabetical"
        );

        host.set_setting("editorTypeStyle", "\"table\"");
        let view = Editor::default().render(PROJECT, &Config::load(), strings);
        assert_eq!(
            value_of(&view, "type"),
            FieldValue::Choice(
                vec![
                    "zap".into(),
                    "fix".into(),
                    "docs".into(),
                    "amper".into(),
                    "chore".into(),
                ],
                "zap".into()
            )
        );
    }

    #[test]
    fn a_ticked_skip_ci_writes_the_chosen_marker() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_setting(
            "skipCiPresets",
            r#"[{"name":"skip ci","marker":"[skip ci]"},{"name":"ci skip","marker":"[ci skip]"}]"#,
        );
        host.set_setting("skipCiDefault", "\"ci skip\"");
        host.set_setting("skipCiDefaultChecked", "true");
        host.set_commit_draft(Some(draft()));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        let view = editor.render(PROJECT, &config, strings);
        assert_eq!(
            value_of(&view, "skipCi"),
            FieldValue::Choice(
                vec!["[skip ci]".into(), "[ci skip]".into()],
                "[ci skip]".into()
            ),
            "the default preset is preselected"
        );
        assert_eq!(value_of(&view, "skipCiOn"), FieldValue::Checkbox(true));

        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert_eq!(
            host.commit_draft().expect("the draft is there").message,
            "feat: stop the empty-queue crash\n\n[ci skip]",
            "the default type comes from the settings, the marker from the checkbox"
        );
    }

    #[test]
    fn an_unticked_skip_ci_stays_out_of_the_message() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(draft()));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("skipCiOn".into(), "false".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert_eq!(
            host.commit_draft().expect("the draft is there").message,
            "feat: stop the empty-queue crash",
            "an unticked marker stays out"
        );
    }

    #[test]
    fn clearing_restores_the_configured_defaults() {
        let _host = MockHost::install();
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "x".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("skipCiOn".into(), "true".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(CLEAR.into()), &config, strings)
            .unwrap();
        assert!(editor.fields.subject.is_empty());
        assert_eq!(
            editor.composed().skip_ci,
            "",
            "the default is unticked unless the settings say otherwise"
        );
    }

    #[test]
    fn a_typed_field_reaches_the_form_and_the_message() {
        let host =
            MockHost::install().with_capabilities(&["vcs.read", "vcs.write", "storage", "secrets"]);
        host.set_setting("commitTypes", r#"["feat","fix"]"#);
        host.set_commit_draft(Some(draft()));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        for (key, value) in [
            ("type", "fix"),
            ("scope", "queue"),
            ("subject", "stop the empty-queue crash"),
        ] {
            editor
                .handle(
                    PROJECT,
                    ViewEvent::Field(key.into(), value.into()),
                    &config,
                    strings,
                )
                .unwrap();
        }

        let view = editor.render(PROJECT, &config, strings);
        assert_eq!(
            value_of(&view, "type"),
            FieldValue::Choice(vec!["feat".into(), "fix".into()], "fix".into())
        );
        assert_eq!(
            value_of(&view, "subject"),
            FieldValue::Text("stop the empty-queue crash".into())
        );

        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        let written = host.commit_draft().expect("the draft is there");
        assert_eq!(written.message, "fix(queue): stop the empty-queue crash");
        assert_eq!(written.revision, 5, "the message is written once");
        assert_eq!(
            host.refreshed_views(),
            vec![VIEW_ID.to_owned(), VIEW_ID.to_owned()],
            "the selected type and then the apply status are redrawn"
        );
    }

    fn storage_host() -> MockHost {
        MockHost::install().with_capabilities(&["vcs.read", "vcs.write", "storage"])
    }

    #[test]
    fn a_draft_left_behind_survives_a_restart() {
        let host = storage_host();
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();
        drop(editor);

        let mut reopened = Editor::default();
        let view = reopened.render(PROJECT, &config, strings);
        assert_eq!(
            value_of(&view, "subject"),
            FieldValue::Text("stop the empty-queue crash".into()),
            "the draft comes back after the instance is dropped"
        );
        assert!(host.storage().contains_key("editor:/repo"));
    }

    #[test]
    fn a_draft_belongs_to_its_project() {
        let _host = storage_host();
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();

        let view = editor.render("/other-repo", &config, strings);
        assert_eq!(
            value_of(&view, "subject"),
            FieldValue::Text(String::new()),
            "another repository starts empty"
        );

        let view = editor.render(PROJECT, &config, strings);
        assert_eq!(
            value_of(&view, "subject"),
            FieldValue::Text("stop the empty-queue crash".into()),
            "going back restores the first repository's draft"
        );
    }

    #[test]
    fn applying_clears_the_stored_draft() {
        let host = storage_host();
        host.set_commit_draft(Some(draft()));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();
        assert!(
            host.storage().contains_key("editor:/repo"),
            "the draft is kept while it is being edited"
        );
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert!(
            !host.storage().contains_key("editor:/repo"),
            "an applied draft is not offered again: {:?}",
            host.storage()
        );
    }

    #[test]
    fn clearing_removes_the_stored_draft() {
        let host = storage_host();
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the empty-queue crash".into()),
                &config,
                strings,
            )
            .unwrap();
        assert!(
            host.storage().contains_key("editor:/repo"),
            "the draft is kept while it is being edited"
        );
        editor
            .handle(PROJECT, ViewEvent::Button(CLEAR.into()), &config, strings)
            .unwrap();
        assert!(!host.storage().contains_key("editor:/repo"));
    }

    #[test]
    fn applying_without_a_subject_writes_nothing() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(openidea_plugin::CommitDraft {
            revision: 1,
            message: "mine".into(),
            amend: false,
            changes: Vec::new(),
        }));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "   ".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert_eq!(
            host.commit_draft().expect("the draft is there").message,
            "mine"
        );
        assert_eq!(
            host.notifications().last().expect("it reports").level,
            NotificationLevel::Warning
        );
    }

    #[test]
    fn applying_replaces_what_the_commit_field_holds() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        host.set_commit_draft(Some(openidea_plugin::CommitDraft {
            revision: 9,
            message: "an older draft".into(),
            amend: false,
            changes: Vec::new(),
        }));
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the crash".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert_eq!(
            host.commit_draft().expect("the draft is there").message,
            "feat: stop the crash",
            "Apply is deliberate: the form replaces the message field"
        );
    }

    #[test]
    fn no_repository_is_reported_without_writing() {
        let host = MockHost::install().with_capabilities(&["vcs.read", "vcs.write"]);
        let strings = strings();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "stop the crash".into()),
                &config,
                strings,
            )
            .unwrap();
        editor
            .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings)
            .unwrap();
        assert!(
            host.notifications()
                .last()
                .expect("it reports")
                .body
                .contains("No Git repository"),
            "the form says why nothing was written"
        );
    }
    #[test]
    fn choice_and_checkbox_changes_refresh_the_visible_values() {
        let host = storage_host();
        let config = Config::load();
        let mut editor = Editor::default();
        editor.render(PROJECT, &config, strings());
        for (key, value) in [
            ("type", "fix"),
            ("skipCi", "[ci skip]"),
            ("skipCiOn", "true"),
            ("skipCiOn", "false"),
        ] {
            let before = host.refreshed_views().len();
            editor
                .handle(
                    PROJECT,
                    ViewEvent::Field(key.into(), value.into()),
                    &config,
                    strings(),
                )
                .unwrap();
            assert_eq!(
                host.refreshed_views().len(),
                before + 1,
                "{key} needs a refreshed view"
            );
            let view = editor.render(PROJECT, &config, strings());
            match value_of(&view, key) {
                FieldValue::Choice(_, chosen) => assert_eq!(chosen, value),
                FieldValue::Checkbox(checked) => assert_eq!(checked, value == "true"),
                other => panic!("unexpected control: {other:?}"),
            }
        }
    }

    #[test]
    fn text_changes_keep_the_hosts_input_in_place() {
        let host = storage_host();
        let config = Config::load();
        let mut editor = Editor::default();
        editor
            .handle(
                PROJECT,
                ViewEvent::Field("subject".into(), "still typing".into()),
                &config,
                strings(),
            )
            .unwrap();
        assert!(host.refreshed_views().is_empty());
        assert_eq!(
            value_of(&editor.render(PROJECT, &config, strings()), "subject"),
            FieldValue::Text("still typing".into())
        );
    }

    #[test]
    fn invalid_headers_keep_the_existing_commit_and_editor_draft() {
        for (key, value) in [
            ("scope", "core)"),
            ("scope", "core\t"),
            ("subject", "first\nsecond"),
            ("type", "not-allowed"),
        ] {
            let host = storage_host();
            let mut original = draft();
            original.message = "keep my draft".into();
            host.set_commit_draft(Some(original));
            let config = Config::load();
            let mut editor = Editor::default();
            for (field, text) in [("subject", "stop the crash"), (key, value)] {
                editor
                    .handle(
                        PROJECT,
                        ViewEvent::Field(field.into(), text.into()),
                        &config,
                        strings(),
                    )
                    .unwrap();
            }
            editor
                .handle(PROJECT, ViewEvent::Button(APPLY.into()), &config, strings())
                .unwrap();
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
            assert_eq!(
                value_of(&editor.render(PROJECT, &config, strings()), "subject"),
                FieldValue::Text(
                    if key == "subject" {
                        value
                    } else {
                        "stop the crash"
                    }
                    .into()
                )
            );
        }
    }
}
