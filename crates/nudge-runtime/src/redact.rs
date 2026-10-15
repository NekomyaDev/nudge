use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::sync::LazyLock;

static PEM_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----")
        .expect("valid pem regex")
});

static AUTH_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bAuthorization\s*:\s*(Bearer|Basic|Token)\s+[A-Za-z0-9_\-\.\+/=]+")
        .expect("valid auth header regex")
});

static BEARER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bBearer\s+[A-Za-z0-9_\-\.]{16,}").expect("valid bearer regex"));

static KV_SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(api_key|apikey|secret|token|password|passwd|auth_token|access_token|private_key|client_secret)(["']?\s*[:=]\s*["']?)([^"'\s,;}]+)(["']?)"#)
        .expect("valid kv secret regex")
});

static VENDOR_SK_ANT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bsk-ant-[a-zA-Z0-9_\-]{20,}").expect("valid anthropic key regex")
});

static VENDOR_SK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsk-[a-zA-Z0-9_\-]{20,}").expect("valid openai key regex"));

static VENDOR_GITHUB_PAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bgithub_pat_[a-zA-Z0-9_]{50,}").expect("valid github pat regex")
});

static VENDOR_GHP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bghp_[a-zA-Z0-9]{36}").expect("valid ghp regex"));

static VENDOR_GLPAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bglpat-[a-zA-Z0-9\-]{20,}").expect("valid glpat regex"));

static VENDOR_AKIA_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("valid akia regex"));

static VENDOR_SLACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bxox[baprs]-[0-9a-zA-Z\-]{10,}").expect("valid slack token regex")
});

/// Is trace redaction disabled via `NUDGE_TRACE_REDACT=0` or `false`?
/// Defaults to false (redaction enabled by default, fail-closed).
pub fn is_redact_disabled() -> bool {
    match std::env::var("NUDGE_TRACE_REDACT") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            t == "0" || t == "false"
        }
        Err(_) => false,
    }
}

/// Is environment variable auto-detection redaction disabled via `NUDGE_TRACE_REDACT_ENV=0` or `false`?
/// Defaults to false (auto-detection enabled by default, fail-closed).
pub fn is_env_redact_disabled() -> bool {
    match std::env::var("NUDGE_TRACE_REDACT_ENV") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            t == "0" || t == "false"
        }
        Err(_) => false,
    }
}

fn matches_env_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with("NUDGE_") {
        return false;
    }
    upper.ends_with("_KEY")
        || upper == "KEY"
        || upper.ends_with("_SECRET")
        || upper == "SECRET"
        || upper.ends_with("_TOKEN")
        || upper == "TOKEN"
        || upper.ends_with("_PASSWORD")
        || upper == "PASSWORD"
        || upper.ends_with("_PASS")
        || upper == "PASS"
        || upper.ends_with("_PASSWD")
        || upper == "PASSWD"
        || upper.contains("AUTH")
        || upper.ends_with("_PAT")
        || upper == "PAT"
}

fn load_env_secrets() -> Vec<(String, String)> {
    let mut secrets = Vec::new();
    for (k, v) in std::env::vars() {
        if matches_env_secret_name(&k) && v.len() >= 6 {
            secrets.push((v, k));
        }
    }
    secrets.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    secrets
}

