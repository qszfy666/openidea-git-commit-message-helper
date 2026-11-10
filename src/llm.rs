//! LLM clients for the three supported protocols.
//!
//! All requests go through `host::fetch`, so the IDE enforces the `net:<host>`
//! capabilities declared in `plugin.toml`. The API key is never read by the
//! plugin: `{{secret:api-key}}` is filled in by the host from the system
//! keychain.
//!
//! A command runs inside a 60 s background task. The first attempt and the
//! compatibility retries below share one budget, so a model that refuses a
//! parameter cannot push the command past that window.

use crate::config::{Config, Protocol};
use crate::thinking;
use openidea_plugin::{Error, HttpRequest, host};
use serde_json::{Value, json};
use std::time::Instant;

/// Time the attempts of one command share, inside the host's 60 s task budget.
const BUDGET_MS: u64 = 50_000;
/// The longest a single attempt may take, as the host allows.
const ATTEMPT_MS: u64 = 55_000;
/// A retry that would get less time than this is not worth starting.
const MIN_ATTEMPT_MS: u64 = 8_000;
/// The first request plus one retry per parameter that can be adjusted.
const MAX_ATTEMPTS: u32 = 3;
/// The connection test only needs a valid answer, not one worth reading.
const TEST_TOKENS: u32 = 16;

fn ensure_ready(config: &Config) -> Result<(), Error> {
    if !host::secret_is_set(&config.secret_slot) {
        return Err(Error::Failed(format!(
            "No API key in slot '{}'. In Settings › Plugins › Installed, select Git Commit Message Helper and fill in the matching LLM API key under Secrets.",
            config.secret_slot
        )));
    }
    Ok(())
}

fn ensure_model(config: &Config) -> Result<(), Error> {
    if config.effective_model().is_empty() {
        return Err(Error::Failed(
            "Model is empty. Pick a provider preset or fill in Model in Settings › Plugins › Git Commit Message Helper."
                .into(),
        ));
    }
    Ok(())
}

fn protocol_suffix(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::OpenAiChat => "/chat/completions",
        Protocol::OpenAiResponses => "/responses",
        Protocol::Anthropic => "/messages",
    }
}

fn endpoint_with(config: &Config, suffix: &str) -> Result<String, Error> {
    join_endpoint(&config.effective_base_url(), suffix)
}

/// Joins a protocol path onto a Base URL: a path-less URL gets `/v1`, a URL
/// with a path gets the suffix appended, and one already ending in the suffix
/// is used as-is.
fn join_endpoint(base: &str, suffix: &str) -> Result<String, Error> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(Error::Failed(
            "Base URL is empty. Pick a provider preset or fill in Base URL in Settings › Plugins › Git Commit Message Helper."
                .into(),
        ));
    }
    if base.ends_with(suffix) {
        return Ok(base.to_owned());
    }
    if has_url_path(base) {
        return Ok(format!("{base}{suffix}"));
    }
    Ok(format!("{base}/v1{suffix}"))
}

/// The URL the plugin posts a generation request to.
pub fn endpoint(config: &Config) -> Result<String, Error> {
    endpoint_with(config, protocol_suffix(config.protocol))
}

/// The model-list URL of the same Base URL. A Base URL that names a complete
/// generation endpoint (which `endpoint` accepts) is reduced to its parent
/// before `/models` is appended.
fn models_endpoint(config: &Config) -> Result<String, Error> {
    let base = config.effective_base_url();
    let base = base.trim().trim_end_matches('/');
    let base = ["/chat/completions", "/responses", "/messages"]
        .iter()
        .find_map(|suffix| base.strip_suffix(*suffix))
        .unwrap_or(base);
    join_endpoint(base, "/models")
}

fn has_url_path(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.contains('/')
}

/// Which request field carries the response budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenParameter {
    /// `max_tokens`: Chat Completions and Anthropic Messages.
    Tokens,
    /// `max_completion_tokens`: Chat Completions models that refuse `max_tokens`.
    CompletionTokens,
    /// `max_output_tokens`: the Responses API.
    OutputTokens,
}

impl TokenParameter {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tokens => "max_tokens",
            Self::CompletionTokens => "max_completion_tokens",
            Self::OutputTokens => "max_output_tokens",
        }
    }
}

/// The parameters one attempt sends, after any compatibility adjustment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RequestShape {
    pub token: TokenParameter,
    /// `None` when the setting turns temperature off or an earlier attempt lost it.
    pub temperature: Option<f64>,
}

