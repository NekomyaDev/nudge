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
            } else if schema.contains("Ticket") {
                r#"{"category": "billing", "priority": "high", "summary": "Charged twice for March invoice"}"#.to_string()
            } else if schema.contains("Translation") {
                r#"{"original": "Hello, how are you?", "translated": "Merhaba, nasılsınız?", "source_lang": "English", "target_lang": "Turkish", "quality": 0.95}"#.to_string()
            } else if schema.contains("Conversation") {
                r#"{"messages": [{"role": "user", "content": "Hello! What can you help me with?"}, {"role": "assistant", "content": "I can help you with Nudge"}], "summary": "Nudge discussion"}"#.to_string()
            } else if schema.contains("Analysis") {
                r#"{"dataset": "Annual revenue", "insights": [{"category": "revenue", "finding": "Revenue grew from 100k to 200k", "impact": "high", "confidence": 0.92}], "summary": "Strong quarterly growth", "recommendations": ["Expand sales team"]}"#.to_string()
            } else if schema.contains("Review") {
                r#"{"summary": "Code review findings", "issues": [{"severity": "critical", "line": 2, "message": "exec() allows arbitrary code execution", "suggestion": "Avoid exec"}], "score": 2.5}"#.to_string()
            } else if schema.contains("Answer") {
                r#"{"answer": "Nudge prevents silent cost overruns with compile-time budget enforcement.", "citations": ["design/spec.md"], "grounded": true, "missing": []}"#.to_string()
            } else {
                let safe_prompt =
                    serde_json::to_string(&req.prompt).unwrap_or_else(|_| "\"\"".to_string());
                format!(
                    r#"{{"status": "success", "prompt": {}, "schema_applied": "{}"}}"#,
                    safe_prompt, schema
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
