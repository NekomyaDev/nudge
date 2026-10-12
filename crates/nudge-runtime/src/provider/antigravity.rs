use super::{LlmRequest, LlmResponse, Provider};
use std::process::Command;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AntigravityProvider {
    base_url: String,
    model: Option<String>,
    api_key: Option<String>,
    oauth_token: Option<String>,
}

impl AntigravityProvider {
    pub fn new() -> Self {
        // 1. Check environment variables
        let base_url = std::env::var("NUDGE_ANTIGRAVITY_BASE_URL")
            .or_else(|_| std::env::var("NUDGE_BASE_URL"))
            .unwrap_or_else(|_| "http://127.0.0.1:8642".to_string());

        let mut model = std::env::var("NUDGE_MODEL").ok();
        let mut api_key = std::env::var("NUDGE_API_KEY")
            .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
            .ok();

        // 2. Check ~/.openhands/profiles/antigravity-proxy.json
        if let Some(home) = dirs_next_home() {
            let profile_path = PathBuf::from(&home).join(".openhands/profiles/antigravity-proxy.json");
            if profile_path.exists() {
                if let Ok(content) = fs::read_to_string(&profile_path) {
                    // Simple manual parsing or JSON parsing if json helper is available in nudge_runtime
                    // nudge_runtime has a json module: crate::json::JsonValue or similar. Let's inspect crate::json or use simple string parsing/serde if available.
                    // Let's check what json module provides in crate::json.
                }
            }

            // 3. Check ~/.jcode/antigravity_oauth.json for access tokens
            let oauth_path = PathBuf::from(&home).join(".jcode/antigravity_oauth.json");
            if oauth_path.exists() {
                if let Ok(content) = fs::read_to_string(&oauth_path) {
                    // parse token
                }
            }
        }

        // Let's do robust file parsing using crate::json if available, or simple fallback.
        let (profile_base_url, profile_model, profile_api_key) = Self::read_openhands_profile();
        let oauth_token = Self::read_oauth_token();

        let final_base_url = if base_url != "http://127.0.0.1:8642" {
            base_url
        } else {
            profile_base_url.unwrap_or(base_url)
        };

        if model.is_none() {
            model = profile_model;
        }

        if api_key.is_none() {
            api_key = profile_api_key;
        }

        Self {
            base_url: final_base_url,
            model,
            api_key,
            oauth_token,
        }
    }

    fn read_openhands_profile() -> (Option<String>, Option<String>, Option<String>) {
        if let Some(home) = dirs_next_home() {
            let profile_path = PathBuf::from(home).join(".openhands/profiles/antigravity-proxy.json");
            if let Ok(content) = fs::read_to_string(profile_path) {
                // Parse using crate::json if possible, or basic parsing
                if let Ok(val) = crate::json::parse_json(&content) {
                    let base_url = val.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let model = val.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let api_key = val.get("api_key").and_then(|v| v.as_str()).map(|s| s.to_string());
                    return (base_url, model, api_key);
                }
            }
        }
        (None, None, None)
    }

    fn read_oauth_token() -> Option<String> {
        if let Some(home) = dirs_next_home() {
            let oauth_path = PathBuf::from(home).join(".jcode/antigravity_oauth.json");
            if let Ok(content) = fs::read_to_string(oauth_path) {
                if let Ok(val) = crate::json::parse_json(&content) {
                    if let Some(token) = val.get("access_token").and_then(|v| v.as_str()) {
                        return Some(token.to_string());
                    }
                    if let Some(token) = val.get("token").and_then(|v| v.as_str()) {
                        return Some(token.to_string());
                    }
                }
            }
        }
        None
    }
}

fn dirs_next_home() -> Option<String> {
    std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())
}

impl Provider for AntigravityProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let model = self.model.as_deref().unwrap_or(&req.model);
        let endpoint = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));

        // Format OpenAI-compatible payload using crate::json or string formatting
        // {"model": model, "messages": [{"role": "user", "content": req.prompt}], "temperature": req.temperature}
        let escaped_prompt = escape_json_string(&req.prompt);
        let escaped_model = escape_json_string(model);
        
        let mut json_payload = format!(
            r#"{{"model": "{}", "messages": [{{"role": "user", "content": "{}"}}], "temperature": {}}}"#,
            escaped_model, escaped_prompt, req.temperature
        );

        if let Some(schema) = &req.schema {
            let escaped_schema = escape_json_string(schema);
            // Insert response_format or schema if supported, or include in prompt / payload
            // For OpenAI-compatible endpoints supporting json_object or schema:
            json_payload = format!(
                r#"{{"model": "{}", "messages": [{{"role": "user", "content": "{}"}}], "temperature": {}, "response_format": {{"type": "json_object"}}}}"#,
                escaped_model, escaped_schema_content(escaped_prompt, &escaped_schema), req.temperature
            );
        }

        let mut cmd = Command::new("curl");
        cmd.arg("-s")
           .arg("-X")
           .arg("POST")
           .arg(&endpoint)
           .arg("-H")
           .arg("Content-Type: application/json");

        if let Some(key) = &self.api_key {
            cmd.arg("-H").arg(format!("Authorization: Bearer {}", key));
        } else if let Some(token) = &self.oauth_token {
            cmd.arg("-H").arg(format!("Authorization: Bearer {}", token));
        }

        cmd.arg("-d").arg(&json_payload);

        let output = cmd.output().map_err(|e| format!("Failed to execute curl command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("curl exited with status {}: {}", output.status, stderr));
        }

        let response_body = String::from_utf8_lossy(&output.stdout);

        // Parse response using crate::json
        let json_val = crate::json::parse_json(&response_body)
            .map_err(|e| format!("Failed to parse JSON response from Antigravity proxy: {}. Body: {}", e, response_body))?;

        // Extract content from choices[0].message.content
        let content = json_val
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|ch| ch.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("Invalid response format from Antigravity proxy: missing choices[0].message.content. Body: {}", response_body))?;

        let tokens_in = (req.prompt.chars().count() as u64 / 4).max(1);
        let tokens_out = (content.chars().count() as u64 / 4).max(1);

        Ok(LlmResponse {
            text: content.to_string(),
            tokens_in,
            tokens_out,
            cost_usd: 0.0,
            model: model.to_string(),
        })
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
