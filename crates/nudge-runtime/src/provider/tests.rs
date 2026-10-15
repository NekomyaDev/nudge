use super::*;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let mut saved = Vec::new();
        for &(k, v) in vars {
            saved.push((k, std::env::var(k).ok()));
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[test]
fn test_fake_provider_deterministic_response() {
    let provider = FakeProvider::new();

    // Test without schema
    let req = LlmRequest {
        prompt: "Hello, Nudge!".to_string(),
        model: "fake-model".to_string(),
        schema: None,
        temperature: 0.7,
        budget_usd: None,
    };

    let resp = provider
        .call(&req)
        .expect("FakeProvider call should succeed");
    assert_eq!(resp.model, "fake-model");
    assert!(resp.text.contains("Hello, Nudge!"));
    assert!(resp.tokens_in > 0);
    assert!(resp.tokens_out > 0);
    assert_eq!(resp.cost_usd, 0.0);

    // Test with schema
    let req_schema = LlmRequest {
        prompt: "Classify this".to_string(),
        model: "fake-model".to_string(),
        schema: Some("{\"type\": \"object\"}".to_string()),
        temperature: 0.0,
        budget_usd: Some(0.01),
    };

    let resp_schema = provider
        .call(&req_schema)
        .expect("FakeProvider call with schema should succeed");
    assert!(resp_schema.text.contains("schema_applied"));
    let parsed: serde_json::Value =
        serde_json::from_str(&resp_schema.text).expect("must be valid JSON");
    assert_eq!(parsed["schema_applied"], "{\"type\": \"object\"}");
}

#[test]
fn test_fake_provider_json_escape_with_quotes_and_braces() {
    let provider = FakeProvider::new();
    let schema_with_quotes =
        r#"{"type": "array", "description": "some \"nested\" quotes and {braces}"}"#;
    let req = LlmRequest {
        prompt: "Extract data with complex schema".to_string(),
        model: "fake-model".to_string(),
        schema: Some(schema_with_quotes.to_string()),
        temperature: 0.0,
        budget_usd: None,
    };
    let resp = provider
        .call(&req)
        .expect("FakeProvider call should succeed");
    // The response must be valid JSON parseable by serde_json
    let parsed: serde_json::Value = serde_json::from_str(&resp.text)
        .expect("response with quotes in schema must be valid parseable JSON");
    assert_eq!(parsed["status"], "success");
    assert_eq!(parsed["schema_applied"], schema_with_quotes);
}

#[test]
fn test_fake_provider_canned_responses_for_research_agent() {
    let provider = FakeProvider::new();

    // [Finding]
    let req_finding = LlmRequest {
        prompt: "Extract findings".to_string(),
        model: "fake-model".to_string(),
        schema: Some("[Finding]".to_string()),
        temperature: 0.0,
        budget_usd: None,
    };
    let resp = provider.call(&req_finding).expect("call succeeds");
    let val: serde_json::Value = serde_json::from_str(&resp.text).expect("valid json array");
    assert!(val.is_array());
    assert!(val[0]["claim"].is_string());
    assert!(val[0]["source"].is_string());
    assert!(val[0]["confidence"].is_number());

    // Report
    let req_report = LlmRequest {
        prompt: "Merge findings".to_string(),
        model: "fake-model".to_string(),
        schema: Some("Report".to_string()),
        temperature: 0.0,
        budget_usd: None,
    };
    let resp = provider.call(&req_report).expect("call succeeds");
    let val: serde_json::Value = serde_json::from_str(&resp.text).expect("valid json object");
    assert!(val.is_object());
    assert!(val["title"].is_string());
    assert!(val["findings"].is_array());
    assert!(val["gaps"].is_array());

    // [string]
    let req_strings = LlmRequest {
        prompt: "3 search angles".to_string(),
        model: "fake-model".to_string(),
        schema: Some("[string]".to_string()),
        temperature: 0.0,
        budget_usd: None,
    };
    let resp = provider.call(&req_strings).expect("call succeeds");
    let val: serde_json::Value = serde_json::from_str(&resp.text).expect("valid json array");
    assert_eq!(val.as_array().unwrap().len(), 3);
}

#[test]
fn test_get_active_provider_default() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set(&[("NUDGE_PROVIDER", Some("fake"))]);
    let provider = get_active_provider();
    let req = LlmRequest {
        prompt: "test".to_string(),
        model: "default".to_string(),
        schema: None,
        temperature: 0.5,
        budget_usd: None,
    };
    let resp = provider
        .call(&req)
        .expect("active provider call should succeed");
    assert!(resp.text.contains("test"));
}

