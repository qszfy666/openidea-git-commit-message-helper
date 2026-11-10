//! Per-model thinking parameters (checklist items 24–28).
//!
//! A model profile picks a thinking level and a parameter preset. A preset is
//! a JSON object keyed by level; each value is a fragment merged into the
//! request body. A custom mapping overrides the preset's template. `DEFAULT`
//! and the `model-default` preset send nothing, so 0.7.x requests stay
//! unchanged until the user opts in.
//!
//! The host's declarative settings cannot show conditional fields, so
//! [`available_levels`] computes the levels the mapping really offers; the
//! **Thinking Levels** action displays them and [`resolve`] refuses a level
//! the mapping does not define.

use crate::config::Protocol;
use serde_json::{Map, Value, json};

/// How much reasoning the profile asks the model for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Send no thinking parameter; the provider applies its own default.
    Default,
    Disabled,
    /// For APIs with an on/off switch rather than graded effort.
    Enabled,
    Low,
    Medium,
    High,
    Max,
}

impl Level {
    /// The levels a mapping may define, in the order shown in the UI.
    pub const SPECIFIED: [Self; 6] = [
        Self::Disabled,
        Self::Enabled,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Max,
    ];

    /// Values come from a settings enum; unknown values fall back to `DEFAULT`.
    pub fn parse(value: Option<&str>) -> Self {
        value.and_then(Self::from_key).unwrap_or(Self::Default)
    }

    /// The mapping key of a level other than `DEFAULT`.
    fn from_key(value: &str) -> Option<Self> {
        Self::SPECIFIED
            .into_iter()
            .find(|level| level.key() == value.trim())
    }

    /// The mapping key, also the value in the settings enum.
    pub fn key(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::Disabled => "DISABLED",
            Self::Enabled => "ENABLED",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Max => "MAX",
        }
    }

    /// Share of the response budget spent on `thinking.budget_tokens`.
    fn budget_share(self) -> u32 {
        match self {
            Self::Enabled | Self::Low => 25,
            Self::Medium => 50,
            Self::High => 75,
            Self::Max => 90,
            Self::Default | Self::Disabled => 0,
        }
    }
}

/// The request-fragment family a model/gateway understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Send nothing (the 0.7.x behaviour).
    ModelDefault,
    /// Standard parameter family of the profile's protocol.
    Auto,
    OpenAiEffort,
    NestedEffort,
    ThinkingObject,
    EnableThinking,
    AnthropicBudget,
    AnthropicAdaptive,
    Custom,
}

impl Preset {
    #[cfg(test)]
    pub const ALL: [Self; 9] = [
        Self::ModelDefault,
        Self::Auto,
        Self::OpenAiEffort,
        Self::NestedEffort,
        Self::ThinkingObject,
        Self::EnableThinking,
        Self::AnthropicBudget,
        Self::AnthropicAdaptive,
        Self::Custom,
    ];

    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("auto") => Self::Auto,
            Some("openai-effort") => Self::OpenAiEffort,
            Some("nested-effort") => Self::NestedEffort,
            Some("thinking-object") => Self::ThinkingObject,
            Some("enable-thinking") => Self::EnableThinking,
            Some("anthropic-budget") => Self::AnthropicBudget,
            Some("anthropic-adaptive") => Self::AnthropicAdaptive,
            Some("custom") => Self::Custom,
            _ => Self::ModelDefault,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::ModelDefault => "model-default",
            Self::Auto => "auto",
            Self::OpenAiEffort => "openai-effort",
            Self::NestedEffort => "nested-effort",
            Self::ThinkingObject => "thinking-object",
            Self::EnableThinking => "enable-thinking",
            Self::AnthropicBudget => "anthropic-budget",
            Self::AnthropicAdaptive => "anthropic-adaptive",
            Self::Custom => "custom",
        }
    }
}

