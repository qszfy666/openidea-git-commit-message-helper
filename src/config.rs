//! Plugin settings and the provider presets derived from them.
//!
//! The IDE renders the settings pages from `settings/*.json`; the plugin reads
//! the values through `host::setting*` (project value over global over the
//! schema default).

use crate::thinking;
use openidea_plugin::host;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLanguage {
    Zh,
    En,
}

impl UiLanguage {
    pub fn load() -> Self {
        if host::locale().to_ascii_lowercase().starts_with("zh") {
            Self::Zh
        } else {
            Self::En
        }
    }

    pub fn is_zh(self) -> bool {
        self == Self::Zh
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLanguage {
    Auto,
    Chinese,
    English,
}

impl MessageLanguage {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("Chinese") => Self::Chinese,
            Some("English") => Self::English,
            _ => Self::Auto,
        }
    }

    /// The language written into the prompt. `Auto` follows the IDE UI
    /// language: plugin API 1.3 has no way to read the repository's commits.
    pub fn prompt_language(self) -> &'static str {
        match self {
            Self::Chinese => "Simplified Chinese",
            Self::English => "English",
            Self::Auto if UiLanguage::load().is_zh() => "Simplified Chinese",
            Self::Auto => "English",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    OpenAiChat,
    OpenAiResponses,
    Anthropic,
}

impl Protocol {
    fn parse(value: Option<&str>) -> Option<Self> {
        match value {
            Some("openai-chat") => Some(Self::OpenAiChat),
            Some("openai-responses") => Some(Self::OpenAiResponses),
            Some("anthropic") => Some(Self::Anthropic),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    OpenAi,
    Anthropic,
    DeepSeek,
    Qwen,
    Kimi,
    Glm,
    MiniMax,
    SiliconFlow,
    Volcengine,
    AzureOpenAi,
    OpenRouter,
    Groq,
    Mistral,
    Ollama,
    Magpie,
    Custom,
}

impl Provider {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("OpenAI") => Self::OpenAi,
            Some("Anthropic") => Self::Anthropic,
            Some("Qwen") => Self::Qwen,
            Some("Kimi") => Self::Kimi,
            Some("GLM") => Self::Glm,
            Some("MiniMax") => Self::MiniMax,
            Some("SiliconFlow") => Self::SiliconFlow,
            Some("Volcengine") => Self::Volcengine,
            Some("Azure OpenAI") => Self::AzureOpenAi,
            Some("OpenRouter") => Self::OpenRouter,
            Some("Groq") => Self::Groq,
            Some("Mistral") => Self::Mistral,
            Some("Ollama") => Self::Ollama,
            Some("Magpie") => Self::Magpie,
            Some("Custom") => Self::Custom,
            _ => Self::DeepSeek,
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Self::Kimi => "https://api.moonshot.cn/v1",
            Self::Glm => "https://open.bigmodel.cn/api/paas/v4",
            Self::MiniMax => "https://api.minimax.chat/v1",
            Self::SiliconFlow => "https://api.siliconflow.cn/v1",
            Self::Volcengine => "https://ark.cn-beijing.volces.com/api/v3",
            Self::AzureOpenAi => "",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::Ollama => "http://127.0.0.1:11434/v1",
            Self::Magpie => "http://127.0.0.1:3425",
            Self::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4o-mini",
            Self::Anthropic => "claude-sonnet-4-5",
            Self::DeepSeek => "deepseek-chat",
            Self::Qwen => "qwen-plus",
            Self::Kimi => "moonshot-v1-8k",
            Self::Glm => "glm-4-plus",
            Self::MiniMax => "MiniMax-M2",
            Self::SiliconFlow => "deepseek-ai/DeepSeek-V3",
            Self::Volcengine => "doubao-seed-1-6",
            Self::AzureOpenAi => "",
            Self::OpenRouter => "openai/gpt-4o-mini",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Mistral => "mistral-large-latest",
            Self::Ollama => "llama3.1",
            Self::Magpie => "",
            Self::Custom => "",
        }
    }

    pub fn default_protocol(self) -> Protocol {
        match self {
            Self::Anthropic => Protocol::Anthropic,
            _ => Protocol::OpenAiChat,
        }
    }
}

/// One allowed commit type: `title` is what the model must return,
/// `description` explains when to choose it (settings table and prompt).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitType {
    pub title: String,
    pub description: String,
}

fn commit_type(title: &str, description: &str) -> CommitType {
    CommitType {
        title: title.trim().to_owned(),
        description: description.trim().to_owned(),
    }
}

