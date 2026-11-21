//! HTTP providers for the UMMP AI agent system (Phase 7).
//!
//! - [`AnthropicProvider`]: Claude Messages API over raw HTTP (Rust has no
//!   official Anthropic SDK). Structured output via `output_config.format`
//!   (`json_schema`), explicit effort, and the server-side refusal fallback
//!   on models that support it.
//! - [`OpenAiCompatibleProvider`]: any `/chat/completions` endpoint, e.g. a
//!   local Ollama server, for offline use.
//!
//! Credentials come from environment variables only and never appear in
//! errors, logs or the audit trail.

mod modrinth;
pub use modrinth::ModrinthIndex;

use serde_json::{json, Value};
use std::time::Duration;
use ummp_core::ai::{AiError, AiProvider, AiRequest, AiResponse, Usage};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// Models that accept `fallbacks: "default"` on the Claude API.
const FALLBACK_MODELS: &[&str] = &[
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-fable-5-1",
    "claude-sonnet-5-5",
];

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(timeout).build()
}

/// Map an HTTP status to a retryable or fatal error. The message comes from
/// the provider's error body, never from our request (no credentials).
fn status_error(status: u16, body: &str) -> AiError {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("HTTP {status}"));
    match status {
        408 | 409 | 429 | 500..=599 => AiError::Transient(format!("HTTP {status}: {detail}")),
        _ => AiError::Fatal(format!("HTTP {status}: {detail}")),
    }
}

/// Send a JSON POST and return `(status, body)`; transport failures are transient.
fn post(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, String)],
    body: &Value,
) -> Result<(u16, String), AiError> {
    let mut req = agent.post(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    match req.send_json(body.clone()) {
        Ok(resp) => {
            let status = resp.status();
            let text = resp
                .into_string()
                .map_err(|e| AiError::Transient(format!("reading response: {e}")))?;
            Ok((status, text))
        }
        Err(ureq::Error::Status(code, resp)) => Ok((code, resp.into_string().unwrap_or_default())),
        Err(ureq::Error::Transport(t)) => {
            Err(AiError::Transient(format!("transport: {}", t.kind())))
        }
    }
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

enum Credential {
    ApiKey(String),
    /// OAuth access token (`ANTHROPIC_AUTH_TOKEN`), sent as a bearer token.
    Bearer(String),
}

pub struct AnthropicProvider {
    credential: Credential,
    base_url: String,
    agent: ureq::Agent,
}

impl AnthropicProvider {
    /// Reads `ANTHROPIC_API_KEY` (or `ANTHROPIC_AUTH_TOKEN`) and the optional
    /// `ANTHROPIC_BASE_URL`.
    pub fn from_env() -> Result<Self, AiError> {
        let non_empty = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let credential = if let Some(key) = non_empty("ANTHROPIC_API_KEY") {
            Credential::ApiKey(key)
        } else if let Some(token) = non_empty("ANTHROPIC_AUTH_TOKEN") {
            Credential::Bearer(token)
        } else {
            return Err(AiError::Fatal(
                "set ANTHROPIC_API_KEY (or ANTHROPIC_AUTH_TOKEN) to use the Anthropic provider"
                    .into(),
            ));
        };
        let base_url =
            non_empty("ANTHROPIC_BASE_URL").unwrap_or_else(|| "https://api.anthropic.com".into());
        Ok(Self::new_with(credential, &base_url))
    }

    /// Provider with an explicit API key and base URL (tests, custom gateways).
    pub fn with_api_key(api_key: String, base_url: &str) -> Self {
        Self::new_with(Credential::ApiKey(api_key), base_url)
    }

    fn new_with(credential: Credential, base_url: &str) -> Self {
        Self {
            credential,
            base_url: base_url.trim_end_matches('/').to_string(),
            // Non-streaming requests with max_tokens ~16k can take minutes.
            agent: agent(Duration::from_mins(10)),
        }
    }

    pub fn request_body(req: &AiRequest) -> Value {
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system,
            "messages": [{"role": "user", "content": req.user}],
            "output_config": {
                "effort": req.effort,
                "format": {"type": "json_schema", "schema": req.schema}
            }
        });
        if FALLBACK_MODELS.contains(&req.model.as_str()) {
            body["fallbacks"] = json!("default");
        }
        body
    }

    fn headers(&self, model: &str) -> Vec<(&'static str, String)> {
        let mut betas = Vec::new();
        if FALLBACK_MODELS.contains(&model) {
            betas.push(FALLBACK_BETA);
        }
        let mut h = vec![
            ("anthropic-version", ANTHROPIC_VERSION.to_string()),
            ("content-type", "application/json".to_string()),
        ];
        match &self.credential {
            Credential::ApiKey(k) => h.push(("x-api-key", k.clone())),
            Credential::Bearer(t) => {
                h.push(("authorization", format!("Bearer {t}")));
                betas.push(OAUTH_BETA);
            }
        }
        if !betas.is_empty() {
            h.push(("anthropic-beta", betas.join(",")));
        }
        h
    }

    pub fn parse_response(
        status: u16,
        body: &str,
        requested_model: &str,
    ) -> Result<AiResponse, AiError> {
        if !(200..300).contains(&status) {
            return Err(status_error(status, body));
        }
        let v: Value = serde_json::from_str(body)
            .map_err(|e| AiError::InvalidOutput(format!("response is not JSON: {e}")))?;
        match v["stop_reason"].as_str() {
            Some("refusal") => {
                let d = &v["stop_details"];
                let why = d["explanation"]
                    .as_str()
                    .or_else(|| d["category"].as_str())
                    .unwrap_or("no details");
                return Err(AiError::Refused(why.to_string()));
            }
            Some("max_tokens") => return Err(AiError::Truncated),
            _ => {}
        }
        // Thinking blocks and other non-text blocks are skipped.
        let text: String = v["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        if text.is_empty() {
            return Err(AiError::InvalidOutput("no text content in response".into()));
        }
        Ok(AiResponse {
            text,
            usage: Usage {
                input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            },
            model: v["model"].as_str().unwrap_or(requested_model).to_string(),
        })
    }
}