#[test]
fn test_vm_llm_call_execution() {
    let provider = FakeProvider::new();
    let req = LlmRequest {
        prompt: "VM LLM Call test".to_string(),
        model: "gpt-4o".to_string(),
        schema: None,
        temperature: 0.2,
        budget_usd: None,
    };
    let res = provider.call(&req).unwrap();
    assert!(res.text.contains("VM LLM Call test"));
    // Real pricing for gpt-4o:
    assert!(res.cost_usd > 0.0);
}

#[test]
fn test_pricing_and_token_counter() {
    let text = "The quick brown fox jumps over the lazy dog. 12345.";
    let tokens = tokenizer::count_tokens(text);
    assert!((10..=20).contains(&tokens), "got {tokens}");

    // gemini-2.5-flash: $0.30/1M in, $2.50/1M out
    let cost = pricing::calculate_cost("gemini-2.5-flash", 1000, 1000, None, None);
    // (1000 * 0.30 + 1000 * 2.50) / 1M = 2.80 / 1000 = 0.0028
    assert!((cost - 0.0028).abs() < 1e-9);

    // gpt-4o: $2.50/1M in, $10.00/1M out
    let cost_gpt4o = pricing::calculate_cost("gpt-4o", 1000, 1000, None, None);
    // (1000 * 2.50 + 1000 * 10.00) / 1M = 12.50 / 1000 = 0.0125
    assert!((cost_gpt4o - 0.0125).abs() < 1e-9);
}

#[test]
fn test_provider_list_models() {
    let fake = FakeProvider::new();
    let models = fake.list_models().unwrap();
    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.id == "fake-model"));

    let antigravity = AntigravityProvider::new();
    let ag_models = antigravity.list_models().unwrap();
    assert!(!ag_models.is_empty());
    assert!(ag_models.iter().any(|m| m.id == "gemini-3-flash"));
    assert!(ag_models.iter().any(|m| m.id == "claude-sonnet-4-6"));
}

#[test]
fn test_stdlib_llm_builtins() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set(&[("NUDGE_PROVIDER", Some("fake"))]);
    use crate::stdlib::llm::execute;
    use crate::value::Value;

    // Test llm.tokens
    let tok_val = execute(
        "llm.tokens",
        vec![Value::String("hello world from nudge".to_string())],
    )
    .unwrap();
    if let Value::Int(n) = tok_val {
        assert!(n >= 4);
    } else {
        panic!("expected Value::Int");
    }

    // Test llm.cost
    let cost_val = execute(
        "llm.cost",
        vec![
            Value::String("gpt-4o".to_string()),
            Value::Int(1_000_000),
            Value::Int(1_000_000),
        ],
    )
    .unwrap();
    if let Value::Float(f) = cost_val {
        // 2.50 + 10.00 = 12.50 USD
        assert!((f - 12.50).abs() < 1e-6);
    } else {
        panic!("expected Value::Float");
    }

    // Test llm.models
    let models_val = execute("llm.models", vec![]).unwrap();
    if let Value::List(list) = models_val {
        assert!(!list.is_empty());
    } else {
        panic!("expected Value::List");
    }
}