impl RequestShape {
    fn for_config(config: &Config) -> Self {
        let token = match config.protocol {
            Protocol::OpenAiResponses => TokenParameter::OutputTokens,
            Protocol::OpenAiChat | Protocol::Anthropic => TokenParameter::Tokens,
        };
        Self {
            token,
            temperature: config.send_temperature.then_some(config.temperature),
        }
    }

    /// What to send instead after a server refused the request, or `None` when
    /// it refused something the plugin cannot drop. Only a parameter the plugin
    /// chose itself is adjusted.
    fn after_rejection(
        self,
        protocol: Protocol,
        status: u16,
        message: &str,
    ) -> Option<(Self, &'static str)> {
        if !(400..500).contains(&status) {
            return None;
        }
        let refusal = message.to_ascii_lowercase();
        if protocol == Protocol::OpenAiChat
            && self.token == TokenParameter::Tokens
            && refusal.contains("max_completion_tokens")
        {
            return Some((
                Self {
                    token: TokenParameter::CompletionTokens,
                    ..self
                },
                "token parameter",
            ));
        }
        if self.temperature.is_some() && refusal.contains("temperature") {
            return Some((
                Self {
                    temperature: None,
                    ..self
                },
                "temperature",
            ));
        }
        None
    }
}

/// What was actually sent, for the notification and the connection test.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostics {
    pub endpoint: String,
    pub token: TokenParameter,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
    pub attempts: u32,
    /// Parameters a refusal made the plugin drop, in order.
    pub adjusted: Vec<&'static str>,
    /// The thinking fragment sent, or why it was omitted. `None` when the
    /// profile sends no thinking parameters at all.
    pub thinking: Option<String>,
}

impl Diagnostics {
    /// Two lines: the URL that was called, then the parameters it carried.
    pub fn summary(&self) -> String {
        let temperature = match self.temperature {
            Some(value) => format!("temperature {value}"),
            None => "temperature omitted".to_owned(),
        };
        let attempts = if self.attempts == 1 {
            "1 attempt".to_owned()
        } else {
            format!("{} attempts", self.attempts)
        };
        let mut line = format!(
            "{} {} · {temperature} · {attempts}",
            self.token.name(),
            self.max_tokens
        );
        if !self.adjusted.is_empty() {
            line.push_str(" · adjusted: ");
            line.push_str(&self.adjusted.join(", "));
        }
        if let Some(thinking) = &self.thinking {
            line.push_str(&format!(" · thinking: {thinking}"));
        }
        format!("{}\n{line}", self.endpoint)
    }
}

/// An answer, with the request that produced it.
pub struct Answer {
    /// The assistant's text (the raw body for the connection test).
    pub text: String,
    pub status: u16,
    pub diagnostics: Diagnostics,
}

/// The host fills `{{secret:<slot>}}` in from the keychain on the way out.
/// Only declared slots can be named, and the plugin never reads the value.
fn secret_placeholder(config: &Config) -> String {
    format!("{{{{secret:{}}}}}", config.secret_slot)
}

fn authorize(config: &Config, request: HttpRequest) -> HttpRequest {
    let secret = secret_placeholder(config);
    let bearer = format!("Bearer {secret}");
    match config.protocol {
        Protocol::Anthropic => request
            .header("x-api-key", secret.as_str())
            .header("anthropic-version", "2023-06-01"),
        Protocol::OpenAiChat | Protocol::OpenAiResponses => {
            request.header("Authorization", bearer.as_str())
        }
    }
}

fn request_body(
    config: &Config,
    shape: RequestShape,
    system: &str,
    user: &str,
    max_tokens: u32,
    thinking_fragment: Option<&Value>,
) -> String {
    let model = config.effective_model();
    let mut value = match config.protocol {
        Protocol::OpenAiChat => json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
        }),
        Protocol::OpenAiResponses => json!({
            "model": model,
            "instructions": system,
            "input": user,
        }),
        Protocol::Anthropic => json!({
            "model": model,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
        }),
    };
    if let Value::Object(map) = &mut value {
        map.insert(shape.token.name().to_owned(), json!(max_tokens));
        if let Some(temperature) = shape.temperature {
            map.insert("temperature".to_owned(), json!(temperature));
        }
        // A validated fragment only adds fields; validate() keeps the
        // plugin-owned request fields out of it.
        if let Some(Value::Object(fragment)) = thinking_fragment {
            for (key, field) in fragment {
                map.insert(key.clone(), field.clone());
            }
        }
    }
    value.to_string()
}

