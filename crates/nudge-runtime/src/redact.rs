use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::sync::{LazyLock, RwLock};

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
    if upper.starts_with("NUDGE_")
        && !upper.contains("KEY")
        && !upper.contains("SECRET")
        && !upper.contains("TOKEN")
        && !upper.contains("AUTH")
    {
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
#[derive(Clone, Debug)]
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
        // Hold ENV_LOCK across all env reads here (load_env_secrets iterates
        // env::vars). The test-side set_var/remove_var also take this lock, so
        // env mutation and env iteration are never concurrent (UB otherwise).
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

static CACHED_REDACTOR: LazyLock<RwLock<Option<(u64, Redactor)>>> =
    LazyLock::new(|| RwLock::new(None));

/// Serializes process-env access in the redactor. `std::env::set_var` and
/// `std::env::vars` must not run concurrently (UB), so every env read here and
/// every env mutation in the tests share this lock.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A lightweight fingerprint of the secret-bearing environment variables (the
/// names and values that look like credentials). Used to auto-refresh the cached
/// Redactor when a secret appears or changes in the env WITHOUT rescanning
/// regexes on every record; this is a cheap env scan and hash, far cheaper than
/// `Redactor::from_env()`. Serialized under ENV_LOCK.
fn secret_env_generation() -> u64 {
    use std::hash::{Hash, Hasher};
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut pairs: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            let u = k.to_ascii_uppercase();
            u.contains("KEY")
                || u.contains("SECRET")
                || u.contains("TOKEN")
                || u.contains("AUTH")
                || u.contains("API")
                || u.contains("PASS")
                || u.contains("CRED")
        })
        .collect();
    pairs.sort();
    for (k, v) in pairs {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    h.finish()
}

/// Reset or invalidate the global redactor cache (useful when environment variables change, e.g. in tests).
pub fn reset_redactor_cache() {
    if let Ok(mut lock) = CACHED_REDACTOR.write() {
        *lock = None;
    }
}

/// Execute a closure with a borrowed reference to the process-wide cached Redactor.
///
/// The Redactor is cached for performance (regexes are compiled once, not per
/// record), but it AUTO-REFRESHES when the secret-bearing environment changes: a
/// lightweight env fingerprint is compared before reuse, so a secret added after
/// the first record is still redacted (no explicit reset needed). Env access is
/// serialized under ENV_LOCK (safe against concurrent `set_var`).
pub fn with_cached_redactor<R, F: FnOnce(&Redactor) -> R>(f: F) -> R {
    let gen = secret_env_generation();
    {
        let lock = CACHED_REDACTOR.read().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_gen, ref r)) = *lock {
            if cached_gen == gen {
                return f(r);
            }
        }
    }
    let mut lock = CACHED_REDACTOR.write().unwrap_or_else(|e| e.into_inner());
    let stale = match *lock {
        Some((g, _)) => g != gen,
        None => true,
    };
    if stale {
        *lock = Some((gen, Redactor::from_env()));
    }
    if let Some((_, ref r)) = *lock {
        f(r)
    } else {
        let fallback = Redactor::from_env();
        f(&fallback)
    }
}

/// Redact fields of a trace record in place, preserving request_hash, v, seq, and kind.
pub fn redact_record_fields(fields: &mut serde_json::Map<String, Value>) {
    if is_redact_disabled() {
        return;
    }
    with_cached_redactor(|redactor| {
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
    });
}

