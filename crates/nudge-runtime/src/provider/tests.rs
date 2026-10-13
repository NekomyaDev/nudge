use super::*;

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
    std::env::set_var("NUDGE_PROVIDER", "fake");
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
