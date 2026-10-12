use super::{pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AntigravityProvider {
    base_url: String,
    model: Option<String>,
    api_key: Option<String>,
    oauth_token: Option<String>,
}

impl Default for AntigravityProvider {
    fn default() -> Self {
        Self::new()
    }
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
            let profile_path =
                PathBuf::from(home).join(".openhands/profiles/antigravity-proxy.json");
            if let Ok(content) = fs::read_to_string(profile_path) {
                if let Ok(val) = crate::json::parse(&content) {
                    let base_url = val
                        .get("base_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let model = val
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let api_key = val
                        .get("api_key")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
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
                if let Ok(val) = crate::json::parse(&content) {
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

    /// Try calling local HTTP proxy, or fall back to native Jcode CLI runner
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

        let mut cmd = Command::new("curl");
        cmd.arg("-s")
            .arg("--connect-timeout")
            .arg("2")
            .arg("-X")
            .arg("POST")
            .arg(&endpoint)
            .arg("-H")
            .arg("Content-Type: application/json");

        if let Some(key) = &self.api_key {
            cmd.arg("-H").arg(format!("Authorization: Bearer {}", key));
        } else if let Some(token) = &self.oauth_token {
            cmd.arg("-H")
                .arg(format!("Authorization: Bearer {}", token));
        }

        cmd.arg("-d").arg(&json_payload);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute curl command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "curl exited with status {}: {}",
                output.status, stderr
            ));
        }

        let response_body = String::from_utf8_lossy(&output.stdout);
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

    /// Fallback direct call via Jcode Antigravity CLI runner
    fn call_via_jcode(&self, req: &LlmRequest, model: &str) -> Result<LlmResponse, String> {
        let jcode_bin = which_jcode().unwrap_or_else(|| "jcode".to_string());

        let target_model = match model {
            "gemini-3-flash" | "default" | "auto" => "gemini-3.6-flash-medium",
            m => m,
        };

        let prompt = if let Some(schema) = &req.schema {
            format!(
                "{}\n\nPlease return only valid JSON matching this schema: {}",
                req.prompt, schema
            )
        } else {
            req.prompt.clone()
        };

        let output = Command::new(&jcode_bin)
            .arg("run")
            .arg("--provider")
            .arg("antigravity")
            .arg("--json")
            .arg("-m")
            .arg(target_model)
            .arg(&prompt)
            .output()
            .map_err(|e| format!("Failed to spawn jcode CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("jcode antigravity run failed: {}", stderr));
        }

        let full_output = String::from_utf8_lossy(&output.stdout);
        if let Ok(json_val) = crate::json::parse(&full_output) {
            let text = json_val
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tin = json_val
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_num())
                .map(|n| n as u64)
                .unwrap_or_else(|| tokenizer::count_tokens(&req.prompt).max(1));
            let tout = json_val
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_num())
                .map(|n| n as u64)
                .unwrap_or_else(|| tokenizer::count_tokens(&text).max(1));
            let cread = json_val
                .get("usage")
                .and_then(|u| u.get("cache_read_input_tokens"))
                .and_then(|v| v.as_num())
                .map(|n| n as u64);
            let cwrite = json_val
                .get("usage")
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .and_then(|v| v.as_num())
                .map(|n| n as u64);
            let cost_usd = pricing::calculate_cost(target_model, tin, tout, cread, cwrite);

            return Ok(LlmResponse {
                text,
                tokens_in: tin,
                tokens_out: tout,
                cost_usd,
                model: target_model.to_string(),
                cache_read: cread,
                cache_write: cwrite,
            });
        }

        let (content, tin, tout, cread, cwrite) =
            parse_jcode_token_output(&full_output, &req.prompt);
        let cost_usd = pricing::calculate_cost(target_model, tin, tout, cread, cwrite);

        Ok(LlmResponse {
            text: content,
            tokens_in: tin,
            tokens_out: tout,
            cost_usd,
            model: target_model.to_string(),
            cache_read: cread,
            cache_write: cwrite,
        })
    }
}

fn which_jcode() -> Option<String> {
    if let Some(home) = dirs_next_home() {
        let p = PathBuf::from(home).join(".local/bin/jcode");
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

/// Parse `[Tokens] upload: X download: Y cache_read: Z cache_write: W` from jcode output
fn parse_jcode_token_output(
    raw: &str,
    original_prompt: &str,
) -> (String, u64, u64, Option<u64>, Option<u64>) {
    let mut content_lines = Vec::new();
    let mut tin = None;
    let mut tout = None;
    let mut cread = None;
    let mut cwrite = None;

    for line in raw.lines() {
        if line.contains("[Tokens]") && line.contains("upload:") && line.contains("download:") {
            // Parse tokens
            for part in line.split_whitespace() {
                if let Some(val) = extract_num_after(part, "upload:") {
                    tin = Some(val);
                } else if let Some(val) = extract_num_after(part, "download:") {
                    tout = Some(val);
                } else if let Some(val) = extract_num_after(part, "cache_read:") {
                    cread = Some(val);
                } else if let Some(val) = extract_num_after(part, "cache_write:") {
                    cwrite = Some(val);
                }
            }
        } else if !line.starts_with("[swarm]") && !line.contains("coordinator model:") {
            content_lines.push(line);
        }
    }

    let text = content_lines.join("\n").trim().to_string();
    let tokens_in = tin.unwrap_or_else(|| tokenizer::count_tokens(original_prompt).max(1));
    let tokens_out = tout.unwrap_or_else(|| tokenizer::count_tokens(&text).max(1));

    (text, tokens_in, tokens_out, cread, cwrite)
}

fn extract_num_after(s: &str, prefix: &str) -> Option<u64> {
    if let Some(idx) = s.find(prefix) {
        let rest = &s[idx + prefix.len()..];
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        num_str.parse::<u64>().ok()
    } else {
        None
    }
}

fn dirs_next_home() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
}

impl Provider for AntigravityProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let raw_model = if !req.model.is_empty() && req.model != "default" && req.model != "auto" {
            req.model.as_str()
        } else {
            self.model.as_deref().unwrap_or("gemini-3.6-flash-medium")
        };
        let clean_model = pricing::normalize_model_name(raw_model);

        // Try local proxy first
        match self.call_via_proxy(req, &clean_model) {
            Ok(resp) => Ok(resp),
            Err(_proxy_err) => {
                // Fall back to native Jcode CLI runner
                self.call_via_jcode(req, &clean_model)
            }
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        // 1. Try local proxy /v1/models if responding
        let endpoint = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let output = Command::new("curl")
            .arg("-s")
            .arg("--connect-timeout")
            .arg("1")
            .arg("-X")
            .arg("GET")
            .arg(&endpoint)
            .output();

        let mut models = Vec::new();
        if let Ok(out) = output {
            if out.status.success() {
                if let Ok(resp_text) = String::from_utf8(out.stdout) {
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