fn build_request(config: &Config, url: &str, body: String, timeout_ms: u32) -> HttpRequest {
    let mut request = authorize(
        config,
        HttpRequest::post(url, body)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json"),
    );
    request.timeout_ms = Some(timeout_ms);
    request
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

/// Posts one prompt, retrying with adjusted parameters when the server refuses
/// one the plugin chose. The attempts share [`BUDGET_MS`].
fn send(config: &Config, system: &str, user: &str, max_tokens: u32) -> Result<Answer, Error> {
    let url = endpoint(config)?;
    let started = Instant::now();
    let mut shape = RequestShape::for_config(config);
    let resolution = thinking::resolve(&config.thinking, config.protocol, max_tokens);
    let mut adjusted: Vec<&'static str> = Vec::new();
    let mut attempts = 0;
    let mut last_failure = String::new();

    loop {
        let left = BUDGET_MS.saturating_sub(started.elapsed().as_millis() as u64);
        if attempts > 0 && left < MIN_ATTEMPT_MS {
            return Err(Error::Failed(format!(
                "{last_failure} (stopped after {attempts} attempts: the task time budget is spent)"
            )));
        }
        attempts += 1;
        let body = request_body(
            config,
            shape,
            system,
            user,
            max_tokens,
            resolution.fragment.as_ref(),
        );
        let timeout = left.clamp(MIN_ATTEMPT_MS, ATTEMPT_MS) as u32;
        let request = build_request(config, &url, body, timeout);
        let response = host::fetch(&request)?;
        let text = response.text();
        if (200..300).contains(&response.status) {
            return Ok(Answer {
                text,
                status: response.status,
                diagnostics: Diagnostics {
                    endpoint: url,
                    token: shape.token,
                    max_tokens,
                    temperature: shape.temperature,
                    attempts,
                    adjusted,
                    thinking: resolution.note.clone(),
                },
            });
        }

        let failure = format!(
            "LLM request failed (HTTP {}): {}",
            response.status,
            error_message(&text)
        );
        match shape.after_rejection(config.protocol, response.status, &text) {
            Some((next, note)) if attempts < MAX_ATTEMPTS => {
                shape = next;
                adjusted.push(note);
                last_failure = failure;
            }
            _ => return Err(Error::Failed(failure)),
        }
    }
}

/// Sends one generation request and returns the assistant's text.
pub fn generate(config: &Config, system: &str, user: &str) -> Result<Answer, Error> {
    ensure_ready(config)?;
    let answer = send(config, system, user, config.max_tokens)?;
    let text = extract_content(config.protocol, &answer.text)?;
    Ok(Answer { text, ..answer })
}

/// Sends a minimal request to check the configured endpoint, model and key.
pub fn test(config: &Config) -> Result<Answer, Error> {
    ensure_ready(config)?;
    ensure_model(config)?;
    let thinking = thinking::resolve(&config.thinking, config.protocol, config.max_tokens);
    let has_budget = thinking
        .fragment
        .as_ref()
        .and_then(|fragment| fragment.get("thinking"))
        .and_then(|fragment| fragment.get("budget_tokens"))
        .is_some();
    // A budgeted thinking request must keep the profile's response limit so
    // the probe tests the configured fragment with a valid token budget.
    let max_tokens = if has_budget {
        config.max_tokens
    } else {
        TEST_TOKENS
    };
    let answer = send(config, "You are a connection test.", "ping", max_tokens)?;
    // A 2xx proves the endpoint, key and model; the body only has to be JSON.
    // The content is not checked: a reasoning model given the 16-token budget
    // answers `content: null` with `finish_reason: "length"`, which is a
    // working connection, not a failure.
    serde_json::from_str::<Value>(answer.text.trim()).map_err(|e| {
        Error::Failed(format!(
            "LLM test returned non-JSON response: {e}: {}",
            clip(&answer.text, 400)
        ))
    })?;
    Ok(answer)
}

/// The model ids the endpoint lists, sorted, for the picker in Settings.
pub fn models(config: &Config) -> Result<Vec<String>, Error> {
    ensure_ready(config)?;
    let url = models_endpoint(config)?;
    let mut request = authorize(
        config,
        HttpRequest::get(&url).header("Accept", "application/json"),
    );
    request.timeout_ms = Some(BUDGET_MS as u32);
    let response = host::fetch(&request)?;
    let text = response.text();
    if !(200..300).contains(&response.status) {
        return Err(Error::Failed(format!(
            "Model list request failed (HTTP {}): {}",
            response.status,
            error_message(&text)
        )));
    }
    let value: Value = serde_json::from_str(text.trim()).map_err(|e| {
        Error::Failed(format!(
            "The model list is not JSON: {e}: {}",
            clip(&text, 400)
        ))
    })?;
    let mut ids: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(Error::Failed(
            "The endpoint listed no models. Fill in Model by hand in Settings.".into(),
        ));
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
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
    use crate::config::{CommitType, MessageLanguage, Provider};
    use openidea_plugin::HttpResponse;
    use openidea_plugin::mock::MockHost;

    fn config(provider: Provider, protocol: Protocol, base_url: &str) -> Config {
        Config {
            provider,
            protocol,
            base_url: base_url.into(),
            model: String::new(),
            temperature: 0.5,
            send_temperature: true,
            max_tokens: 4096,
            language: MessageLanguage::Auto,
            max_diff_chars: 12_000,
            commit_types: vec![CommitType {
                title: "feat".into(),
                description: String::new(),
            }],
            extra_prompt: String::new(),
            secret_slot: "api-key".into(),
            template: None,
            thinking: thinking::Settings::default(),
        }
    }

    fn body_of(config: &Config, shape: RequestShape) -> Value {
        serde_json::from_str(&request_body(config, shape, "system", "user", 4096, None)).unwrap()
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
    fn lists_models_next_to_the_protocol_path() {
        assert_eq!(
            models_endpoint(&config(Provider::OpenAi, Protocol::OpenAiChat, "")).unwrap(),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            models_endpoint(&config(Provider::Anthropic, Protocol::Anthropic, "")).unwrap(),
            "https://api.anthropic.com/v1/models"
        );
        assert_eq!(
            models_endpoint(&config(
                Provider::Magpie,
                Protocol::OpenAiChat,
                "http://127.0.0.1:3425"
            ))
            .unwrap(),
            "http://127.0.0.1:3425/v1/models"
        );
        // The Base URL may name the generation endpoint itself, as `endpoint`
        // accepts; the model list has to go next to it, not under it.
        assert_eq!(
            models_endpoint(&config(
                Provider::Custom,
                Protocol::OpenAiChat,
                "https://llm.example.com/v1/chat/completions"
            ))
            .unwrap(),
            "https://llm.example.com/v1/models"
        );
    }

    #[test]
    fn a_profile_carries_its_own_secret_slot() {
        let mut config = config(Provider::OpenAi, Protocol::OpenAiChat, "");
        config.secret_slot = "{{SECRET_hhyvxk2c}}".into();
        let chat = authorize(&config, HttpRequest::post("https://example.com", "{}"));
        assert!(chat.headers.iter().any(|(name, value)| {
            name == "Authorization" && value == "Bearer {{secret:{{SECRET_hhyvxk2c}}}}"
        }));

        config.protocol = Protocol::Anthropic;
        let anthropic = authorize(&config, HttpRequest::post("https://example.com", "{}"));
        assert!(anthropic.headers.iter().any(|(name, value)| {
            name == "x-api-key" && value == "{{secret:{{SECRET_hhyvxk2c}}}}"
        }));
    }

    #[test]
    fn sends_the_token_parameter_of_the_protocol() {
        let chat = body_of(
            &config(Provider::OpenAi, Protocol::OpenAiChat, ""),
            RequestShape {
                token: TokenParameter::Tokens,
                temperature: Some(0.5),
            },
        );
        assert_eq!(chat["max_tokens"], 4096);
        assert_eq!(chat["temperature"], 0.5);

        let responses = body_of(
            &config(Provider::OpenAi, Protocol::OpenAiResponses, ""),
            RequestShape {
                token: TokenParameter::OutputTokens,
                temperature: None,
            },
        );
        assert_eq!(responses["max_output_tokens"], 4096);
        assert!(
            responses.get("temperature").is_none(),
            "an omitted temperature is left out: {responses}"
        );
    }

    #[test]
    fn a_refused_max_tokens_becomes_max_completion_tokens() {
        let shape = RequestShape {
            token: TokenParameter::Tokens,
            temperature: Some(0.5),
        };
        let refused = "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.";
        let (next, note) = shape
            .after_rejection(Protocol::OpenAiChat, 400, refused)
            .expect("the parameter is adjustable");
        assert_eq!(note, "token parameter");
        assert_eq!(next.token, TokenParameter::CompletionTokens);
        assert_eq!(next.temperature, Some(0.5), "temperature stays");
    }

    #[test]
    fn a_refused_temperature_is_dropped() {
        let shape = RequestShape {
            token: TokenParameter::Tokens,
            temperature: Some(0.5),
        };
        let (next, note) = shape
            .after_rejection(
                Protocol::OpenAiChat,
                400,
                "Unsupported value: 'temperature' does not support 0.5 with this model.",
            )
            .expect("temperature is adjustable");
        assert_eq!(note, "temperature");
        assert_eq!(next.temperature, None);
        assert_eq!(
            next.token,
            TokenParameter::Tokens,
            "the token parameter stays"
        );
        assert_eq!(
            next.after_rejection(Protocol::OpenAiChat, 400, "temperature is unsupported"),
            None,
            "a parameter that is already omitted is not adjusted twice"
        );
    }

    #[test]
    fn other_failures_and_protocols_are_not_adjusted() {
        let shape = RequestShape {
            token: TokenParameter::Tokens,
            temperature: Some(0.5),
        };
        assert_eq!(
            shape.after_rejection(Protocol::OpenAiChat, 401, "invalid api key"),
            None
        );
        assert_eq!(
            shape.after_rejection(Protocol::OpenAiChat, 500, "max_completion_tokens"),
            None,
            "a server error is not a parameter refusal"
        );
        assert_eq!(
            shape.after_rejection(Protocol::Anthropic, 400, "use max_completion_tokens"),
            None,
            "Anthropic takes max_tokens, so the field is not renamed"
        );
    }

    #[test]
    fn diagnostics_name_what_was_sent() {
        let diagnostics = Diagnostics {
            endpoint: "https://api.example.com/v1/chat/completions".into(),
            token: TokenParameter::CompletionTokens,
            max_tokens: 64_000,
            temperature: None,
            attempts: 2,
            adjusted: vec!["token parameter"],
            thinking: None,
        };
        assert_eq!(
            diagnostics.summary(),
            "https://api.example.com/v1/chat/completions\n\
max_completion_tokens 64000 · temperature omitted · 2 attempts · adjusted: token parameter"
        );
    }

    #[test]
    fn diagnostics_show_the_thinking_fragment() {
        let mut diagnostics = Diagnostics {
            endpoint: "https://api.example.com/v1/chat/completions".into(),
            token: TokenParameter::Tokens,
            max_tokens: 4096,
            temperature: Some(0.5),
            attempts: 1,
            adjusted: Vec::new(),
            thinking: None,
        };
        diagnostics.thinking = Some("HIGH → {\"reasoning_effort\":\"high\"}".into());
        assert!(
            diagnostics.summary().contains("thinking: HIGH"),
            "{}",
            diagnostics.summary()
        );
    }

    #[test]
    fn a_validated_fragment_is_merged_into_the_request_body() {
        let fragment = json!({"reasoning_effort": "high"});
        let body: Value = serde_json::from_str(&request_body(
            &config(Provider::OpenAi, Protocol::OpenAiChat, ""),
            RequestShape {
                token: TokenParameter::Tokens,
                temperature: Some(0.5),
            },
            "system",
            "user",
            4096,
            Some(&fragment),
        ))
        .unwrap();
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["max_tokens"], 4096);
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

    #[test]
    fn model_list_reads_the_ids_and_sorts_them() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|request| {
            assert_eq!(request.method, "GET");
            assert!(request.url.ends_with("/v1/models"), "{}", request.url);
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name == "Authorization" && value.starts_with("Bearer ")),
                "the key goes out in the header: {:?}",
                request.headers
            );
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"data":[{"id":"deepseek-reasoner"},{"id":"deepseek-chat"},{"id":"deepseek-chat"}]}"#.to_vec(),
            })
        });

        let models = models(&config(Provider::DeepSeek, Protocol::OpenAiChat, "")).unwrap();
        assert_eq!(models, vec!["deepseek-chat", "deepseek-reasoner"]);
    }

    #[test]
    fn model_list_reports_a_refused_request() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 401,
                headers: Vec::new(),
                body: br#"{"error":{"message":"bad key"}}"#.to_vec(),
            })
        });

        let error = models(&config(Provider::DeepSeek, Protocol::OpenAiChat, "")).unwrap_err();
        assert!(format!("{error}").contains("bad key"), "{error:?}");
    }
}