/// The thinking part of a resolved [`Config`](crate::config::Config).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub level: Level,
    pub preset: Preset,
    /// `None` means "use the preset's template"; a trimmed-empty column is
    /// treated the same way at the settings boundary.
    pub mapping: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            level: Level::Default,
            preset: Preset::ModelDefault,
            mapping: None,
        }
    }
}

/// What one request sends and what the diagnostics say about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    /// Request-body fields to merge, or `None` when thinking is omitted.
    pub fragment: Option<Value>,
    /// A line for the request diagnostics; `None` when nothing was configured.
    pub note: Option<String>,
}

const BUDGET_PLACEHOLDER: &str = "$thinking_budget";
const MAX_MAPPING_LENGTH: usize = 65_536;
const MAX_JSON_DEPTH: usize = 12;
const MIN_THINKING_BUDGET: u32 = 1_024;
const THINKING_OUTPUT_RESERVE: u32 = 256;
/// Fields the plugin owns; a mapping may not replace the prompt or its limits.
const RESERVED_REQUEST_FIELDS: [&str; 15] = [
    "model",
    "messages",
    "system",
    "input",
    "instructions",
    "stream",
    "temperature",
    "max_tokens",
    "max_completion_tokens",
    "max_output_tokens",
    "tools",
    "tool_choice",
    "functions",
    "function_call",
    "response_format",
];

/// The default mapping of a preset. `auto` follows the profile's protocol;
/// custom starts empty because the user supplies the whole mapping.
pub fn preset_template(preset: Preset, protocol: Protocol) -> String {
    const GRADED: [(Level, &str); 3] = [
        (Level::Low, "low"),
        (Level::Medium, "medium"),
        (Level::High, "high"),
    ];
    const WITH_MAX: [(Level, &str); 4] = [
        (Level::Low, "low"),
        (Level::Medium, "medium"),
        (Level::High, "high"),
        // OpenAI-style endpoints know `high`, not `max`; a gateway that does
        // support `max` can put it in a custom mapping.
        (Level::Max, "high"),
    ];
    let mut mapping = Map::new();
    let insert = |mapping: &mut Map<String, Value>, level: Level, fragment: Value| {
        mapping.insert(level.key().to_owned(), fragment);
    };
    match preset {
        Preset::ModelDefault | Preset::Custom => {}
        Preset::Auto => match protocol {
            Protocol::OpenAiChat => {
                insert(
                    &mut mapping,
                    Level::Disabled,
                    json!({"reasoning_effort": "none"}),
                );
                for (level, effort) in WITH_MAX {
                    insert(&mut mapping, level, json!({ "reasoning_effort": effort }));
                }
            }
            Protocol::OpenAiResponses => {
                insert(
                    &mut mapping,
                    Level::Disabled,
                    json!({"reasoning": {"effort": "none"}}),
                );
                for (level, effort) in WITH_MAX {
                    insert(
                        &mut mapping,
                        level,
                        json!({"reasoning": {"effort": effort}}),
                    );
                }
            }
            Protocol::Anthropic => {
                insert(
                    &mut mapping,
                    Level::Disabled,
                    thinking_fragment("disabled", false),
                );
                for (level, _) in WITH_MAX {
                    insert(&mut mapping, level, thinking_fragment("enabled", true));
                }
            }
        },
        Preset::OpenAiEffort => {
            for (level, effort) in GRADED {
                insert(&mut mapping, level, json!({ "reasoning_effort": effort }));
            }
        }
        Preset::NestedEffort => {
            for (level, effort) in GRADED {
                insert(
                    &mut mapping,
                    level,
                    json!({"reasoning": {"effort": effort}}),
                );
            }
        }
        Preset::ThinkingObject => {
            insert(
                &mut mapping,
                Level::Disabled,
                thinking_fragment("disabled", false),
            );
            insert(
                &mut mapping,
                Level::Enabled,
                thinking_fragment("enabled", false),
            );
        }
        Preset::EnableThinking => {
            insert(
                &mut mapping,
                Level::Disabled,
                json!({"enable_thinking": false}),
            );
            insert(
                &mut mapping,
                Level::Enabled,
                json!({"enable_thinking": true}),
            );
        }
        Preset::AnthropicBudget => {
            insert(
                &mut mapping,
                Level::Disabled,
                thinking_fragment("disabled", false),
            );
            for (level, _) in WITH_MAX {
                insert(&mut mapping, level, thinking_fragment("enabled", true));
            }
        }
        Preset::AnthropicAdaptive => {
            for (level, effort) in GRADED {
                insert(
                    &mut mapping,
                    level,
                    json!({
                        "thinking": {"type": "adaptive"},
                        "output_config": {"effort": effort}
                    }),
                );
            }
        }
    }
    serde_json::to_string_pretty(&Value::Object(mapping))
        .expect("a preset template is a JSON object")
}