#[test]
fn test_live_provider_missing_credentials_fails_loudly_no_mock() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set(&[("NUDGE_PROVIDER", Some("openai")), ("OPENAI_API_KEY", None)]);

    // 1. try_get_active_provider fails closed
    let res = try_get_active_provider();
    assert!(res.is_err(), "expected error for missing credentials");
    let err = res.err().unwrap();
    assert!(
        err.contains("error[E-auth-0401]"),
        "expected E-auth-0401, got: {err}"
    );

    // 2. get_active_provider returns FailClosedProvider; .call() fails loudly (no mock)
    let provider = get_active_provider();
    let req = LlmRequest {
        prompt: "test prompt".to_string(),
        model: "gpt-4o".to_string(),
        schema: None,
        temperature: 0.0,
        budget_usd: None,
    };
    let call_res = provider.call(&req);
    assert!(call_res.is_err(), "must not silently fall back to mock");
    let call_err = call_res.err().unwrap();
    assert!(
        call_err.contains("error[E-auth-0401]"),
        "expected E-auth-0401 on call, got: {call_err}"
    );

    // 3. Test empty key
    let _guard_empty = EnvGuard::set(&[
        ("NUDGE_PROVIDER", Some("openai")),
        ("OPENAI_API_KEY", Some("   ")),
    ]);
    let empty_res = try_get_active_provider();
    assert!(empty_res.is_err());
    assert!(empty_res.err().unwrap().contains("error[E-auth-0401]"));

    // 4. Test claude with missing key
    let _guard_claude = EnvGuard::set(&[
        ("NUDGE_PROVIDER", Some("claude")),
        ("ANTHROPIC_API_KEY", None),
    ]);
    let claude_res = try_get_active_provider();
    assert!(claude_res.is_err());
    let claude_err = claude_res.err().unwrap();
    assert!(
        claude_err.contains("error[E-auth-0401]"),
        "expected E-auth-0401 for claude, got: {claude_err}"
    );
    let claude_call = get_active_provider().call(&req);
    assert!(claude_call.is_err());
    assert!(claude_call.err().unwrap().contains("error[E-auth-0401]"));
}

#[test]
fn test_nudge_provider_fake_and_replay_still_work() {
    let _lock = ENV_LOCK.lock().unwrap();

    // 1. Explicit NUDGE_PROVIDER=fake
    let _guard = EnvGuard::set(&[("NUDGE_PROVIDER", Some("fake"))]);
    let provider = try_get_active_provider().expect("fake provider should initialize");
    let req = LlmRequest {
        prompt: "Hello world".to_string(),
        model: "fake-model".to_string(),
        schema: None,
        temperature: 0.0,
        budget_usd: None,
    };
    let resp = provider
        .call(&req)
        .expect("call to fake provider should succeed");
    assert_eq!(resp.model, "fake-model");
    assert!(resp.text.contains("Hello world"));

    // 2. Default when unset is fake
    let _guard_unset = EnvGuard::set(&[("NUDGE_PROVIDER", None)]);
    let default_provider = get_active_provider();
    let default_resp = default_provider
        .call(&req)
        .expect("default fake call should succeed");
    assert!(default_resp.text.contains("Hello world"));
}