/// Sanitizes raw text by stripping terminal ANSI escape sequences (Issue #259).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1B' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next_c) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&next_c) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_sequences() {
        let dirty = "\x1B[31mRed Alert\x1B[0m: \x1B[1;32mAll Systems Go\x1B[0m";
        assert_eq!(strip_ansi(dirty), "Red Alert: All Systems Go");
        assert_eq!(strip_ansi("Clean Text"), "Clean Text");
    }

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

    #[test]
    fn test_redactor_auto_refresh_picks_up_late_secret() {
        use serde_json::json;
        // Warm the cache with an unrelated env secret so a Redactor is built.
        {
            let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            std::env::set_var("NUDGE_EARLY_KEY", "earlykey1234567890");
        }
        reset_redactor_cache();
        let mut warm = serde_json::Map::new();
        warm.insert("output".into(), json!("warm earlykey1234567890"));
        redact_record_fields(&mut warm);
        assert!(warm["output"].as_str().unwrap().contains("REDACTED"));

        // Add a NEW secret after the cache is warm. Auto-refresh picks it up
        // immediately (no explicit reset, no throttle delay). Use a NON-format
        // value so only the env-secret detector (not the sk-/Bearer regexes) can
        // catch it — this exercises the auto-refresh path specifically.
        {
            let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            std::env::set_var("NUDGE_LATE_KEY", "latekey1234567890");
        }
        let mut late = serde_json::Map::new();
        late.insert("output".into(), json!("late latekey1234567890"));
        redact_record_fields(&mut late);
        let out = late["output"].as_str().unwrap();
        assert!(
            !out.contains("latekey1234567890"),
            "auto-refresh should redact a secret added after the cache warmed: {out}"
        );

        // cleanup
        {
            let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            std::env::remove_var("NUDGE_EARLY_KEY");
            std::env::remove_var("NUDGE_LATE_KEY");
        }
        reset_redactor_cache();
    }

    #[test]
    fn test_nudge_api_key_env_var_is_redacted_in_trace_record() {
        use serde_json::json;
        let key_name = "NUDGE_API_KEY";
        let secret_val = "sk-secret-test-value-12345";
        // Serialize env mutation against the redactor's env iteration (UB if
        // concurrent). The lock is released before redact_record_fields, which
        // takes it internally — no self-deadlock.
        {
            let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            std::env::set_var(key_name, secret_val);
        }
        reset_redactor_cache();

        let mut record = serde_json::Map::new();
        record.insert("v".into(), json!(1));
        record.insert("seq".into(), json!(1));
        record.insert("kind".into(), json!("tool.call"));
        record.insert("request_hash".into(), json!("sha256:dummy"));
        record.insert(
            "output".into(),
            json!(format!("Response using {secret_val} successfully")),
        );

        redact_record_fields(&mut record);

        {
            let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            std::env::remove_var(key_name);
        }
        reset_redactor_cache();

        let out = record.get("output").and_then(Value::as_str).unwrap();
        assert!(
            !out.contains(secret_val),
            "trace output should not contain raw secret value: {out}"
        );
        assert!(
            out.contains("[REDACTED:ENV:NUDGE_API_KEY]"),
            "expected [REDACTED:ENV:NUDGE_API_KEY] in output: {out}"
        );
    }

    #[test]
    fn test_redactor_cache_avoids_recompilation() {
        use serde_json::json;
        reset_redactor_cache();
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let mut record = serde_json::Map::new();
            record.insert("kind".into(), json!("llm.call"));
            record.insert("output".into(), json!("normal text without secrets"));
            redact_record_fields(&mut record);
        }
        let elapsed = start.elapsed();
        // 1000 cached record redactions should complete within a few milliseconds
        assert!(
            elapsed.as_millis() < 500,
            "1000 cached trace redactions took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_large_log_redaction_throughput() {
        let redactor = Redactor {
            env_secrets: Vec::new(),
            extra_keys: Vec::new(),
            custom_patterns: Vec::new(),
        };
        // Generate ~1MB log text with occasional tokens
        let chunk = "2026-10-28T12:00:00Z [INFO] request processing Authorization: Bearer a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6 user action logged\n";
        let large_log = chunk.repeat(10_000); // ~1.1MB
        let start = std::time::Instant::now();
        let redacted = redactor.redact_string(&large_log);
        let elapsed = start.elapsed();
        assert!(!redacted.contains("a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6"));
        assert!(redacted.contains("[REDACTED]"));
        // 1MB throughput test: lazy static regexes ensure fast completion
        assert!(
            elapsed.as_secs() < 3,
            "1MB log redaction took too long: {:?}",
            elapsed
        );
    }
}
