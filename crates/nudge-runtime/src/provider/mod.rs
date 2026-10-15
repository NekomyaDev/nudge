pub mod antigravity;
pub mod claude;
pub mod copilot;
pub mod fake;
pub mod gate;
pub mod http;
pub mod openai;
pub mod pricing;
pub mod stream;
pub mod tokenizer;

pub use antigravity::AntigravityProvider;
pub use claude::ClaudeProvider;
pub use copilot::CopilotProvider;
pub use fake::FakeProvider;
pub use gate::{
    get_provider_gate_policy, ProviderGateEntry, ProviderGatePolicy, PROVIDER_GATE_TABLE,
};
pub use openai::OpenAIProvider;
pub use pricing::{calculate_cost, get_model_pricing, ModelPricing};
pub use stream::{
    extract_openai_delta, is_done, is_done_sentinel, parse_openai_chunk, ChunkOutcome, SseEvent,
    SseReader,
};
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

/// A provider that fails closed with a designated error message.
/// Returned when a live provider's credentials are missing or the provider is disabled.
#[derive(Debug, Clone)]
pub struct FailClosedProvider {
    pub provider_name: String,
    pub error: String,
}

impl FailClosedProvider {
    pub fn new(provider_name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            provider_name: provider_name.into(),
            error: error.into(),
        }
    }
}

impl Provider for FailClosedProvider {
    fn call(&self, _req: &LlmRequest) -> Result<LlmResponse, String> {
        Err(self.error.clone())
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        Err(self.error.clone())
    }
}

/// Create an authorized provider instance by name according to the per-provider gate.
/// Fails closed if the adapter is disabled or required credentials are missing.
pub fn create_provider(provider_name: &str) -> Result<Box<dyn Provider>, String> {
    let policy = gate::get_provider_gate_policy(provider_name);
    match policy {
        gate::ProviderGatePolicy::Enabled => {
            if provider_name == "fake" {
                Ok(Box::new(FakeProvider::new()))
            } else {
                Err(format!(
                    "error[E-auth-0404]: provider adapter '{provider_name}' has no enabled implementation"
                ))
            }
        }
        gate::ProviderGatePolicy::ApiKey { .. } => match provider_name {
            "openai" => OpenAIProvider::new().map(|p| Box::new(p) as Box<dyn Provider>),
            "claude" => ClaudeProvider::new().map(|p| Box::new(p) as Box<dyn Provider>),
            "antigravity" => {
                AntigravityProvider::try_new().map(|p| Box::new(p) as Box<dyn Provider>)
            }
            other => Err(format!(
                "error[E-auth-0404]: provider adapter '{other}' has no live implementation"
            )),
        },
        gate::ProviderGatePolicy::DeviceFlow { .. } => match provider_name {
            "github_copilot" => CopilotProvider::new().map(|p| Box::new(p) as Box<dyn Provider>),
            other => Err(format!(
                "error[E-auth-0404]: provider adapter '{other}' has no live implementation"
            )),
        },
        gate::ProviderGatePolicy::Disabled { reason } => {
            Err(format!("error[E-auth-0404]: {reason}"))
        }
    }
}

/// Try resolving the active provider according to `NUDGE_PROVIDER` (defaults to "fake").
/// Returns Err if the requested provider fails authorization or is disabled.
pub fn try_get_active_provider() -> Result<Box<dyn Provider>, String> {
    let provider_name = std::env::var("NUDGE_PROVIDER").unwrap_or_else(|_| "fake".to_string());
    create_provider(&provider_name)
}

/// Returns the active provider box. If the provider's credentials are missing or
/// the adapter is disabled, returns a `FailClosedProvider` that fails loudly on `.call()`
/// and `.list_models()`. NEVER silently substitutes `FakeProvider`.
pub fn get_active_provider() -> Box<dyn Provider> {
    match try_get_active_provider() {
        Ok(provider) => provider,
        Err(err) => {
            let provider_name =
                std::env::var("NUDGE_PROVIDER").unwrap_or_else(|_| "fake".to_string());
            Box::new(FailClosedProvider::new(provider_name, err))
        }
    }
}

#[cfg(test)]
mod tests;