fn thinking_fragment(kind: &str, budget: bool) -> Value {
    if budget {
        json!({"thinking": {"type": kind, "budget_tokens": BUDGET_PLACEHOLDER}})
    } else {
        json!({"thinking": {"type": kind}})
    }
}

/// A filled-in mapping column overrides the preset template; clearing it
/// restores the preset.
pub fn mapping_text(settings: &Settings, protocol: Protocol) -> String {
    settings
        .mapping
        .clone()
        .unwrap_or_else(|| preset_template(settings.preset, protocol))
}

/// Validates a mapping's shape, level names, fragments, reserved fields,
/// placeholders, nesting depth and size.
pub fn validate(text: &str) -> Result<Value, String> {
    const SHAPE: &str = "Mapping must be a JSON object keyed by thinking level.";
    if text.len() > MAX_MAPPING_LENGTH {
        return Err("Mapping is too large (64 KiB limit).".to_owned());
    }
    if text.trim().is_empty() {
        return Err(SHAPE.to_owned());
    }
    let value: Value = serde_json::from_str(text).map_err(|_| SHAPE.to_owned())?;
    let Value::Object(map) = &value else {
        return Err(SHAPE.to_owned());
    };
    for (key, fragment) in map {
        if key == Level::Default.key() {
            return Err("DEFAULT is implicit and cannot be mapped.".to_owned());
        }
        Level::from_key(key).ok_or_else(|| format!("Unknown thinking level '{key}'."))?;
        let Value::Object(fields) = fragment else {
            return Err(format!("{key} must map to a non-empty JSON object."));
        };
        if fields.is_empty() {
            return Err(format!("{key} must map to a non-empty JSON object."));
        }
        for field in fields.keys() {
            if RESERVED_REQUEST_FIELDS.contains(&field.as_str()) {
                return Err(format!(
                    "The mapping cannot override the request field '{field}'."
                ));
            }
        }
        check_fragment(fragment, 0, &[])?;
    }
    Ok(value)
}

