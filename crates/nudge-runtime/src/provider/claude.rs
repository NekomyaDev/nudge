use super::{pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ClaudeProvider {
    base_url: String,
    api_key: String,
}

impl ClaudeProvider {
    pub fn new() -> Result<Self, String> {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string());
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY environment variable not set".to_string())?;

        Ok(Self { base_url, api_key })
    }
}

impl Provider for ClaudeProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": req.prompt
        })];

        let body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "max_tokens": 4096,
            "temperature": req.temperature,
        });

        let body_str = body.to_string();

        let output = Command::new("curl")
            .arg("-s")
            .arg("-X")
            .arg("POST")
            .arg(&self.base_url)
            .arg("-H")
            .arg(format!("x-api-key: {}", self.api_key))
            .arg("-H")
            .arg("anthropic-version: 2023-06-01")
            .arg("-H")
            .arg("content-type: application/json")
            .arg("-d")
            .arg(&body_str)
            .output()
            .map_err(|e| format!("Failed to execute curl command for Anthropic: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "curl exited with non-zero status for Anthropic: {}",
                output.status
            ));
        }

        let resp_text = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from Anthropic response: {}", e))?;

        let json_resp: serde_json::Value = serde_json::from_str(&resp_text).map_err(|e| {
            format!(
                "Failed to parse Anthropic JSON response: {}. Response: {}",
                e, resp_text
            )
        })?;

        let text = json_resp["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = &json_resp["usage"];
        let tokens_in = usage["input_tokens"]
            .as_u64()
            .unwrap_or_else(|| tokenizer::count_tokens(&req.prompt).max(1));
        let tokens_out = usage["output_tokens"]
            .as_u64()
            .unwrap_or_else(|| tokenizer::count_tokens(&text).max(1));
        let cache_read = usage["cache_read_input_tokens"].as_u64();
        let cache_write = usage["cache_creation_input_tokens"].as_u64();
        let cost_usd =
            pricing::calculate_cost(&req.model, tokens_in, tokens_out, cache_read, cache_write);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd,
            model: req.model.clone(),
            cache_read,
            cache_write,
        })
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let output = Command::new("curl")
            .arg("-s")
            .arg("-X")
            .arg("GET")
            .arg("https://api.anthropic.com/v1/models")
            .arg("-H")
            .arg(format!("x-api-key: {}", self.api_key))
            .arg("-H")
            .arg("anthropic-version: 2023-06-01")
            .output();

        let mut models = Vec::new();
        if let Ok(out) = output {
            if out.status.success() {
                if let Ok(resp_text) = String::from_utf8(out.stdout) {
                    if let Ok(json_resp) = serde_json::from_str::<serde_json::Value>(&resp_text) {
                        if let Some(data) = json_resp["data"].as_array() {
                            for item in data {
                                if let Some(id) = item["id"].as_str() {
                                    let pricing = pricing::get_model_pricing(id);
                                    models.push(ModelInfo {
                                        id: id.to_string(),
                                        name: id.to_string(),
                                        provider: "anthropic".to_string(),
                                        input_price_per_1m: pricing
                                            .as_ref()
                                            .map(|p| p.input_usd_per_1m),
                                        output_price_per_1m: pricing
                                            .as_ref()
                                            .map(|p| p.output_usd_per_1m),
                                        context_window: Some(200_000),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if models.is_empty() {
            let defaults = [
                "claude-3-5-sonnet-20241022",
                "claude-3-5-haiku-20241022",
                "claude-3-opus-20240229",
                "claude-sonnet-4-5",
                "claude-haiku-4-5",
            ];
            for id in defaults {
                let pricing = pricing::get_model_pricing(id);
                models.push(ModelInfo {
                    id: id.to_string(),
                    name: id.to_string(),
                    provider: "anthropic".to_string(),
                    input_price_per_1m: pricing.as_ref().map(|p| p.input_usd_per_1m),
                    output_price_per_1m: pricing.as_ref().map(|p| p.output_usd_per_1m),
                    context_window: Some(200_000),
                });
            }
        }

        Ok(models)
    }
}