#[test]
fn test_antigravity_no_longer_reads_foreign_tokens_and_fails_closed() {
    let _lock = ENV_LOCK.lock().unwrap();

    // Create a temporary directory mimicking foreign app credential directories
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_foreign_token_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let jcode_dir = temp_dir.join(".jcode");
    let openhands_dir = temp_dir.join(".openhands").join("profiles");
    std::fs::create_dir_all(&jcode_dir).unwrap();
    std::fs::create_dir_all(&openhands_dir).unwrap();

    let jcode_token_file = jcode_dir.join("antigravity_oauth.json");
    std::fs::write(
        &jcode_token_file,
        r#"{"access_token": "parasitic_stolen_jcode_token"}"#,
    )
    .unwrap();

    let openhands_profile_file = openhands_dir.join("antigravity-proxy.json");
    std::fs::write(
        &openhands_profile_file,
        r#"{"api_key": "parasitic_stolen_openhands_key", "base_url": "http://127.0.0.1:9999"}"#,
    )
    .unwrap();

    let temp_home = temp_dir.to_str().unwrap();
    let _guard = EnvGuard::set(&[
        ("HOME", Some(temp_home)),
        ("USERPROFILE", Some(temp_home)),
        ("NUDGE_PROVIDER", Some("antigravity")),
        ("NUDGE_API_KEY", None),
        ("ANTIGRAVITY_API_KEY", None),
    ]);

    // 1. try_get_active_provider fails closed with E-auth-0404
    let res = try_get_active_provider();
    assert!(
        res.is_err(),
        "antigravity must fail closed when foreign tokens exist"
    );
    let err = res.err().unwrap();
    assert!(
        err.contains("error[E-auth-0404]"),
        "expected E-auth-0404, got: {err}"
    );
    assert!(
        err.contains("disabled because no official public OAuth contract exists"),
        "expected disabled reason, got: {err}"
    );

    // 2. get_active_provider().call(&req) fails closed
    let req = LlmRequest {
        prompt: "test antigravity".to_string(),
        model: "default".to_string(),
        schema: None,
        temperature: 0.0,
        budget_usd: None,
    };
    let call_res = get_active_provider().call(&req);
    assert!(call_res.is_err());
    let call_err = call_res.err().unwrap();
    assert!(
        call_err.contains("error[E-auth-0404]"),
        "expected E-auth-0404 on call, got: {call_err}"
    );

    // 3. Direct AntigravityProvider::new().call(&req) also fails closed
    let direct_provider = AntigravityProvider::new();
    let direct_res = direct_provider.call(&req);
    assert!(direct_res.is_err());
    let direct_err = direct_res.err().unwrap();
    assert!(
        direct_err.contains("error[E-auth-0404]"),
        "direct antigravity call must fail closed, got: {direct_err}"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_provider_marked_disabled_refuses_live_call() {
    let _lock = ENV_LOCK.lock().unwrap();

    // 1. antigravity without credentials is typed Disabled in the gate
    let _guard_ag = EnvGuard::set(&[
        ("NUDGE_PROVIDER", Some("antigravity")),
        ("NUDGE_API_KEY", None),
        ("ANTIGRAVITY_API_KEY", None),
    ]);
    let req = LlmRequest {
        prompt: "disabled test".to_string(),
        model: "default".to_string(),
        schema: None,
        temperature: 0.0,
        budget_usd: None,
    };
    let res = get_active_provider().call(&req);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("error[E-auth-0404]"), "got: {err}");

    // 2. Explicitly disabling a provider via NUDGE_DISABLE_PROVIDER_<NAME>=1
    let _guard_disabled = EnvGuard::set(&[
        ("NUDGE_PROVIDER", Some("openai")),
        ("OPENAI_API_KEY", Some("valid-dummy-key")),
        ("NUDGE_DISABLE_PROVIDER_OPENAI", Some("1")),
    ]);
    let res_disabled = get_active_provider().call(&req);
    assert!(res_disabled.is_err());
    let err_disabled = res_disabled.err().unwrap();
    assert!(
        err_disabled.contains("error[E-auth-0404]") && err_disabled.contains("disabled"),
        "got: {err_disabled}"
    );

    // 3. Unknown provider name refuses live call with E-auth-0404
    let _guard_unknown = EnvGuard::set(&[("NUDGE_PROVIDER", Some("completely_unknown_xyz"))]);
    let res_unknown = get_active_provider().call(&req);
    assert!(res_unknown.is_err());
    let err_unknown = res_unknown.err().unwrap();
    assert!(
        err_unknown.contains("error[E-auth-0404]"),
        "unknown provider must fail with E-auth-0404, got: {err_unknown}"
    );
}

#[test]
fn test_provider_gate_table_and_policy() {
    let _lock = ENV_LOCK.lock().unwrap();

    // Check table entries
    assert!(PROVIDER_GATE_TABLE
        .iter()
        .any(|e| e.name == "fake" && e.policy == ProviderGatePolicy::Enabled));
    assert!(PROVIDER_GATE_TABLE
        .iter()
        .any(|e| e.name == "openai" && !e.oauth_enabled));
    assert!(PROVIDER_GATE_TABLE
        .iter()
        .any(|e| e.name == "claude" && !e.oauth_enabled));
    assert!(PROVIDER_GATE_TABLE
        .iter()
        .any(|e| e.name == "antigravity" && !e.oauth_enabled));

    // Test get_provider_gate_policy
    let _guard = EnvGuard::set(&[("NUDGE_API_KEY", None), ("ANTIGRAVITY_API_KEY", None)]);
    match get_provider_gate_policy("antigravity") {
        ProviderGatePolicy::Disabled { reason } => {
            assert!(reason.contains("no official public OAuth contract exists"));
        }
        other => panic!("expected Disabled, got: {other:?}"),
    }

    // When first-party key is set, antigravity switches to ApiKey policy
    let _guard_key = EnvGuard::set(&[("NUDGE_API_KEY", Some("my-first-party-key"))]);
    match get_provider_gate_policy("antigravity") {
        ProviderGatePolicy::ApiKey { primary_env, .. } => {
            assert_eq!(primary_env, "NUDGE_API_KEY");
        }
        other => panic!("expected ApiKey, got: {other:?}"),
    }
}