/// Rejects null values, invalid placeholders and nesting beyond the limit.
fn check_fragment(value: &Value, depth: usize, path: &[&str]) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err(format!(
            "Mapping nesting depth exceeds {MAX_JSON_DEPTH} levels at '{}'.",
            path.join(".")
        ));
    }
    match value {
        Value::Null => Err(format!(
            "null parameter values are not allowed (at '{}').",
            path.join(".")
        )),
        Value::String(text) if text.starts_with('$') => {
            if text == BUDGET_PLACEHOLDER && path == ["thinking", "budget_tokens"] {
                Ok(())
            } else {
                Err(format!(
                    "The {BUDGET_PLACEHOLDER} placeholder is only valid at thinking.budget_tokens (found at '{}').",
                    path.join(".")
                ))
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                let mut child_path = path.to_vec();
                child_path.push(key);
                check_fragment(child, depth + 1, &child_path)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            let mut child_path = path.to_vec();
            child_path.push("[]");
            for child in items {
                check_fragment(child, depth + 1, &child_path)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The levels the current preset/mapping actually offers (item 27), and a
/// validation error when the mapping cannot be used at all.
pub fn available_levels(
    settings: &Settings,
    protocol: Protocol,
    max_tokens: u32,
) -> (Vec<Level>, Option<String>) {
    let mut levels = vec![Level::Default];
    if settings.preset == Preset::ModelDefault {
        return (levels, None);
    }
    let mapping = match validate(&mapping_text(settings, protocol)) {
        Ok(mapping) => mapping,
        Err(error) => return (levels, Some(error)),
    };
    for level in Level::SPECIFIED {
        if let Some(fragment) = mapping.get(level.key())
            && fragment_for(fragment, level, max_tokens).is_ok()
        {
            levels.push(level);
        }
    }
    (levels, None)
}

/// The fragment the selected level sends, or the reason it was omitted.
pub fn resolve(settings: &Settings, protocol: Protocol, max_tokens: u32) -> Resolution {
    if settings.level == Level::Default || settings.preset == Preset::ModelDefault {
        return Resolution {
            fragment: None,
            note: None,
        };
    }
    let mapping = match validate(&mapping_text(settings, protocol)) {
        Ok(mapping) => mapping,
        Err(error) => {
            return Resolution {
                fragment: None,
                note: Some(format!("invalid mapping ({error}); omitted")),
            };
        }
    };
    let key = settings.level.key();
    let Some(fragment) = mapping.get(key) else {
        return Resolution {
            fragment: None,
            note: Some(format!("{key} has no mapping; omitted")),
        };
    };
    match fragment_for(fragment, settings.level, max_tokens) {
        Ok(fragment) => Resolution {
            note: Some(format!(
                "{key} → {}",
                crate::ui::clip(&fragment.to_string(), 160)
            )),
            fragment: Some(fragment),
        },
        Err(error) => Resolution {
            fragment: None,
            note: Some(format!("{key} unavailable ({error}); omitted")),
        },
    }
}

/// Resolves `thinking.budget_tokens` and checks it against the response limit
/// (item 28). The fragment is otherwise sent verbatim.
fn fragment_for(fragment: &Value, level: Level, max_tokens: u32) -> Result<Value, String> {
    let mut out = fragment.clone();
    if let Some(thinking) = out.get_mut("thinking")
        && let Some(budget) = thinking.get_mut("budget_tokens")
    {
        let value = match budget {
            Value::String(text) if text == BUDGET_PLACEHOLDER => budget_for(level, max_tokens)?,
            Value::Number(number) => number
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| "thinking.budget_tokens must be an integer.".to_owned())?,
            _ => {
                return Err(
                    "thinking.budget_tokens must be an integer or $thinking_budget.".to_owned(),
                );
            }
        };
        if value < MIN_THINKING_BUDGET || value >= max_tokens {
            return Err(format!(
                "thinking.budget_tokens must be at least {MIN_THINKING_BUDGET} and less than max response tokens ({max_tokens})."
            ));
        }
        *budget = json!(value);
    }
    Ok(out)
}

/// A graded level spends a growing share of the response budget on thinking,
/// leaving room for the answer. Anthropic requires at least 1024 tokens and a
/// budget strictly below `max_tokens`.
fn budget_for(level: Level, max_tokens: u32) -> Result<u32, String> {
    let upper = max_tokens.saturating_sub(THINKING_OUTPUT_RESERVE);
    if upper < MIN_THINKING_BUDGET {
        return Err(format!(
            "thinking.budget_tokens needs max response tokens above {} (configured: {max_tokens}).",
            MIN_THINKING_BUDGET + THINKING_OUTPUT_RESERVE
        ));
    }
    let share = u64::from(level.budget_share());
    let budget = (u64::from(max_tokens) * share / 100) as u32;
    Ok(budget.max(MIN_THINKING_BUDGET).min(upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(preset: Preset, level: Level, mapping: Option<&str>) -> Settings {
        Settings {
            level,
            preset,
            mapping: mapping.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn preset_templates_cover_the_parameter_families() {
        let chat = preset_template(Preset::Auto, Protocol::OpenAiChat);
        assert!(chat.contains("reasoning_effort"), "{chat}");
        let responses = preset_template(Preset::Auto, Protocol::OpenAiResponses);
        assert!(responses.contains("\"reasoning\""), "{responses}");
        let anthropic = preset_template(Preset::Auto, Protocol::Anthropic);
        assert!(anthropic.contains(BUDGET_PLACEHOLDER), "{anthropic}");
        assert!(preset_template(Preset::NestedEffort, Protocol::OpenAiChat).contains("\"effort\""));
        assert!(
            preset_template(Preset::ThinkingObject, Protocol::OpenAiChat).contains("\"disabled\"")
        );
        assert!(preset_template(Preset::EnableThinking, Protocol::OpenAiChat).contains("false"));
        assert!(
            preset_template(Preset::AnthropicAdaptive, Protocol::Anthropic)
                .contains("\"output_config\"")
        );
        for preset in Preset::ALL {
            let template = preset_template(preset, Protocol::OpenAiChat);
            assert!(validate(&template).is_ok(), "{}: {template}", preset.key());
        }
    }

    #[test]
    fn custom_mapping_overrides_the_preset_and_clearing_restores_it() {
        let custom = settings(
            Preset::OpenAiEffort,
            Level::High,
            Some(r#"{"HIGH":{"x":1}}"#),
        );
        assert_eq!(
            mapping_text(&custom, Protocol::OpenAiChat),
            r#"{"HIGH":{"x":1}}"#
        );
        let preset = settings(Preset::OpenAiEffort, Level::High, None);
        assert!(
            mapping_text(&preset, Protocol::OpenAiChat).contains("reasoning_effort"),
            "{}",
            mapping_text(&preset, Protocol::OpenAiChat)
        );
    }

    #[test]
    fn validation_rejects_placeholder_keys_that_only_look_like_nested_paths() {
        for mapping in [
            r#"{"HIGH":{"thinking.budget_tokens":"$thinking_budget"}}"#,
            r#"{"HIGH":{"":{"thinking.budget_tokens":"$thinking_budget"}}}"#,
            r#"{"HIGH":{"thinking":[{"budget_tokens":"$thinking_budget"}]}}"#,
        ] {
            let error = validate(mapping).expect_err("the placeholder has no valid budget path");
            assert!(
                error.contains("only valid at thinking.budget_tokens"),
                "{error}"
            );
        }
    }

    #[test]
    fn validation_rejects_bad_mappings() {
        let cases = [
            ("[]", "JSON object"),
            ("{", "JSON object"),
            (r#"{"BOGUS":{"x":1}}"#, "Unknown thinking level"),
            (r#"{"DEFAULT":{"x":1}}"#, "DEFAULT"),
            (r#"{"HIGH":{}}"#, "non-empty"),
            (r#"{"HIGH":{"x":null}}"#, "null"),
            (r#"{"HIGH":{"max_tokens":10}}"#, "override"),
            (r#"{"HIGH":{"x":"$thinking_budget"}}"#, "$thinking_budget"),
        ];
        for (text, expected) in cases {
            let error = validate(text).expect_err("{text} should be rejected");
            assert!(error.contains(expected), "{text}: {error}");
        }

        let mut deep = String::from(r#"{"HIGH":{"x":"#);
        for _ in 0..14 {
            deep.push_str("{\"a\":");
        }
        deep.push('1');
        for _ in 0..14 {
            deep.push('}');
        }
        deep.push_str("}}");
        assert!(validate(&deep).unwrap_err().contains("depth"), "{deep}");

        let huge = format!(r#"{{"HIGH":{{"x":"{}"}}}}"#, "a".repeat(MAX_MAPPING_LENGTH));
        assert!(validate(&huge).unwrap_err().contains("too large"));
    }

    #[test]
    fn available_levels_follow_the_mapping() {
        let default = settings(Preset::ModelDefault, Level::High, None);
        assert_eq!(
            available_levels(&default, Protocol::OpenAiChat, 4096).0,
            vec![Level::Default]
        );

        let on_off = settings(
            Preset::Custom,
            Level::Enabled,
            Some(r#"{"ENABLED":{"enable_thinking":true}}"#),
        );
        let (levels, error) = available_levels(&on_off, Protocol::OpenAiChat, 4096);
        assert_eq!(levels, vec![Level::Default, Level::Enabled]);
        assert!(error.is_none());

        let broken = settings(Preset::Custom, Level::High, Some("[]"));
        let (levels, error) = available_levels(&broken, Protocol::OpenAiChat, 4096);
        assert_eq!(levels, vec![Level::Default]);
        assert!(error.is_some());
    }

    #[test]
    fn the_selected_level_reaches_the_request_fragment() {
        let settings = settings(Preset::OpenAiEffort, Level::High, None);
        let resolved = resolve(&settings, Protocol::OpenAiChat, 4096);
        assert_eq!(
            resolved.fragment.expect("HIGH is mapped")["reasoning_effort"],
            "high"
        );
        assert!(resolved.note.unwrap().contains("HIGH"));
    }

    #[test]
    fn default_and_model_default_send_nothing() {
        let default = settings(Preset::OpenAiEffort, Level::Default, None);
        assert_eq!(resolve(&default, Protocol::OpenAiChat, 4096).fragment, None);
        let model_default = settings(Preset::ModelDefault, Level::High, None);
        assert_eq!(
            resolve(&model_default, Protocol::OpenAiChat, 4096).fragment,
            None
        );
    }

    #[test]
    fn a_level_without_a_mapping_is_omitted() {
        let settings = settings(
            Preset::Custom,
            Level::Low,
            Some(r#"{"ENABLED":{"enable_thinking":true}}"#),
        );
        let resolved = resolve(&settings, Protocol::OpenAiChat, 4096);
        assert_eq!(resolved.fragment, None);
        assert!(
            resolved.note.unwrap().contains("LOW"),
            "the diagnostics say why nothing was sent"
        );
    }

    #[test]
    fn the_budget_placeholder_is_resolved_from_the_response_limit() {
        let settings = settings(Preset::AnthropicBudget, Level::High, None);
        let fragment = resolve(&settings, Protocol::Anthropic, 4096)
            .fragment
            .expect("HIGH has a budget fragment");
        assert_eq!(fragment["thinking"]["type"], "enabled");
        assert_eq!(fragment["thinking"]["budget_tokens"], 3_072);
    }

    #[test]
    fn a_budget_outside_the_response_limit_is_rejected() {
        for raw in [
            r#"{"HIGH":{"thinking":{"type":"enabled","budget_tokens":5000}}}"#,
            r#"{"HIGH":{"thinking":{"type":"enabled","budget_tokens":512}}}"#,
            r#"{"HIGH":{"thinking":{"type":"enabled","budget_tokens":"$other"}}}"#,
        ] {
            let settings = settings(Preset::Custom, Level::High, Some(raw));
            let resolved = resolve(&settings, Protocol::Anthropic, 4096);
            assert_eq!(resolved.fragment, None, "{raw}");
            assert!(
                resolved.note.unwrap().contains("budget_tokens"),
                "{raw}: the diagnostics say why"
            );
        }
        let settings = settings(Preset::AnthropicBudget, Level::High, None);
        assert_eq!(
            resolve(&settings, Protocol::Anthropic, 1_024).fragment,
            None,
            "the response limit cannot hold a valid budget"
        );
    }
}
