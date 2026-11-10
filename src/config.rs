//! Plugin settings and the provider presets derived from them.
//!
//! The IDE renders the settings pages from `settings/*.json`; the plugin reads
//! the values through `host::setting*` (project value over global over the
//! schema default).

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

const DEFAULT_TYPES: &str = "feat,fix,docs,style,refactor,perf,test,build,ci,chore,revert";

fn default_commit_types() -> Vec<String> {
    DEFAULT_TYPES.split(',').map(ToOwned::to_owned).collect()
}

/// The settings page stores `commitTypes` as a string array; older settings
/// files used one comma-separated string, so both forms are accepted.
fn commit_types() -> Vec<String> {
    let Some(raw) = host::setting("commitTypes") else {
        return default_commit_types();
    };
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    let types: Vec<String> = match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::trim).map(ToOwned::to_owned))
            .filter(|item| !item.is_empty())
            .collect(),
        Value::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    if types.is_empty() {
        default_commit_types()
    } else {
        types
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub provider: Provider,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub send_temperature: bool,
    pub max_tokens: u32,
    pub language: MessageLanguage,
    pub max_diff_chars: usize,
    pub commit_types: Vec<String>,
    pub extra_prompt: String,
}

impl Config {
    pub fn load() -> Self {
        let protocol_setting = host::setting_string("protocol");
        let protocol = Protocol::parse(protocol_setting.as_deref()).unwrap_or_else(|| {
            let provider = Provider::parse(host::setting_string("provider").as_deref());
            provider.default_protocol()
        });
        let provider = Provider::parse(host::setting_string("provider").as_deref());

        let commit_types = commit_types();

        Self {
            provider,
            protocol,
            base_url: host::setting_string("baseUrl")
                .unwrap_or_default()
                .trim()
                .to_owned(),
            model: host::setting_string("model")
                .unwrap_or_default()
                .trim()
                .to_owned(),
            temperature: host::setting_number("temperature")
                .unwrap_or(0.5)
                .clamp(0.0, 2.0),
            send_temperature: host::setting_bool("sendTemperature").unwrap_or(true),
            max_tokens: host::setting_number("maxTokens")
                .unwrap_or(64_000.0)
                .clamp(256.0, 128_000.0) as u32,
            language: MessageLanguage::parse(host::setting_string("language").as_deref()),
            max_diff_chars: host::setting_number("maxDiffChars")
                .unwrap_or(12_000.0)
                .clamp(2_000.0, 200_000.0) as usize,
            commit_types,
            extra_prompt: host::setting_string("extraPrompt")
                .unwrap_or_default()
                .trim()
                .to_owned(),
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
        assert_eq!(config.commit_types, vec!["feat", "fix"]);
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
        assert_eq!(config.commit_types, vec!["feat", "docs", "ci"]);
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
