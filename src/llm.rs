//! LLM clients for the three supported protocols.
//!
//! All requests go through `host::fetch`, so the IDE enforces the `net:<host>`
//! capabilities declared in `plugin.toml`. The API key is never read by the
//! plugin: `{{secret:api-key}}` is filled in by the host from the system
//! keychain.

use crate::config::{Config, Protocol};
use openidea_plugin::{Error, HttpRequest, host};
use serde_json::{Value, json};

const REQUEST_TIMEOUT_MS: u32 = 55_000;

pub fn endpoint(config: &Config) -> Result<String, Error> {
    let base = config.effective_base_url();
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(Error::Failed(
            "Base URL is empty. Pick a provider preset or fill in Base URL in Settings › Plugins › Git Commit Message Helper."
                .into(),
        ));
    }
    let suffix = match config.protocol {
        Protocol::OpenAiChat => "/chat/completions",
        Protocol::OpenAiResponses => "/responses",
        Protocol::Anthropic => "/messages",
    };
    if base.ends_with(suffix) {
        return Ok(base.to_owned());
    }
    if has_url_path(base) {
        return Ok(format!("{base}{suffix}"));
    }
    Ok(format!("{base}/v1{suffix}"))
}

fn has_url_path(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.contains('/')
}

fn request_body(config: &Config, system: &str, user: &str) -> String {
    request_body_with_limit(config, system, user, config.max_tokens)
}

fn request_body_with_limit(config: &Config, system: &str, user: &str, max_tokens: u32) -> String {
    let model = config.effective_model();
    let value = match config.protocol {
        Protocol::OpenAiChat => json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "temperature": config.temperature,
            "max_tokens": max_tokens
        }),
        Protocol::OpenAiResponses => json!({
            "model": model,
            "instructions": system,
            "input": user,
            "temperature": config.temperature,
            "max_output_tokens": max_tokens
        }),
        Protocol::Anthropic => json!({
            "model": model,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            "temperature": config.temperature,
            "max_tokens": max_tokens
        }),
    };
    value.to_string()
}

fn build_request(config: &Config, body: String) -> Result<HttpRequest, Error> {
    let mut request = HttpRequest::post(&endpoint(config)?, body)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json");
    match config.protocol {
        Protocol::Anthropic => {
            request = request
                .header("x-api-key", "{{secret:api-key}}")
                .header("anthropic-version", "2023-06-01");
        }
        Protocol::OpenAiChat | Protocol::OpenAiResponses => {
            request = request.header("Authorization", "Bearer {{secret:api-key}}");
        }
    }
    request.timeout_ms = Some(REQUEST_TIMEOUT_MS);
    Ok(request)
}

/// Sends one generation request and returns the assistant's text.
pub fn generate(config: &Config, system: &str, user: &str) -> Result<String, Error> {
    if !host::secret_is_set("api-key") {
        return Err(Error::Failed(
            "No API key. Open Settings › Plugins › Git Commit Message Helper and enter the LLM API key.".into(),
        ));
    }

    let body = request_body(config, system, user);
    let request = build_request(config, body)?;
    let response = host::fetch(&request)?;
    let text = response.text();
    if !(200..300).contains(&response.status) {
        return Err(Error::Failed(format!(
            "LLM request failed (HTTP {}): {}",
            response.status,
            error_message(&text)
        )));
    }
    extract_content(config.protocol, &text)
}

/// Sends a minimal request to check the configured endpoint, model and key.
pub fn test(config: &Config) -> Result<String, Error> {
    if !host::secret_is_set("api-key") {
        return Err(Error::Failed(
            "No API key. Open Settings › Plugins › Git Commit Message Helper and enter the LLM API key.".into(),
        ));
    }
    if config.effective_model().is_empty() {
        return Err(Error::Failed(
            "Model is empty. Pick a provider preset or fill in Model in Settings › Plugins › Git Commit Message Helper."
                .into(),
        ));
    }
    let body = request_body_with_limit(config, "You are a connection test.", "ping", 16);
    let request = build_request(config, body)?;
    let response = host::fetch(&request)?;
    let text = response.text();
    if !(200..300).contains(&response.status) {
        return Err(Error::Failed(format!(
            "LLM test failed (HTTP {}): {}",
            response.status,
            error_message(&text)
        )));
    }
    extract_content(config.protocol, &text)?;
    Ok(format!(
        "HTTP {} · {}",
        response.status,
        config.effective_model()
    ))
}

