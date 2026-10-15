//! GitHub Copilot provider adapter (RFC 8628 Device Flow proven path).
//!
//! Gated by ProviderGatePolicy::DeviceFlow. Uses first-party RFC 8628 device flow
//! or explicit environment tokens. Never harvests foreign tokens from other tools.

use super::{http, pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};
use crate::auth::{is_tty, CredentialStore, DeviceFlowClient, DeviceFlowConfig};

#[derive(Debug, Clone)]
pub struct CopilotProvider {
    base_url: String,
    token: String,
}

impl CopilotProvider {
    pub fn new() -> Result<Self, String> {
        let base_url = std::env::var("COPILOT_BASE_URL")
            .or_else(|_| std::env::var("GITHUB_COPILOT_BASE_URL"))
            .unwrap_or_else(|_| "https://api.githubcopilot.com/chat/completions".to_string());

        // 1. Check explicit environment variable (NUDGE_AUTH_GITHUB_COPILOT, COPILOT_TOKEN, GITHUB_TOKEN)
        let mut token = std::env::var("NUDGE_AUTH_GITHUB_COPILOT")
            .or_else(|_| std::env::var("COPILOT_TOKEN"))
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .ok()
            .filter(|t| !t.trim().is_empty());

        // 2. Check Nudge's credential store (~/.nudge/credentials.json)
        if token.is_none() {
            if let Ok(store) = CredentialStore::load_default() {
                if let Some(cred) = store.get_provider("github_copilot") {
                    if !cred.is_expired() {
                        token = Some(cred.access_token().to_string());
                    }
                }
            }
        }

        // 3. If token is missing, attempt interactive device flow if TTY
        let token = match token {
            Some(t) => t,
            None => {
                if !is_tty() {
                    return Err(
                        "error[E-auth-0401]: credential missing or uninitialized for provider 'github_copilot': cannot run interactive device-flow in non-interactive session (no TTY)"
                            .to_string(),
                    );
                }
                let config =
                    DeviceFlowConfig::for_provider("github_copilot", Some("model:inference"))?;
                let client = DeviceFlowClient::new(config);
                let cred = client.authenticate("default")?;
                cred.access_token().to_string()
            }
        };

        Ok(Self { base_url, token })
    }

    pub fn with_token(token: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base_url: base_url
                .unwrap_or_else(|| "https://api.githubcopilot.com/chat/completions".to_string()),
            token: token.into(),
        }
    }
}

impl Provider for CopilotProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let mut body_map = serde_json::Map::new();
        body_map.insert(
            "model".to_string(),
            serde_json::Value::String(
                if req.model.is_empty() || req.model == "default" || req.model == "fake:default" {
                    "copilot-chat".to_string()
                } else {
                    req.model.clone()
                },
            ),
        );
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": req.prompt.clone(),
        })];
        body_map.insert("messages".to_string(), serde_json::Value::Array(messages));
        body_map.insert(
            "temperature".to_string(),
            serde_json::Value::from(req.temperature),
        );

        let body_str = serde_json::Value::Object(body_map).to_string();
        let headers = vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", self.token),
            ),
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Editor-Version".to_string(), "vscode/1.85.0".to_string()),
        ];

        let (status, resp_text) = http::post_json(&self.base_url, &headers, &body_str)?;
        if !(200..300).contains(&status) {
            return Err(format!(
                "HTTP error {status} from github_copilot: {resp_text}"
            ));
        }

        let json_resp: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| format!("Failed to parse response JSON from github_copilot: {e}"))?;

        let choice = json_resp
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| "github_copilot response missing choices[0]".to_string())?;

        let text = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let usage = json_resp.get("usage");
        let tokens_in = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| tokenizer::count_tokens(&req.prompt));
        let tokens_out = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| tokenizer::count_tokens(&text));

        let cost_usd = pricing::calculate_cost(&req.model, tokens_in, tokens_out, None, None);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd,
            model: req.model.clone(),
            cache_read: None,
            cache_write: None,
        })
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![ModelInfo {
            id: "copilot-chat".to_string(),
            name: "Copilot Chat".to_string(),
            provider: "github_copilot".to_string(),
            input_price_per_1m: Some(0.0),
            output_price_per_1m: Some(0.0),
            context_window: Some(128_000),
        }])
    }
}
