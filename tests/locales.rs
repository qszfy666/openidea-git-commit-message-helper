//! Every text the IDE shows from the manifest and the settings schemas has an
//! entry in each shipped catalog; the IDE shows text without an entry in
//! English, whatever the interface language.

use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// The interface languages the plugin translates.
const CATALOGS: [&str; 3] = ["zh-CN", "ja", "ko"];

/// The manifest keys the IDE translates in the contribution sections.
const MANIFEST_TEXT_KEYS: [&str; 4] = ["title", "category", "label", "description"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `key = "value"` lines of the `contributes` sections. The manifest writes
/// every shown text as a one-line basic string; the package `description`
/// above them is not translated by the IDE.
fn manifest_texts(out: &mut BTreeSet<String>) {
    let text = std::fs::read_to_string(root().join("plugin.toml")).unwrap();
    let mut in_contributes = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_contributes = line.trim_start_matches('[').starts_with("contributes");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if in_contributes && MANIFEST_TEXT_KEYS.contains(&key.trim()) {
            let value: String = serde_json::from_str(value.trim()).unwrap();
            out.insert(value);
        }
    }
}

/// The `title`, `description` and `group` of a setting, its table columns and
/// its nested properties, as the IDE localizes them.
fn schema_texts(setting: &Map<String, Value>, out: &mut BTreeSet<String>) {
    for key in ["title", "description", "group"] {
        if let Some(Value::String(text)) = setting.get(key) {
            out.insert(text.clone());
        }
    }
    if let Some(Value::Object(items)) = setting.get("items") {
        schema_texts(items, out);
    }
    if let Some(Value::Object(properties)) = setting.get("properties") {
        for property in properties.values() {
            schema_texts(property.as_object().unwrap(), out);
        }
    }
}

fn shown_texts() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    manifest_texts(&mut out);
    for entry in std::fs::read_dir(root().join("settings")).unwrap() {
        let path = entry.unwrap().path();
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        for property in properties.values() {
            schema_texts(property.as_object().unwrap(), &mut out);
        }
    }
    out
}

#[test]
fn every_catalog_translates_every_shown_text() {
    let shown = shown_texts();
    assert!(
        shown.contains("Commit message language"),
        "the schemas were not read"
    );
    assert!(shown.contains("LLM API key"), "the manifest was not read");
    for tag in CATALOGS {
        let path = root().join("locales").join(format!("{tag}.json"));
        let catalog: Map<String, Value> = std::fs::read_to_string(&path)
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap_or_default();
        let missing: Vec<&String> = shown
            .iter()
            .filter(|text| !catalog.contains_key(*text))
            .collect();
        assert!(
            missing.is_empty(),
            "locales/{tag}.json misses {} texts: {missing:#?}",
            missing.len()
        );
    }
}
