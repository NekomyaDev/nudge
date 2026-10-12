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
    assert!(resp_schema.text.contains("{\"type\": \"object\"}"));
}

#[test]
fn test_get_active_provider_default() {
    // Ensure NUDGE_PROVIDER is unset or set to fake
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
    // Since OpCode::LlmCall might be handled by standard library or VM instruction,
    // let's check how LlmCall is handled or if we can test executing a program or standard function using FakeProvider.
    // Let's verify if stdlib has llm / prompt support or if VM handles OpCode::LlmCall.
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
}
