use super::{pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    base_url: String,
    api_key: String,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self, String> {
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY environment variable not set".to_string())?;

        Ok(Self { base_url, api_key })
    }

    fn models_endpoint(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.replace("/chat/completions", "/models")
        } else if self.base_url.ends_with("/v1") || self.base_url.ends_with("/v1/") {
            format!("{}/models", self.base_url.trim_end_matches('/'))
        } else {
            "https://api.openai.com/v1/models".to_string()
        }
    }
}

impl Provider for OpenAIProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let mut body_map = serde_json::Map::new();
        body_map.insert(
            "model".to_string(),
            serde_json::Value::String(req.model.clone()),
        );

        let messages = vec![serde_json::json!({
            "role": "user",
            "content": req.prompt
        })];
        body_map.insert("messages".to_string(), serde_json::Value::Array(messages));
        body_map.insert(
            "temperature".to_string(),
            serde_json::Value::from(req.temperature),
        );

        if let Some(schema) = &req.schema {
            if let Ok(parsed_schema) = serde_json::from_str::<serde_json::Value>(schema) {
                body_map.insert(
                    "response_format".to_string(),
                    serde_json::json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "nudge_schema",
                            "schema": parsed_schema
                        }
                    }),
                );
            }
        }

        let body_str = serde_json::Value::Object(body_map).to_string();

        let output = Command::new("curl")
            .arg("-s")
            .arg("-X")
            .arg("POST")
            .arg(&self.base_url)
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", self.api_key))
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-d")
            .arg(&body_str)
            .output()
            .map_err(|e| format!("Failed to execute curl command: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "curl exited with non-zero status: {}",
                output.status
            ));
        }

        let resp_text = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from OpenAI response: {}", e))?;

        let json_resp: serde_json::Value = serde_json::from_str(&resp_text).map_err(|e| {
            format!(
                "Failed to parse OpenAI JSON response: {}. Response: {}",
                e, resp_text
            )
        })?;

        if let Some(err) = json_resp.get("error") {
            return Err(format!("OpenAI API error: {}", err));
        }

        let text = json_resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let usage = &json_resp["usage"];
        let tokens_in = usage["prompt_tokens"]
            .as_u64()
            .unwrap_or_else(|| tokenizer::count_tokens(&req.prompt).max(1));
        let tokens_out = usage["completion_tokens"]
            .as_u64()
            .unwrap_or_else(|| tokenizer::count_tokens(&text).max(1));
        let cache_read = usage["prompt_tokens_details"]["cached_tokens"].as_u64();
        let cost_usd = pricing::calculate_cost(&req.model, tokens_in, tokens_out, cache_read, None);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd,
            model: req.model.clone(),
            cache_read,
            cache_write: None,
        })
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let endpoint = self.models_endpoint();
        let output = Command::new("curl")
            .arg("-s")
            .arg("-X")
            .arg("GET")
            .arg(&endpoint)
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", self.api_key))
            .output()
            .map_err(|e| format!("Failed to query OpenAI models: {}", e))?;

        if !output.status.success() {
            return Err(format!("curl error querying models: {}", output.status));
        }

        let resp_text = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from OpenAI models response: {}", e))?;

        let json_resp: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| format!("Failed to parse models JSON: {}", e))?;

        let mut models = Vec::new();
        if let Some(data) = json_resp["data"].as_array() {
            for item in data {
                if let Some(id) = item["id"].as_str() {
                    let pricing = pricing::get_model_pricing(id);
                    models.push(ModelInfo {
                        id: id.to_string(),
                        name: id.to_string(),
                        provider: "openai".to_string(),
                        input_price_per_1m: pricing.as_ref().map(|p| p.input_usd_per_1m),
                        output_price_per_1m: pricing.as_ref().map(|p| p.output_usd_per_1m),
                        context_window: Some(128_000),
                    });
                }
            }
        }

        // Fallback default list if API returned empty
        if models.is_empty() {
            for &id in &["gpt-4o", "gpt-4o-mini", "o1", "o3-mini", "gpt-4-turbo"] {
                let pricing = pricing::get_model_pricing(id);
                models.push(ModelInfo {
                    id: id.to_string(),
                    name: id.to_string(),
                    provider: "openai".to_string(),
                    input_price_per_1m: pricing.as_ref().map(|p| p.input_usd_per_1m),
                    output_price_per_1m: pricing.as_ref().map(|p| p.output_usd_per_1m),
                    context_window: Some(128_000),
                });
            }
        }

        Ok(models)
    }
}
