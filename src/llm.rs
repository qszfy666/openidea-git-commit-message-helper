//! LLM clients for the three supported protocols.
//!
//! All requests go through `host::fetch`, so the IDE enforces the `net:<host>`
//! capabilities declared in `plugin.toml`; under `net:ask` it asks the user
//! about any other HTTPS host. The API key is never read by the
//! plugin: `{{secret:api-key}}` is filled in by the host from the system
//! keychain.
//!
//! A command runs inside a 60 s background task. The first attempt and the
//! compatibility retries below share one budget, so a model that refuses a
//! parameter cannot push the command past that window.

use crate::config::{self, Config, Protocol, UiLanguage};
use crate::thinking;
use crate::ui;
use openidea_plugin::{Error, HttpRequest, host};
use serde::{Deserialize, Serialize};
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

/// Error text in the IDE's UI language, read when the error is made.
fn strings() -> &'static ui::Strings {
    ui::strings(UiLanguage::load())
}

fn ensure_ready() -> Result<(), Error> {
    if !host::secret_is_set(config::SECRET) {
        return Err(Error::Failed(
            strings().llm_no_key.replace("{slot}", config::SECRET),
        ));
    }
    Ok(())
}

fn ensure_model(config: &Config) -> Result<(), Error> {
    if config.effective_model().is_empty() {
        return Err(Error::Failed(strings().llm_no_model.into()));
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
        return Err(Error::Failed(strings().llm_no_base_url.into()));
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
    /// The field a cached refusal names.
    fn named(name: &str) -> Option<Self> {
        [Self::Tokens, Self::CompletionTokens, Self::OutputTokens]
            .into_iter()
            .find(|token| token.name() == name)
    }

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
    /// Parameters a refusal of an earlier request had already dropped, applied
    /// from the compatibility cache without asking the server again.
    pub cached: Vec<&'static str>,
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
        if !self.cached.is_empty() {
            line.push_str(" · cached: ");
            line.push_str(&self.cached.join(", "));
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

/// The host fills `{{secret:api-key}}` in from the keychain on the way out;
/// the plugin never reads the value.
fn authorize(config: &Config, request: HttpRequest) -> HttpRequest {
    let secret = format!("{{{{secret:{}}}}}", config::SECRET);
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

/// What a server refused for one endpoint, model, protocol and thinking
/// mapping (checklist 39). Stored with `host::storage`, so it survives the idle
/// reclaim that drops the Wasm instance.
#[derive(Default, Serialize, Deserialize)]
struct Cached {
    /// The token field the server accepted, when it is not the default one.
    token: Option<String>,
    /// The server refused `temperature` for this model.
    temperature_off: bool,
}

fn protocol_key(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::OpenAiChat => "openai-chat",
        Protocol::OpenAiResponses => "openai-responses",
        Protocol::Anthropic => "anthropic",
    }
}

fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A temperature-value refusal applies only to that configured value.
fn cache_key(config: &Config) -> String {
    let identity = format!(
        "{}|{}|{}|{}|{:?}",
        config.effective_base_url(),
        config.effective_model(),
        protocol_key(config.protocol),
        thinking::mapping_text(&config.thinking, config.protocol),
        config.send_temperature.then_some(config.temperature)
    );
    format!("compat:{:016x}", fnv1a(&identity))
}

/// The shape to start with, from what an earlier request learned.
fn cached_shape(config: &Config, default: RequestShape) -> (RequestShape, Vec<&'static str>) {
    let Some(cached) = host::storage::get(&cache_key(config))
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<Cached>(&text).ok())
    else {
        return (default, Vec::new());
    };
    let mut shape = default;
    let mut notes: Vec<&'static str> = Vec::new();
    if let Some(token) = cached.token.as_deref().and_then(TokenParameter::named)
        && token != default.token
    {
        shape.token = token;
        notes.push("token parameter");
    }
    if cached.temperature_off && default.temperature.is_some() {
        shape.temperature = None;
        notes.push("temperature");
    }
    (shape, notes)
}

/// Remembers what this request had to drop, so the next one starts there.
fn remember_shape(config: &Config, default: RequestShape, shape: RequestShape) {
    let changed = shape.token != default.token
        || (default.temperature.is_some() && shape.temperature.is_none());
    if !changed {
        return;
    }
    let cached = Cached {
        token: (shape.token != default.token).then(|| shape.token.name().to_owned()),
        temperature_off: default.temperature.is_some() && shape.temperature.is_none(),
    };
    if let Ok(text) = serde_json::to_string(&cached) {
        let _ = host::storage::set(&cache_key(config), &text);
    }
}

/// Posts one prompt, retrying with adjusted parameters when the server refuses
/// one the plugin chose. The attempts share [`BUDGET_MS`].
fn send(config: &Config, system: &str, user: &str, max_tokens: u32) -> Result<Answer, Error> {
    let url = endpoint(config)?;
    let started = Instant::now();
    let default_shape = RequestShape::for_config(config);
    let (mut shape, cached) = cached_shape(config, default_shape);
    let resolution = thinking::resolve(&config.thinking, config.protocol, max_tokens);
    let mut adjusted: Vec<&'static str> = Vec::new();
    let mut attempts = 0;
    let mut last_failure = String::new();

    loop {
        let left = BUDGET_MS.saturating_sub(started.elapsed().as_millis() as u64);
        if attempts > 0 && left < MIN_ATTEMPT_MS {
            return Err(Error::Failed(
                strings()
                    .llm_budget_spent
                    .replace("{attempts}", &attempts.to_string())
                    .replace("{failure}", &last_failure),
            ));
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
            remember_shape(config, default_shape, shape);
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
                    cached,
                    thinking: resolution.note.clone(),
                },
            });
        }

        let failure = strings()
            .llm_request_failed
            .replace("{status}", &response.status.to_string())
            .replace("{detail}", &error_message(&text));
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
    ensure_ready()?;
    let answer = send(config, system, user, config.max_tokens)?;
    let text = extract_content(config.protocol, &answer.text)?;
    Ok(Answer { text, ..answer })
}

