use super::{http, pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AntigravityProvider {
    base_url: String,
    model: Option<String>,
    api_key: Option<String>,
}

impl Default for AntigravityProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AntigravityProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("NUDGE_ANTIGRAVITY_BASE_URL")
            .or_else(|_| std::env::var("NUDGE_BASE_URL"))
            .unwrap_or_else(|_| "http://127.0.0.1:8642".to_string());

        let model = std::env::var("NUDGE_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty());
        let api_key = std::env::var("NUDGE_API_KEY")
            .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
            .ok()
            .filter(|k| !k.trim().is_empty());

        Self {
            base_url,
            model,
            api_key,
        }
    }

    pub fn try_new() -> Result<Self, String> {
        let p = Self::new();
        if p.api_key.is_none() {
            Err(
                "error[E-auth-0404]: provider adapter 'antigravity' is disabled because no official public OAuth contract exists"
                    .to_string(),
            )
        } else {
            Ok(p)
        }
    }

    /// Try calling local HTTP proxy with first-party API key credentials
    fn call_via_proxy(&self, req: &LlmRequest, model: &str) -> Result<LlmResponse, String> {
        let endpoint = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );

        let escaped_prompt = escape_json_string(&req.prompt);
        let escaped_model = escape_json_string(model);

        let json_payload = if let Some(schema) = &req.schema {
            let escaped_schema = escape_json_string(schema);
            format!(
                r#"{{"model": "{}", "messages": [{{"role": "user", "content": "{}"}}], "temperature": {}, "response_format": {{"type": "json_object"}}}}"#,
                escaped_model,
                escaped_schema_content(escaped_prompt, &escaped_schema),
                req.temperature
            )
        } else {
            format!(
                r#"{{"model": "{}", "messages": [{{"role": "user", "content": "{}"}}], "temperature": {}}}"#,
                escaped_model, escaped_prompt, req.temperature
            )
        };

        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        if let Some(key) = &self.api_key {
            headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
        }

        let (status, response_body) = http::post_json(&endpoint, &headers, &json_payload)?;

        if !(200..300).contains(&status) {
            return Err(format!(
                "Antigravity proxy returned HTTP status {}: {}",
                status, response_body
            ));
        }
        if response_body.trim().is_empty() {
            return Err("Empty response from proxy".to_string());
        }

        let json_val = crate::json::parse(&response_body).map_err(|e| {
            format!(
                "Failed to parse JSON response from Antigravity proxy: {}. Body: {}",
                e, response_body
            )
        })?;

        let content = json_val
            .get("choices")
            .and_then(|c| c.idx(0))
            .and_then(|ch| ch.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                format!(
                    "Invalid response format from Antigravity proxy: missing choices[0].message.content. Body: {}",
                    response_body
                )
            })?;

        // Extract usage if present
        let (tokens_in, tokens_out, cache_read) = if let Some(usage) = json_val.get("usage") {
            let tin = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_num())
                .map(|f| f as u64)
                .unwrap_or_else(|| tokenizer::count_tokens(&req.prompt).max(1));
            let tout = usage
                .get("completion_tokens")
                .and_then(|v| v.as_num())
                .map(|f| f as u64)
                .unwrap_or_else(|| tokenizer::count_tokens(content).max(1));
            let cread = usage
                .get("prompt_tokens_details")
                .and_then(|v| v.get("cached_tokens"))
                .and_then(|v| v.as_num())
                .map(|f| f as u64);
            (tin, tout, cread)
        } else {
            let tin = tokenizer::count_tokens(&req.prompt).max(1);
            let tout = tokenizer::count_tokens(content).max(1);
            (tin, tout, None)
        };

        let cost_usd = pricing::calculate_cost(model, tokens_in, tokens_out, cache_read, None);

        Ok(LlmResponse {
            text: content.to_string(),
            tokens_in,
            tokens_out,
            cost_usd,
            model: model.to_string(),
            cache_read,
            cache_write: None,
        })
    }
}

impl Provider for AntigravityProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        if self.api_key.is_none() {
            return Err(
                "error[E-auth-0404]: provider adapter 'antigravity' is disabled because no official public OAuth contract exists"
                    .to_string(),
            );
        }
        let raw_model = if !req.model.is_empty() && req.model != "default" && req.model != "auto" {
            req.model.as_str()
        } else {
            self.model.as_deref().unwrap_or("gemini-3.6-flash-medium")
        };
        let clean_model = pricing::normalize_model_name(raw_model);

        self.call_via_proxy(req, &clean_model)
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        // 1. Try local proxy /v1/models if responding
        let endpoint = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let mut headers = Vec::new();
        if let Some(key) = &self.api_key {
            headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
        }

        let mut models = Vec::new();
        if let Ok((status, resp_text)) =
            http::get_with_timeout(&endpoint, &headers, Duration::from_secs(1))
        {
            if (200..300).contains(&status) {
                if let Ok(json_val) = crate::json::parse(&resp_text) {
                    if let Some(crate::json::Json::Arr(data)) = json_val.get("data") {
                        for item in data {
                            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                                let pricing = pricing::get_model_pricing(id);
                                models.push(ModelInfo {
                                    id: id.to_string(),
                                    name: id.to_string(),
                                    provider: "antigravity".to_string(),
                                    input_price_per_1m: pricing
                                        .as_ref()
                                        .map(|p| p.input_usd_per_1m),
                                    output_price_per_1m: pricing
                                        .as_ref()
                                        .map(|p| p.output_usd_per_1m),
                                    context_window: Some(1_000_000),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. If proxy didn't return, return the full verified Antigravity fleet
        if models.is_empty() {
            let fleet = [
                "gemini-3-flash",
                "gemini-3.1-pro-high",
                "gemini-3.1-pro-low",
                "gemini-3.6-flash-high",
                "gemini-3.6-flash-medium",
                "gemini-3.6-flash-low",
                "gemini-3.8-flash-tiered",
                "gemini-2.5-flash",
                "gemini-2.5-pro",
                "claude-sonnet-4-6",
                "claude-opus-4-6-thinking",
                "gpt-oss-120b-medium",
            ];
            for id in fleet {
                let pricing = pricing::get_model_pricing(id);
                models.push(ModelInfo {
                    id: id.to_string(),
                    name: id.to_string(),
                    provider: "antigravity".to_string(),
                    input_price_per_1m: pricing.as_ref().map(|p| p.input_usd_per_1m),
                    output_price_per_1m: pricing.as_ref().map(|p| p.output_usd_per_1m),
                    context_window: Some(1_000_000),
                });
            }
        }

        Ok(models)
    }
}

fn escape_json_string(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn escaped_schema_content(prompt: String, schema: &str) -> String {
    format!("{}\n\nOutput strictly matching schema: {}", prompt, schema)
}
