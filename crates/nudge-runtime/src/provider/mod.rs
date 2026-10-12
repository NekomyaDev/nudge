pub mod antigravity;
pub mod claude;
pub mod fake;
pub mod http;
pub mod openai;
pub mod pricing;
pub mod tokenizer;

pub use antigravity::AntigravityProvider;
pub use claude::ClaudeProvider;
pub use fake::FakeProvider;
pub use openai::OpenAIProvider;
pub use pricing::{calculate_cost, get_model_pricing, ModelPricing};
pub use tokenizer::count_tokens;

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub prompt: String,
    pub model: String,
    pub schema: Option<String>,
    pub temperature: f64,
    pub budget_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
    pub model: String,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub input_price_per_1m: Option<f64>,
    pub output_price_per_1m: Option<f64>,
    pub context_window: Option<u64>,
}

pub trait Provider: Send + Sync {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String>;
    fn list_models(&self) -> Result<Vec<ModelInfo>, String>;
}

pub fn get_active_provider() -> Box<dyn Provider> {
    let provider_name = std::env::var("NUDGE_PROVIDER").unwrap_or_else(|_| "fake".to_string());
    match provider_name.as_str() {
        "fake" => Box::new(FakeProvider::new()),
        "openai" => match OpenAIProvider::new() {
            Ok(p) => Box::new(p),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to initialize OpenAIProvider: {}. Falling back to FakeProvider.",
                    e
                );
                Box::new(FakeProvider::new())
            }
        },
        "claude" => match ClaudeProvider::new() {
            Ok(p) => Box::new(p),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to initialize ClaudeProvider: {}. Falling back to FakeProvider.",
                    e
                );
                Box::new(FakeProvider::new())
            }
        },
        "antigravity" => Box::new(AntigravityProvider::new()),
        _ => Box::new(FakeProvider::new()),
    }
}

#[cfg(test)]
mod tests;