fn load_extra_keys() -> Vec<String> {
    let Ok(val) = std::env::var("NUDGE_TRACE_REDACT_KEYS") else {
        return Vec::new();
    };
    val.split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn load_custom_patterns() -> Vec<Regex> {
    let Ok(val) = std::env::var("NUDGE_TRACE_REDACT_PATTERNS") else {
        return Vec::new();
    };
    let val = val.trim();
    if val.is_empty() {
        return Vec::new();
    }
    let mut patterns = Vec::new();
    if Path::new(val).is_file() {
        if let Ok(content) = std::fs::read_to_string(val) {
            for line in content.lines() {
                let l = line.trim();
                if !l.is_empty() && !l.starts_with('#') {
                    if let Ok(re) = Regex::new(l) {
                        patterns.push(re);
                    }
                }
            }
        }
    } else {
        for part in val.split(',') {
            let p = part.trim();
            if !p.is_empty() {
                if let Ok(re) = Regex::new(p) {
                    patterns.push(re);
                }
            }
        }
    }
    patterns
}

/// Redaction engine configured from environment and defaults.
pub struct Redactor {
    pub env_secrets: Vec<(String, String)>,
    pub extra_keys: Vec<String>,
    pub custom_patterns: Vec<Regex>,
}

impl Default for Redactor {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Redactor {
    /// Construct a Redactor reflecting the current environment.
    pub fn from_env() -> Self {
        let env_secrets = if is_env_redact_disabled() {
            Vec::new()
        } else {
            load_env_secrets()
        };
        let extra_keys = load_extra_keys();
        let custom_patterns = load_custom_patterns();
        Self {
            env_secrets,
            extra_keys,
            custom_patterns,
        }
    }

    /// Check if a JSON object field key matches structural secret rules.
    pub fn is_structural_key(&self, key: &str) -> bool {
        let lower = key.to_ascii_lowercase();
        match lower.as_str() {
            "password" | "secret" | "token" | "auth" | "api_key" | "access_token"
            | "refresh_token" => true,
            _ => self.extra_keys.iter().any(|k| k == &lower),
        }
    }

    /// Apply all string masking layers in order:
    /// 1. PEM private key blocks
    /// 2. Active environment secrets -> [REDACTED:ENV:<NAME>]
    /// 3. Authorization / Bearer headers
    /// 4. Key-value secret patterns
    /// 5. Known vendor token prefixes
    /// 6. Custom patterns from NUDGE_TRACE_REDACT_PATTERNS
    pub fn redact_string(&self, s: &str) -> String {
        let mut text = s.to_string();

        // 1. PEM private key blocks
        if text.contains("-----BEGIN") && text.contains("PRIVATE KEY-----") {
            text = PEM_KEY_RE
                .replace_all(&text, "[REDACTED PRIVATE KEY]")
                .into_owned();
        }

        // 2. Active environment secrets
        for (secret_val, env_name) in &self.env_secrets {
            if text.contains(secret_val) {
                let mask = format!("[REDACTED:ENV:{env_name}]");
                text = text.replace(secret_val, &mask);
            }
        }

        // 3. Authorization headers and Bearer tokens
        let lower = text.to_ascii_lowercase();
        if lower.contains("authorization") {
            text = AUTH_HEADER_RE
                .replace_all(&text, "Authorization: $1 [REDACTED]")
                .into_owned();
        }
        if text.contains("Bearer") {
            text = BEARER_RE
                .replace_all(&text, "Bearer [REDACTED]")
                .into_owned();
        }

        // 4. Key-value secret patterns
        let lower = text.to_ascii_lowercase();
        if lower.contains("key")
            || lower.contains("secret")
            || lower.contains("token")
            || lower.contains("pass")
            || lower.contains("auth")
        {
            text = KV_SECRET_RE
                .replace_all(&text, |caps: &regex::Captures| {
                    let val = &caps[3];
                    if val.starts_with("[REDACTED") {
                        caps[0].to_string()
                    } else {
                        format!(
                            "{}{}[REDACTED]{}",
                            &caps[1],
                            &caps[2],
                            caps.get(4).map_or("", |m| m.as_str())
                        )
                    }
                })
                .into_owned();
        }

        // 5. Vendor token prefixes
        if text.contains("sk-ant-") {
            text = VENDOR_SK_ANT_RE
                .replace_all(&text, "[REDACTED]")
                .into_owned();
        }
        if text.contains("sk-") {
            text = VENDOR_SK_RE.replace_all(&text, "[REDACTED]").into_owned();
        }
        if text.contains("github_pat_") {
            text = VENDOR_GITHUB_PAT_RE
                .replace_all(&text, "[REDACTED]")
                .into_owned();
        }
        if text.contains("ghp_") {
            text = VENDOR_GHP_RE.replace_all(&text, "[REDACTED]").into_owned();
        }
        if text.contains("glpat-") {
            text = VENDOR_GLPAT_RE
                .replace_all(&text, "[REDACTED]")
                .into_owned();
        }
        if text.contains("AKIA") {
            text = VENDOR_AKIA_RE.replace_all(&text, "[REDACTED]").into_owned();
        }
        if text.contains("xox") {
            text = VENDOR_SLACK_RE
                .replace_all(&text, "[REDACTED]")
                .into_owned();
        }

        // 6. Custom patterns from NUDGE_TRACE_REDACT_PATTERNS
        for re in &self.custom_patterns {
            text = re.replace_all(&text, "[REDACTED]").into_owned();
        }

        text
    }

    /// Recursively redact a JSON value.
    pub fn redact_value(&self, val: &mut Value) {
        match val {
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    if self.is_structural_key(k) {
                        *v = Value::String("[REDACTED]".to_string());
                    } else {
                        self.redact_value(v);
                    }
                }
            }
            Value::Array(arr) => {
                for item in arr.iter_mut() {
                    self.redact_value(item);
                }
            }
            Value::String(s) => {
                *s = self.redact_string(s);
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

/// Redact fields of a trace record in place, preserving request_hash, v, seq, and kind.
pub fn redact_record_fields(fields: &mut serde_json::Map<String, Value>) {
    if is_redact_disabled() {
        return;
    }
    let redactor = Redactor::from_env();
    for (key, val) in fields.iter_mut() {
        if key == "request_hash" || key == "v" || key == "seq" || key == "kind" {
            continue;
        }
        if redactor.is_structural_key(key) {
            *val = Value::String("[REDACTED]".to_string());
        } else {
            redactor.redact_value(val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_headers_and_bearer() {
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: Vec::new(),
            custom_patterns: Vec::new(),
        };

        let t1 = "Headers: Authorization: Bearer secret_bearer_token_12345; Content-Type: json";
        assert_eq!(
            redactor.redact_string(t1),
            "Headers: Authorization: Bearer [REDACTED]; Content-Type: json"
        );

        let t2 = "Authorization: Basic dXNlcjpwYXNz";
        assert_eq!(
            redactor.redact_string(t2),
            "Authorization: Basic [REDACTED]"
        );

        let t3 = "Standalone token Bearer 1234567890abcdef1234567890 in body";
        assert_eq!(
            redactor.redact_string(t3),
            "Standalone token Bearer [REDACTED] in body"
        );
    }

    #[test]
    fn test_kv_secret_patterns() {
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: Vec::new(),
            custom_patterns: Vec::new(),
        };

        let t1 = "config: api_key=secret_val_123; other=ok";
        assert_eq!(
            redactor.redact_string(t1),
            "config: api_key=[REDACTED]; other=ok"
        );

        let t2 = "login with password: my_secure_password_99";
        assert_eq!(
            redactor.redact_string(t2),
            "login with password: [REDACTED]"
        );

        let t3 = r#"{"client_secret": "my_client_secret_xyz"}"#;
        assert_eq!(
            redactor.redact_string(t3),
            r#"{"client_secret": "[REDACTED]"}"#
        );
    }

    #[test]
    fn test_vendor_prefixes() {
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: Vec::new(),
            custom_patterns: Vec::new(),
        };

        let t1 = "Anthropic key: sk-ant-api03-1234567890abcdef1234567890";
        assert_eq!(redactor.redact_string(t1), "Anthropic key: [REDACTED]");

        let t2 = "OpenAI key: sk-proj1234567890abcdef1234567890";
        assert_eq!(redactor.redact_string(t2), "OpenAI key: [REDACTED]");

        let t3 = "GitHub: ghp_1234567890abcdef1234567890abcdef1234";
        assert_eq!(redactor.redact_string(t3), "GitHub: [REDACTED]");

        let t4 = "PAT: github_pat_1234567890123456789012345678901234567890123456789012";
        assert_eq!(redactor.redact_string(t4), "PAT: [REDACTED]");

        let t5 = "GitLab: glpat-abcdef12345678901234";
        assert_eq!(redactor.redact_string(t5), "GitLab: [REDACTED]");

        let t6 = "AWS: AKIAIOSFODNN7EXAMPLE key";
        assert_eq!(redactor.redact_string(t6), "AWS: [REDACTED] key");

        let t7 = "Slack: xoxb-1234567890-1234567890";
        assert_eq!(redactor.redact_string(t7), "Slack: [REDACTED]");

        let pem =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0\n-----END RSA PRIVATE KEY-----";
        assert_eq!(redactor.redact_string(pem), "[REDACTED PRIVATE KEY]");
    }

    #[test]
    fn test_structural_json_masking() {
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: vec!["customer_pin".into(), "ssn".into()],
            custom_patterns: Vec::new(),
        };

        let mut val = serde_json::json!({
            "user": "alice",
            "password": "secret_password",
            "token": "tok_12345",
            "details": {
                "customer_pin": 1234,
                "nested_token": "safe",
                "auth": {"method": "bearer"}
            },
            "list": [
                {"secret": "val1"},
                {"name": "bob"}
            ]
        });

        redactor.redact_value(&mut val);

        assert_eq!(val["user"], "alice");
        assert_eq!(val["password"], "[REDACTED]");
        assert_eq!(val["token"], "[REDACTED]");
        assert_eq!(val["details"]["customer_pin"], "[REDACTED]");
        assert_eq!(val["details"]["nested_token"], "safe");
        assert_eq!(val["details"]["auth"], "[REDACTED]");
        assert_eq!(val["list"][0]["secret"], "[REDACTED]");
        assert_eq!(val["list"][1]["name"], "bob");
    }

    #[test]
    fn test_env_secrets_masking() {
        let redactor = Redactor {
            env_secrets: vec![
                ("super_secret_env_token_123".into(), "TEST_API_TOKEN".into()),
                ("shorter_secret".into(), "MY_PASSWORD".into()),
            ],
            extra_keys: Vec::new(),
            custom_patterns: Vec::new(),
        };

        let t = "Use token: super_secret_env_token_123 to log in with shorter_secret now";
        assert_eq!(
            redactor.redact_string(t),
            "Use token: [REDACTED:ENV:TEST_API_TOKEN] to log in with [REDACTED:ENV:MY_PASSWORD] now"
        );
    }

    #[test]
    fn test_custom_patterns() {
        let re = Regex::new(r"CONFIDENTIAL-[0-9]{4}").unwrap();
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: Vec::new(),
            custom_patterns: vec![re],
        };

        let t = "Internal project id CONFIDENTIAL-9876 must not leak";
        assert_eq!(
            redactor.redact_string(t),
            "Internal project id [REDACTED] must not leak"
        );
    }
}