impl AiProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn complete(&self, req: &AiRequest) -> Result<AiResponse, AiError> {
        let url = format!("{}/v1/messages", self.base_url);
        let (status, body) = post(
            &self.agent,
            &url,
            &self.headers(&req.model),
            &Self::request_body(req),
        )?;
        Self::parse_response(status, &body, &req.model)
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible (local models)
// ---------------------------------------------------------------------------

pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    agent: ureq::Agent,
}

impl OpenAiCompatibleProvider {
    /// `base_url` such as `http://localhost:11434/v1`. The key, if any, is
    /// read from `OPENAI_API_KEY`.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            agent: agent(Duration::from_mins(10)),
        }
    }

    /// Provider with an explicit API key. The caller owns the credential lifecycle.
    pub fn with_api_key(base_url: &str, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: Some(api_key),
            agent: agent(Duration::from_mins(10)),
        }
    }

    pub fn request_body(req: &AiRequest) -> Value {
        json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "ummp_output", "strict": true, "schema": req.schema}
            }
        })
    }

    pub fn parse_response(
        status: u16,
        body: &str,
        requested_model: &str,
    ) -> Result<AiResponse, AiError> {
        if !(200..300).contains(&status) {
            return Err(status_error(status, body));
        }
        let v: Value = serde_json::from_str(body)
            .map_err(|e| AiError::InvalidOutput(format!("response is not JSON: {e}")))?;
        let choice = &v["choices"][0];
        if choice["finish_reason"] == "length" {
            return Err(AiError::Truncated);
        }
        if let Some(refusal) = choice["message"]["refusal"].as_str() {
            return Err(AiError::Refused(refusal.to_string()));
        }
        let text = choice["message"]["content"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AiError::InvalidOutput("no message content in response".into()))?;
        Ok(AiResponse {
            text: text.to_string(),
            usage: Usage {
                input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            },
            model: v["model"].as_str().unwrap_or(requested_model).to_string(),
        })
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &'static str {
        "openai-compatible"
    }

    fn complete(&self, req: &AiRequest) -> Result<AiResponse, AiError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut headers = vec![("content-type", "application/json".to_string())];
        if let Some(k) = &self.api_key {
            headers.push(("authorization", format!("Bearer {k}")));
        }
        let (status, body) = post(&self.agent, &url, &headers, &Self::request_body(req))?;
        Self::parse_response(status, &body, &req.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn req(model: &str) -> AiRequest {
        AiRequest {
            model: model.into(),
            effort: "high".into(),
            max_tokens: 16_000,
            system: "sys".into(),
            user: "hello".into(),
            schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        }
    }

    #[test]
    fn test_anthropic_body_structured_output_and_fallbacks() {
        let b = AnthropicProvider::request_body(&req("claude-opus-5-5"));
        assert_eq!(b["output_config"]["format"]["type"], "json_schema");
        assert_eq!(b["output_config"]["effort"], "high");
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["messages"][0]["role"], "user");
        assert!(b.get("tool_choice").is_none()); // forced tool use 400s on Opus 5.5
        assert!(b.get("thinking").is_none()); // adaptive by default
        let b = AnthropicProvider::request_body(&req("claude-haiku-5-5"));
        assert!(b.get("fallbacks").is_none());
    }

    #[test]
    fn test_anthropic_parse_text_skips_thinking() {
        let body = r#"{"model":"claude-opus-5-5","stop_reason":"end_turn",
            "content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"a\":1}"}],
            "usage":{"input_tokens":10,"output_tokens":5}}"#;
        let r = AnthropicProvider::parse_response(200, body, "x").unwrap();
        assert_eq!(r.text, r#"{"a":1}"#);
        assert_eq!(r.usage.total(), 15);
        assert_eq!(r.model, "claude-opus-5-5");
    }

    #[test]
    fn test_anthropic_parse_errors() {
        let refusal = r#"{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber","explanation":null},"content":[]}"#;
        assert_eq!(
            AnthropicProvider::parse_response(200, refusal, "m").unwrap_err(),
            AiError::Refused("cyber".into())
        );
        let trunc = r#"{"stop_reason":"max_tokens","content":[{"type":"text","text":"{"}]}"#;
        assert_eq!(
            AnthropicProvider::parse_response(200, trunc, "m").unwrap_err(),
            AiError::Truncated
        );
        let overloaded =
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(matches!(
            AnthropicProvider::parse_response(529, overloaded, "m").unwrap_err(),
            AiError::Transient(m) if m.contains("Overloaded")
        ));
        assert!(matches!(
            AnthropicProvider::parse_response(401, "{}", "m").unwrap_err(),
            AiError::Fatal(_)
        ));
    }

    #[test]
    fn test_openai_body_and_parse() {
        let b = OpenAiCompatibleProvider::request_body(&req("qwen"));
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["response_format"]["type"], "json_schema");
        let ok = r#"{"model":"qwen","choices":[{"finish_reason":"stop","message":{"content":"{}"}}],
                    "usage":{"prompt_tokens":3,"completion_tokens":2}}"#;
        let r = OpenAiCompatibleProvider::parse_response(200, ok, "qwen").unwrap();
        assert_eq!((r.text.as_str(), r.usage.total()), ("{}", 5));
        let len = r#"{"choices":[{"finish_reason":"length","message":{"content":"{"}}]}"#;
        assert_eq!(
            OpenAiCompatibleProvider::parse_response(200, len, "q").unwrap_err(),
            AiError::Truncated
        );
    }

    /// Real HTTP round trip against a one-shot local server: checks the path,
    /// headers and body that go over the wire, and that the key never leaks
    /// into an error.
    #[test]
    fn test_anthropic_http_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let reply = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
            write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });

        let p = AnthropicProvider::with_api_key("sk-test-secret".into(), &format!("http://{addr}"));
        let err = p.complete(&req("claude-opus-5-5")).unwrap_err();
        let (head, body) = server.join().unwrap();

        assert!(head.starts_with("POST /v1/messages HTTP/1.1"));
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("x-api-key: sk-test-secret"));
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        assert!(lower.contains("anthropic-beta: server-side-fallback-2026-07-01"));
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["model"], "claude-opus-5-5");
        assert!(matches!(&err, AiError::Fatal(m) if m.contains("invalid x-api-key")));
        assert!(!err.to_string().contains("sk-test-secret"));
    }
}