fn default_commit_types() -> Vec<CommitType> {
    [
        ("feat", "A new feature"),
        ("fix", "A bug fix"),
        ("docs", "Documentation only"),
        ("style", "Formatting or style, no behaviour change"),
        (
            "refactor",
            "Code change that neither fixes a bug nor adds a feature",
        ),
        ("perf", "Performance improvement"),
        ("test", "Tests only"),
        ("build", "Build system or dependencies"),
        ("ci", "CI configuration"),
        ("chore", "Maintenance tasks"),
        ("revert", "Reverts an earlier commit"),
    ]
    .into_iter()
    .map(|(title, description)| commit_type(title, description))
    .collect()
}

/// `commitTypes` is a table of title/description objects. Settings saved by
/// 0.6.x used a string array, older ones one comma-separated string; both are
/// still accepted, with an empty description.
fn commit_types() -> Vec<CommitType> {
    let Some(raw) = host::setting("commitTypes") else {
        return default_commit_types();
    };
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    let types: Vec<CommitType> = match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(title) => Some(commit_type(&title, "")),
                Value::Object(_) => Some(commit_type(
                    item.get("title").and_then(Value::as_str).unwrap_or(""),
                    item.get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )),
                _ => None,
            })
            .filter(|item| !item.title.is_empty())
            .collect(),
        Value::String(text) => text
            .split(',')
            .map(|title| commit_type(title, ""))
            .filter(|item| !item.title.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    if types.is_empty() {
        default_commit_types()
    } else {
        types
    }
}

/// Reads a declarative table setting. The IDE saves each row as a complete
/// object; rows of an unexpected shape are ignored by the callers.
fn setting_json(key: &str) -> Option<Value> {
    host::setting(key).and_then(|raw| serde_json::from_str(&raw).ok())
}

/// The row of a named-profile table selected by its selector setting. A blank
/// selector leaves the single settings in charge; a name no row has also does.
fn selected(key: &str, selector: &str) -> Option<Value> {
    let name = host::setting_string(selector)?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let Value::Array(items) = setting_json(key)? else {
        return None;
    };
    items.into_iter().find(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .is_some_and(|candidate| candidate.trim() == name)
    })
}

/// A field of the active profile, or the single setting with the same key.
fn profile_text(profile: Option<&Value>, key: &str) -> Option<String> {
    profile
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| host::setting_string(key))
}

fn profile_number(profile: Option<&Value>, key: &str) -> Option<f64> {
    profile
        .and_then(|value| value.get(key))
        .and_then(Value::as_f64)
        .or_else(|| host::setting_number(key))
}

fn profile_bool(profile: Option<&Value>, key: &str) -> Option<bool> {
    profile
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .or_else(|| host::setting_bool(key))
}

