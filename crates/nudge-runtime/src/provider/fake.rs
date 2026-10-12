use super::{LlmRequest, LlmResponse, Provider};

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
            format!(
                r#"{{"status": "success", "prompt": "{}", "schema_applied": "{}"}}"#,
                req.prompt, schema
            )
        } else {
            format!(
                r#"{{"status": "success", "response": "Mock response for prompt: {}"}}"#,
                req.prompt
            )
        };

        let tokens_in = (req.prompt.chars().count() as u64 / 4).max(1);
        let tokens_out = (text.chars().count() as u64 / 4).max(1);

        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            cost_usd: 0.0,
            model: req.model.clone(),
        })
    }
}
