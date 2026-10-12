use super::{LlmRequest, LlmResponse, Provider};
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
        
        Ok(Self {
            base_url,
            api_key,
        })
    }
}

impl Provider for ClaudeProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": req.prompt
            })
        ];

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
            .arg("-H").arg(format!("x-api-key: {}", self.api_key))
            .arg("-H").arg("anthropic-version: 2023-06-01")
            .arg("-H").arg("content-type: application/json")
            .arg("-d").arg(&body_str)
            .output()
            .map_err(|e| format!("Failed to execute curl command for Anthropic: {}", e))?;

        if !output.status.success() {
            return Err(format!("curl exited with non-zero status for Anthropic: {}", output.status));
        }

        let resp_text = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 from Anthropic response: {}", e))?;

        let json_resp: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| format!("Failed to parse Anthropic JSON response: {}. Response: {}", e, resp_text))?;

        let text = json_resp["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = &json_resp["usage"];
        let tokens_in = usage["input_tokens"].as_u64().unwrap_or(0);
        let tokens_out = usage["output_tokens"].as_u64().unwrap_or(0);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd: 0.0,
            model: req.model.clone(),
        })
    }
}