fn error_message(body: &str) -> String {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(body.trim()) {
        if let Some(message) = map
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return clip(message, 600);
        }
        if let Some(message) = map.get("message").and_then(Value::as_str) {
            return clip(message, 600);
        }
    }
    clip(body.trim(), 600)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

/// The Responses API returns `output: [{type:"reasoning",...},{content:[{type:"output_text",text}...]}]`
/// (the convenience `output_text` field is not always present).
fn responses_output_text(value: &Value) -> Option<String> {
    let items = value.get("output")?.as_array()?;
    let mut texts = Vec::new();
    for item in items {
        if let Some(text) = item.get("text").and_then(value_text)
            && !text.is_empty()
        {
            texts.push(text);
            continue;
        }
        if let Some(text) = item.get("content").and_then(value_text)
            && !text.is_empty()
        {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(value_text)
                .filter(|t| !t.is_empty())
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        Value::Object(map) => map.get("text").and_then(value_text),
        _ => None,
    }
}

pub fn extract_content(protocol: Protocol, body: &str) -> Result<String, Error> {
    let value: Value = serde_json::from_str(body.trim()).map_err(|e| {
        Error::Failed(format!(
            "LLM returned non-JSON response: {e}: {}",
            clip(body, 400)
        ))
    })?;
    let text = match protocol {
        Protocol::OpenAiChat => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(value_text),
        Protocol::OpenAiResponses => value
            .get("output_text")
            .and_then(value_text)
            .or_else(|| responses_output_text(&value)),
        Protocol::Anthropic => value.get("content").and_then(value_text),
    };
    match text {
        Some(text) if !text.trim().is_empty() => Ok(text),
        _ => Err(Error::Failed(format!(
            "LLM response did not contain any text: {}",
            clip(body, 400)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MessageLanguage, Provider};

    fn config(provider: Provider, protocol: Protocol, base_url: &str) -> Config {
        Config {
            provider,
            protocol,
            base_url: base_url.into(),
            model: String::new(),
            temperature: 0.5,
            max_tokens: 4096,
            language: MessageLanguage::Auto,
            max_diff_chars: 12_000,
            commit_types: vec!["feat".into()],
            extra_prompt: String::new(),
        }
    }

    #[test]
    fn joins_common_endpoints() {
        assert_eq!(
            endpoint(&config(Provider::OpenAi, Protocol::OpenAiChat, "")).unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(&config(Provider::DeepSeek, Protocol::OpenAiChat, "")).unwrap(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(&config(Provider::Anthropic, Protocol::Anthropic, "")).unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            endpoint(&config(Provider::Glm, Protocol::OpenAiChat, "")).unwrap(),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            endpoint(&config(Provider::Magpie, Protocol::OpenAiChat, "")).unwrap(),
            "http://127.0.0.1:3425/v1/chat/completions"
        );
        assert_eq!(
            endpoint(&config(
                Provider::Custom,
                Protocol::OpenAiChat,
                "https://llm.example.com/v1"
            ))
            .unwrap(),
            "https://llm.example.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(&config(
                Provider::Custom,
                Protocol::OpenAiResponses,
                "https://llm.example.com/v1"
            ))
            .unwrap(),
            "https://llm.example.com/v1/responses"
        );
    }

    #[test]
    fn extracts_each_protocol() {
        let chat = r#"{"choices":[{"message":{"content":"{\"type\":\"feat\"}"}}]}"#;
        assert_eq!(
            extract_content(Protocol::OpenAiChat, chat).unwrap(),
            "{\"type\":\"feat\"}"
        );

        let responses = r#"{"output_text":"hello"}"#;
        assert_eq!(
            extract_content(Protocol::OpenAiResponses, responses).unwrap(),
            "hello"
        );

        let anthropic =
            r#"{"content":[{"type":"text","text":"hello"},{"type":"text","text":"world"}]}"#;
        assert_eq!(
            extract_content(Protocol::Anthropic, anthropic).unwrap(),
            "hello\nworld"
        );
    }

    #[test]
    fn reports_provider_error_message() {
        assert_eq!(
            error_message(r#"{"error":{"message":"bad key"}}"#),
            "bad key"
        );
    }
}