/// Secrets must be declared in plugin.toml, so a profile can only name one of
/// the four fixed slots. Anything else falls back to the original key.
fn secret_slot(profile: Option<&Value>) -> String {
    const SLOTS: [&str; 4] = ["api-key", "api-key-2", "api-key-3", "api-key-4"];
    let candidate = profile_text(profile, "secretSlot").unwrap_or_default();
    let candidate = candidate.trim();
    if SLOTS.contains(&candidate) {
        candidate.to_owned()
    } else {
        "api-key".to_owned()
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The active model profile, or the single LLM settings when none is
    /// selected: every request reads only these resolved values.
    pub provider: Provider,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub send_temperature: bool,
    pub max_tokens: u32,
    /// Which declared plugin secret the request carries.
    pub secret_slot: String,
    pub language: MessageLanguage,
    pub max_diff_chars: usize,
    pub commit_types: Vec<CommitType>,
    /// The selected prompt profile's text, or the single extra system prompt.
    pub extra_prompt: String,
    /// The selected output template; `None` is the built-in Default.
    pub template: Option<String>,
    /// Thinking level, parameter preset and optional custom mapping.
    pub thinking: thinking::Settings,
}

impl Config {
    pub fn load() -> Self {
        let model = selected("modelProfiles", "activeModel");
        let provider = Provider::parse(profile_text(model.as_ref(), "provider").as_deref());
        let protocol = Protocol::parse(profile_text(model.as_ref(), "protocol").as_deref())
            .unwrap_or_else(|| provider.default_protocol());

        let prompt = selected("promptProfiles", "defaultPrompt");
        let extra_prompt = match prompt.as_ref() {
            Some(profile) => profile
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned(),
            // No profile selected: the 0.6.x single extra prompt still works.
            None => host::setting_string("extraPrompt")
                .unwrap_or_default()
                .trim()
                .to_owned(),
        };
        // 'Default' is the built-in template. A table row with that name must
        // not shadow it, which is how the built-in stays protected.
        let default_template = host::setting_string("defaultTemplate").unwrap_or_default();
        let template = if default_template.trim() == "Default" {
            None
        } else {
            selected("templates", "defaultTemplate").and_then(|profile| {
                profile
                    .get("template")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
        };

        Self {
            provider,
            protocol,
            base_url: profile_text(model.as_ref(), "baseUrl")
                .unwrap_or_default()
                .trim()
                .to_owned(),
            model: profile_text(model.as_ref(), "model")
                .unwrap_or_default()
                .trim()
                .to_owned(),
            temperature: profile_number(model.as_ref(), "temperature")
                .unwrap_or(0.5)
                .clamp(0.0, 2.0),
            send_temperature: profile_bool(model.as_ref(), "sendTemperature").unwrap_or(true),
            max_tokens: profile_number(model.as_ref(), "maxTokens")
                .unwrap_or(64_000.0)
                .clamp(256.0, 128_000.0) as u32,
            secret_slot: secret_slot(model.as_ref()),
            language: MessageLanguage::parse(host::setting_string("language").as_deref()),
            max_diff_chars: host::setting_number("maxDiffChars")
                .unwrap_or(12_000.0)
                .clamp(2_000.0, 200_000.0) as usize,
            commit_types: commit_types(),
            extra_prompt,
            template,
            thinking: thinking::Settings {
                level: thinking::Level::parse(
                    profile_text(model.as_ref(), "thinkingLevel").as_deref(),
                ),
                preset: thinking::Preset::parse(
                    profile_text(model.as_ref(), "thinkingPreset").as_deref(),
                ),
                // A blank column means "use the preset template".
                mapping: profile_text(model.as_ref(), "thinkingMapping")
                    .map(|text| text.trim().to_owned())
                    .filter(|text| !text.is_empty()),
            },
        }
    }

    pub fn effective_base_url(&self) -> String {
        if self.base_url.is_empty() {
            self.provider.base_url().to_owned()
        } else {
            self.base_url.clone()
        }
    }

    pub fn effective_model(&self) -> String {
        if self.model.is_empty() {
            self.provider.default_model().to_owned()
        } else {
            self.model.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn defaults_when_unset() {
        let _host = MockHost::install();
        let config = Config::load();
        assert_eq!(config.provider, Provider::DeepSeek);
        assert_eq!(config.protocol, Protocol::OpenAiChat);
        assert_eq!(config.effective_base_url(), "https://api.deepseek.com/v1");
        assert_eq!(config.effective_model(), "deepseek-chat");
        assert_eq!(
            config.max_tokens, 64_000,
            "room for a reasoning model, within what Anthropic accepts"
        );
        assert_eq!(config.commit_types.len(), 11);
        assert_eq!(config.commit_types[0].title, "feat");
        assert_eq!(config.commit_types[0].description, "A new feature");
        assert_eq!(config.secret_slot, "api-key");
        assert_eq!(config.template, None);
    }

    #[test]
    fn reads_settings() {
        let host = MockHost::install();
        host.set_setting("provider", "\"Anthropic\"");
        host.set_setting("protocol", "\"anthropic\"");
        host.set_setting("model", "\"claude-sonnet-4-5\"");
        host.set_setting("temperature", "0.2");
        host.set_setting("maxTokens", "2048");
        host.set_setting("sendTemperature", "false");
        host.set_setting("commitTypes", "\"feat, fix\"");
        let config = Config::load();
        assert_eq!(config.provider, Provider::Anthropic);
        assert_eq!(config.protocol, Protocol::Anthropic);
        assert_eq!(config.effective_base_url(), "https://api.anthropic.com");
        assert_eq!(config.max_tokens, 2048);
        assert!(
            !config.send_temperature,
            "the model can omit temperature up front"
        );
        assert_eq!(
            config
                .commit_types
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["feat", "fix"]
        );
    }

    #[test]
    fn defaults_to_sending_temperature() {
        let _host = MockHost::install();
        assert!(Config::load().send_temperature);
    }

    #[test]
    fn reads_commit_types_from_the_array_setting() {
        let host = MockHost::install();
        host.set_setting("commitTypes", r#"["feat","docs","ci"]"#);
        let config = Config::load();
        assert_eq!(
            config
                .commit_types
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["feat", "docs", "ci"]
        );
    }

    #[test]
    fn reads_commit_type_descriptions_from_the_table() {
        let host = MockHost::install();
        host.set_setting(
            "commitTypes",
            r#"[{"title":"security","description":"Security hardening"}]"#,
        );
        assert_eq!(
            Config::load().commit_types,
            vec![CommitType {
                title: "security".into(),
                description: "Security hardening".into(),
            }]
        );
    }

    #[test]
    fn the_active_model_profile_overrides_the_single_settings() {
        let host = MockHost::install();
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Work","provider":"Custom","protocol":"openai-responses","baseUrl":"https://llm.example.com/v1","model":"work-model","temperature":0.1,"sendTemperature":false,"maxTokens":2048,"secretSlot":"api-key-3"}]"#,
        );
        host.set_setting("activeModel", "\"Work\"");
        let config = Config::load();
        assert_eq!(config.protocol, Protocol::OpenAiResponses);
        assert_eq!(config.effective_base_url(), "https://llm.example.com/v1");
        assert_eq!(config.effective_model(), "work-model");
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.temperature, 0.1);
        assert!(!config.send_temperature);
        assert_eq!(config.secret_slot, "api-key-3");
    }

    #[test]
    fn an_unknown_key_slot_cannot_leave_the_declared_set() {
        let host = MockHost::install();
        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Work","provider":"Custom","secretSlot":"{{secret:anything}}"}]"#,
        );
        host.set_setting("activeModel", "\"Work\"");
        assert_eq!(Config::load().secret_slot, "api-key");
    }

    #[test]
    fn a_selected_prompt_profile_replaces_the_single_extra_prompt() {
        let host = MockHost::install();
        host.set_setting("extraPrompt", "\"old single prompt\"");
        host.set_setting(
            "promptProfiles",
            r#"[{"name":"Team","text":"Team writing preferences"}]"#,
        );
        host.set_setting("defaultPrompt", "\"Team\"");
        assert_eq!(Config::load().extra_prompt, "Team writing preferences");

        host.set_setting("defaultPrompt", "\"Missing\"");
        assert_eq!(Config::load().extra_prompt, "old single prompt");
    }

    #[test]
    fn a_selected_template_is_used_but_default_stays_built_in() {
        let host = MockHost::install();
        host.set_setting(
            "templates",
            r#"[{"name":"Team","template":"{{subject}}"},{"name":"Default","template":"HACKED"}]"#,
        );
        host.set_setting("defaultTemplate", "\"Team\"");
        assert_eq!(Config::load().template.as_deref(), Some("{{subject}}"));
        host.set_setting("defaultTemplate", "\"Default\"");
        assert_eq!(Config::load().template, None);
    }

    #[test]
    fn thinking_defaults_to_sending_nothing() {
        let _host = MockHost::install();
        assert_eq!(Config::load().thinking, thinking::Settings::default());
    }

    #[test]
    fn the_active_profile_carries_its_thinking_settings() {
        let host = MockHost::install();
        host.set_setting("thinkingLevel", "\"LOW\"");
        host.set_setting("thinkingPreset", "\"nested-effort\"");
        host.set_setting("thinkingMapping", "\"  \"");
        assert_eq!(Config::load().thinking.level, thinking::Level::Low);
        assert_eq!(
            Config::load().thinking.preset,
            thinking::Preset::NestedEffort
        );
        assert_eq!(Config::load().thinking.mapping, None);

        host.set_setting(
            "modelProfiles",
            r#"[{"name":"Reasoner","thinkingLevel":"HIGH","thinkingPreset":"custom","thinkingMapping":"{\"HIGH\":{\"reasoning_effort\":\"high\"}}"}]"#,
        );
        host.set_setting("activeModel", "\"Reasoner\"");
        assert_eq!(Config::load().thinking.level, thinking::Level::High);
        assert_eq!(Config::load().thinking.preset, thinking::Preset::Custom);
        assert!(
            Config::load()
                .thinking
                .mapping
                .unwrap()
                .contains("reasoning_effort")
        );
    }

    #[test]
    fn ui_language_follows_the_ide_locale() {
        let host = MockHost::install();
        host.set_locale("zh-CN");
        assert_eq!(UiLanguage::load(), UiLanguage::Zh);
        host.set_locale("en");
        assert_eq!(UiLanguage::load(), UiLanguage::En);
    }
}
