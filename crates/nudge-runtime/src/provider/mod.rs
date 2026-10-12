pub mod fake;
pub mod openai;
pub mod claude;
pub mod antigravity;

pub use fake::FakeProvider;
pub use openai::OpenAIProvider;
pub use claude::ClaudeProvider;
pub use antigravity::AntigravityProvider;

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub prompt: String,
    pub model: String,
    pub schema: Option<String>,
    pub temperature: f64,
    pub budget_usd: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
    pub model: String,
}

pub trait Provider: Send + Sync {
    fn call(&self, req: &LlmRequest) -> Result<LlmResponse, String>;
}

pub fn get_active_provider() -> Box<dyn Provider> {
    let provider_name = std::env::var("NUDGE_PROVIDER").unwrap_or_else(|_| "fake".to_string());
    match provider_name.as_str() {
        "fake" => Box::new(FakeProvider::new()),
        "openai" => {
            match OpenAIProvider::new() {
                Ok(p) => Box::new(p),
                Err(e) => {
                    eprintln!("Warning: Failed to initialize OpenAIProvider: {}. Falling back to FakeProvider.", e);
                    Box::new(FakeProvider::new())
                }
            }
        }
        "claude" => {
            match ClaudeProvider::new() {
                Ok(p) => Box::new(p),
                Err(e) => {
                    eprintln!("Warning: Failed to initialize ClaudeProvider: {}. Falling back to FakeProvider.", e);
                    Box::new(FakeProvider::new())
                }
            }
        }
        "antigravity" => Box::new(AntigravityProvider::new()),
        _ => {
            Box::new(FakeProvider::new())
        }
    }
}

#[cfg(test)]
mod tests;
