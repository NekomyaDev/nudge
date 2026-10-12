use super::{pricing, tokenizer, LlmRequest, LlmResponse, ModelInfo, Provider};

#[derive(Debug, Clone, Default)]
pub struct FakeProvider;

impl FakeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for FakeProvider {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String> {
        let text = if let Some(schema) = &req.schema {
            if schema.contains("Smoke") || schema.contains("title") {
                r#"{"title": "smoke test", "confidence": 0.99}"#.to_string()
            } else {
                format!(
                    r#"{{"status": "success", "prompt": "{}", "schema_applied": "{}"}}"#,
                    req.prompt, schema
                )
            }
        } else {
            format!("fake: {}", req.prompt)
        };

        let tokens_in = tokenizer::count_tokens(&req.prompt).max(1);
        let tokens_out = tokenizer::count_tokens(&text).max(1);
        let cost_usd = pricing::calculate_cost(&req.model, tokens_in, tokens_out, None, None);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd,
            model: req.model.clone(),
            cache_read: None,
            cache_write: None,
        })
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![
            ModelInfo {
                id: "fake-model".to_string(),
                name: "Fake Deterministic Model".to_string(),
                provider: "fake".to_string(),
                input_price_per_1m: Some(0.0),
                output_price_per_1m: Some(0.0),
                context_window: Some(128_000),
            },
            ModelInfo {
                id: "fake-fast".to_string(),
                name: "Fake Fast Model".to_string(),
                provider: "fake".to_string(),
                input_price_per_1m: Some(0.0),
                output_price_per_1m: Some(0.0),
                context_window: Some(32_000),
            },
        ])
    }
}