/// Sends a minimal request to check the configured endpoint, model and key.
pub fn test(config: &Config) -> Result<Answer, Error> {
    ensure_ready()?;
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
        Error::Failed(
            strings()
                .llm_test_not_json
                .replace("{error}", &e.to_string())
                .replace("{body}", &clip(&answer.text, 400)),
        )
    })?;
    Ok(answer)
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
        Error::Failed(
            strings()
                .llm_not_json
                .replace("{error}", &e.to_string())
                .replace("{body}", &clip(body, 400)),
        )
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
        _ => Err(Error::Failed(
            strings().llm_no_text.replace("{body}", &clip(body, 400)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommitType, Provider};
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
            language: "English".into(),
            max_diff_chars: 12_000,
            commit_types: vec![CommitType {
                title: "feat".into(),
                description: String::new(),
            }],
            extra_prompt: String::new(),
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
    fn the_api_key_goes_in_the_header_of_the_protocol() {
        let mut config = config(Provider::OpenAi, Protocol::OpenAiChat, "");
        let chat = authorize(&config, HttpRequest::post("https://example.com", "{}"));
        assert!(chat.headers.iter().any(|(name, value)| {
            name == "Authorization" && value == "Bearer {{secret:api-key}}"
        }));

        config.protocol = Protocol::Anthropic;
        let anthropic = authorize(&config, HttpRequest::post("https://example.com", "{}"));
        assert!(
            anthropic
                .headers
                .iter()
                .any(|(name, value)| { name == "x-api-key" && value == "{{secret:api-key}}" })
        );
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
            cached: Vec::new(),
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
            cached: Vec::new(),
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

    fn refusing_temperature_host() -> MockHost {
        let host =
            MockHost::install().with_capabilities(&["secrets", "storage", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test-key");
        host.on_fetch(|request| {
            let body = String::from_utf8_lossy(request.body.as_deref().unwrap_or_default());
            if body.contains("\"temperature\"") {
                return Ok(HttpResponse {
                    status: 400,
                    headers: Vec::new(),
                    body: br#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.5 with this model."}}"#.to_vec(),
                });
            }
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"choices":[{"message":{"content":"ok"}}]}"#.to_vec(),
            })
        });
        host
    }

    #[test]
    fn a_refused_temperature_is_not_asked_again() {
        let host = refusing_temperature_host();
        let config = config(Provider::DeepSeek, Protocol::OpenAiChat, "");

        let first = send(&config, "system", "user", 4096).expect("the retry answers");
        assert_eq!(first.diagnostics.attempts, 2);
        assert_eq!(first.diagnostics.adjusted, vec!["temperature"]);
        assert!(first.diagnostics.cached.is_empty());
        assert_eq!(host.requests().len(), 2);

        let second = send(&config, "system", "user", 4096).expect("the cache answers");
        assert_eq!(
            second.diagnostics.attempts, 1,
            "the refusal is remembered, not asked again"
        );
        assert_eq!(second.diagnostics.cached, vec!["temperature"]);
        assert!(
            second.diagnostics.summary().contains("cached: temperature"),
            "{}",
            second.diagnostics.summary()
        );
        assert_eq!(
            host.requests().len(),
            3,
            "one request, not two: {:?}",
            host.requests()
        );
        let last = host.requests().last().cloned().expect("a request was made");
        assert!(
            !String::from_utf8_lossy(last.body.as_deref().unwrap_or_default())
                .contains("\"temperature\""),
            "the cached shape leaves temperature out"
        );
    }

    #[test]
    fn the_cache_is_per_endpoint_and_model() {
        let host = refusing_temperature_host();
        let config = config(Provider::DeepSeek, Protocol::OpenAiChat, "");
        send(&config, "system", "user", 4096).expect("the retry answers");
        assert_eq!(host.requests().len(), 2);

        let mut other_model = config.clone();
        other_model.model = "another-model".into();
        send(&other_model, "system", "user", 4096).expect("the retry answers");
        assert_eq!(
            host.requests().len(),
            4,
            "another model starts from the configured shape"
        );

        let mut other_endpoint = config.clone();
        other_endpoint.base_url = "https://api.deepseek.com/other".into();
        send(&other_endpoint, "system", "user", 4096).expect("the retry answers");
        assert_eq!(
            host.requests().len(),
            6,
            "another endpoint is another entry"
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
    fn errors_follow_the_ide_language() {
        let host =
            MockHost::install().with_capabilities(&["secrets", "storage", "net:api.deepseek.com"]);
        host.set_locale("zh-CN");
        let deepseek = config(Provider::DeepSeek, Protocol::OpenAiChat, "");

        let error = generate(&deepseek, "system", "user")
            .err()
            .expect("no key")
            .to_string();
        assert!(
            error.starts_with("密钥槽 'api-key' 中没有 API 密钥。"),
            "{error}"
        );

        host.set_secret("api-key", "test-key");
        host.on_fetch(|_| {
            Ok(HttpResponse {
                status: 500,
                headers: Vec::new(),
                body: br#"{"error":{"message":"overloaded"}}"#.to_vec(),
            })
        });
        let error = generate(&deepseek, "system", "user")
            .err()
            .expect("HTTP 500")
            .to_string();
        assert_eq!(error, "LLM 请求失败（HTTP 500）：overloaded");

        let custom = config(Provider::Custom, Protocol::OpenAiChat, "");
        let error = endpoint(&custom).unwrap_err().to_string();
        assert!(error.starts_with("「基础 URL」为空。"), "{error}");

        let error = extract_content(Protocol::OpenAiChat, "{}")
            .unwrap_err()
            .to_string();
        assert_eq!(error, "LLM 响应中没有任何文本：{}");
    }

    #[test]
    fn reports_provider_error_message() {
        assert_eq!(
            error_message(r#"{"error":{"message":"bad key"}}"#),
            "bad key"
        );
    }

    #[test]
    fn changing_temperature_retries_the_configured_value() {
        let host =
            MockHost::install().with_capabilities(&["storage", "secrets", "net:api.deepseek.com"]);
        host.set_secret("api-key", "test");
        host.on_fetch(|request| {
            let body: Value = serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
            if body.get("temperature") == Some(&json!(0.5)) {
                return Ok(HttpResponse { status: 400, headers: vec![], body: br#"{"error":{"message":"Unsupported value: temperature must be between 0 and 0.3"}}"#.to_vec() });
            }
            Ok(HttpResponse {status: 200, headers: vec![], body: br#"{"choices":[{"message":{"content":"ok"}}]}"#.to_vec()})
        });
        let mut config = config(Provider::DeepSeek, Protocol::OpenAiChat, "");
        send(&config, "s", "u", 4096).unwrap();
        config.temperature = 0.2;
        let answer = send(&config, "s", "u", 4096).unwrap();
        assert_eq!(answer.diagnostics.temperature, Some(0.2));
        assert!(answer.diagnostics.cached.is_empty());
        assert_eq!(host.requests().len(), 3);
        let last = host.requests().last().unwrap().clone();
        let body: Value = serde_json::from_slice(last.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["temperature"], 0.2);
    }
}
