pub mod fake;

pub use fake::FakeProvider;

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
        _ => {
            Box::new(FakeProvider::new())
        }
    }
}
