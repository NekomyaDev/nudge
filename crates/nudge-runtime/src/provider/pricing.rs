use std::collections::HashMap;

/// Official pricing in USD per 1,000,000 tokens
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
    pub cache_read_usd_per_1m: Option<f64>,
    pub cache_write_usd_per_1m: Option<f64>,
}

impl ModelPricing {
    pub const fn new(input: f64, output: f64) -> Self {
        Self {
            input_usd_per_1m: input,
            output_usd_per_1m: output,
            cache_read_usd_per_1m: None,
            cache_write_usd_per_1m: None,
        }
    }

    pub const fn with_cache(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Self {
        Self {
            input_usd_per_1m: input,
            output_usd_per_1m: output,
            cache_read_usd_per_1m: Some(cache_read),
            cache_write_usd_per_1m: Some(cache_write),
        }
    }

    pub const fn with_cached_read(input: f64, output: f64, cache_read: f64) -> Self {
        Self {
            input_usd_per_1m: input,
            output_usd_per_1m: output,
            cache_read_usd_per_1m: Some(cache_read),
            cache_write_usd_per_1m: None,
        }
    }
}

/// Official rate table
pub fn get_official_pricing() -> HashMap<&'static str, ModelPricing> {
    let mut m = HashMap::new();

    // ── OpenAI ──
    m.insert("gpt-4o", ModelPricing::with_cached_read(2.50, 10.00, 1.25));
    m.insert(
        "gpt-4o-2024-08-06",
        ModelPricing::with_cached_read(2.50, 10.00, 1.25),
    );
    m.insert(
        "gpt-4o-2024-11-20",
        ModelPricing::with_cached_read(2.50, 10.00, 1.25),
    );
    m.insert(
        "gpt-4o-mini",
        ModelPricing::with_cached_read(0.15, 0.60, 0.075),
    );
    m.insert(
        "gpt-4o-mini-2024-07-18",
        ModelPricing::with_cached_read(0.15, 0.60, 0.075),
    );
    m.insert("o1", ModelPricing::with_cached_read(15.00, 60.00, 7.50));
    m.insert(
        "o1-preview",
        ModelPricing::with_cached_read(15.00, 60.00, 7.50),
    );
    m.insert("o1-mini", ModelPricing::with_cached_read(1.10, 4.40, 0.55));
    m.insert("o3-mini", ModelPricing::with_cached_read(1.10, 4.40, 0.55));
    m.insert("gpt-4-turbo", ModelPricing::new(10.00, 30.00));
    m.insert("gpt-4", ModelPricing::new(30.00, 60.00));
    m.insert("gpt-3.5-turbo", ModelPricing::new(0.50, 1.50));

    // ── Anthropic Claude ──
    m.insert(
        "claude-3-5-sonnet",
        ModelPricing::with_cache(3.00, 15.00, 0.30, 3.75),
    );
    m.insert(
        "claude-3-5-sonnet-20241022",
        ModelPricing::with_cache(3.00, 15.00, 0.30, 3.75),
    );
    m.insert(
        "claude-3-5-haiku",
        ModelPricing::with_cache(0.80, 4.00, 0.08, 1.00),
    );
    m.insert(
        "claude-3-5-haiku-20241022",
        ModelPricing::with_cache(0.80, 4.00, 0.08, 1.00),
    );
    m.insert(
        "claude-3-opus",
        ModelPricing::with_cache(15.00, 75.00, 1.50, 18.75),
    );
    m.insert(
        "claude-3-opus-20240229",
        ModelPricing::with_cache(15.00, 75.00, 1.50, 18.75),
    );
    m.insert("claude-3-sonnet", ModelPricing::new(3.00, 15.00));
    m.insert("claude-3-sonnet-20240229", ModelPricing::new(3.00, 15.00));
    m.insert("claude-3-haiku", ModelPricing::new(0.25, 1.25));
    m.insert("claude-3-haiku-20240307", ModelPricing::new(0.25, 1.25));
    m.insert(
        "claude-sonnet-4-5",
        ModelPricing::with_cache(3.00, 15.00, 0.30, 3.75),
    );
    m.insert(
        "claude-haiku-4-5",
        ModelPricing::with_cache(1.00, 5.00, 0.10, 1.25),
    );
    m.insert(
        "claude-opus-4-6-thinking",
        ModelPricing::with_cache(15.00, 75.00, 1.50, 18.75),
    );
    m.insert(
        "claude-sonnet-4-6",
        ModelPricing::with_cache(3.00, 15.00, 0.30, 3.75),
    );

    // ── Google Gemini ──
    m.insert("gemini-2.5-flash", ModelPricing::new(0.30, 2.50));
    m.insert("gemini-2.0-flash", ModelPricing::new(0.10, 0.40));
    m.insert("gemini-2.0-flash-exp", ModelPricing::new(0.10, 0.40));
    m.insert(
        "gemini-1.5-flash",
        ModelPricing::with_cached_read(0.075, 0.30, 0.01875),
    );
    m.insert(
        "gemini-1.5-pro",
        ModelPricing::with_cached_read(1.25, 5.00, 0.3125),
    );
    m.insert("gemini-3-flash", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3-flash-agent", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3.1-pro", ModelPricing::new(1.25, 5.00));
    m.insert("gemini-3.1-pro-high", ModelPricing::new(1.25, 5.00));
    m.insert("gemini-3.1-pro-low", ModelPricing::new(1.25, 5.00));
    m.insert("gemini-3.5-flash-low", ModelPricing::new(0.10, 0.40));
    m.insert("gemini-3.5-flash-lite", ModelPricing::new(0.10, 0.40));
    m.insert("gemini-3.6-flash-high", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3.6-flash-medium", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3.6-flash-low", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3.8-flash-high", ModelPricing::new(0.15, 0.60));
    m.insert("gemini-3.8-flash-tiered", ModelPricing::new(0.15, 0.60));

    // ── DeepSeek ──
    m.insert(
        "deepseek-chat",
        ModelPricing::with_cached_read(0.14, 0.28, 0.014),
    );
    m.insert(
        "deepseek-v3",
        ModelPricing::with_cached_read(0.14, 0.28, 0.014),
    );
    m.insert(
        "deepseek-reasoner",
        ModelPricing::with_cached_read(0.55, 2.19, 0.14),
    );
    m.insert(
        "deepseek-r1",
        ModelPricing::with_cached_read(0.55, 2.19, 0.14),
    );

    // ── Meta LLaMA ──
    m.insert("llama-3.3-70b-versatile", ModelPricing::new(0.59, 0.79));
    m.insert("llama-3.3-70b", ModelPricing::new(0.59, 0.79));
    m.insert("llama-3.1-8b-instant", ModelPricing::new(0.05, 0.08));
    m.insert("llama-3.1-8b", ModelPricing::new(0.05, 0.08));
    m.insert("llama-3.1-70b", ModelPricing::new(0.59, 0.79));
    m.insert("llama-3.1-405b", ModelPricing::new(2.00, 2.00));

    // ── Mistral ──
    m.insert("mistral-small-latest", ModelPricing::new(0.10, 0.30));
    m.insert("mistral-small", ModelPricing::new(0.10, 0.30));
    m.insert("mistral-large-latest", ModelPricing::new(2.00, 6.00));
    m.insert("mistral-large", ModelPricing::new(2.00, 6.00));
    m.insert("codestral-latest", ModelPricing::new(0.30, 0.90));
    m.insert("codestral", ModelPricing::new(0.30, 0.90));

    // ── Xiaomi / Qwen ──
    m.insert("mimo", ModelPricing::new(0.10, 0.20));
    m.insert("qwen-2.5-72b", ModelPricing::new(0.35, 0.40));
    m.insert("qwen-2.5-coder-32b", ModelPricing::new(0.20, 0.20));

    m
}

/// Strip prefixes like `openai:`, `anthropic:`, `gemini:`, `openai/`, etc.
pub fn normalize_model_name(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let stripped = if let Some(idx) = lower.find(':') {
        &lower[idx + 1..]
    } else if let Some(idx) = lower.find('/') {
        &lower[idx + 1..]
    } else {
        &lower
    };

    stripped.trim().to_string()
}

/// Retrieve pricing for a model from custom env var or official registry
pub fn get_model_pricing(model_name: &str) -> Option<ModelPricing> {
    let normalized = normalize_model_name(model_name);

    // 1. Check NUDGE_PRICING env override
    if let Ok(pricing_json) = std::env::var("NUDGE_PRICING") {
        if let Ok(val) = crate::json::parse(&pricing_json) {
            if let Some(entry) = val.get(&normalized) {
                if let (Some(inp), Some(outp)) = (
                    entry.idx(0).and_then(|v| v.as_num()),
                    entry.idx(1).and_then(|v| v.as_num()),
                ) {
                    return Some(ModelPricing::new(inp, outp));
                }
                if let (Some(inp), Some(outp)) = (
                    entry.get("in").and_then(|v| v.as_num()),
                    entry.get("out").and_then(|v| v.as_num()),
                ) {
                    return Some(ModelPricing::new(inp, outp));
                }
            }
        }
    }

    // 2. Exact match in official pricing
    let official = get_official_pricing();
    if let Some(p) = official.get(normalized.as_str()) {
        return Some(p.clone());
    }

    // 3. Fallback matching without version date tags (e.g. gpt-4o-2024-05-13 -> gpt-4o)
    for (key, p) in &official {
        if normalized.starts_with(key) {
            return Some(p.clone());
        }
    }

    None
}

/// Calculate cost in USD using exact token counts and official model rates.
/// Rate is in USD per 1M tokens: (tokens * rate) / 1,000,000.
pub fn calculate_cost(
    model: &str,
    tokens_in: u64,
    tokens_out: u64,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
) -> f64 {
    let Some(pricing) = get_model_pricing(model) else {
        return 0.0;
    };

    let mut total = 0.0;

    // Output tokens cost
    total += (tokens_out as f64 * pricing.output_usd_per_1m) / 1_000_000.0;

    // Cache read tokens discount if applicable
    let c_read = cache_read.unwrap_or(0);
    let c_write = cache_write.unwrap_or(0);

    if c_read > 0 {
        let read_rate = pricing
            .cache_read_usd_per_1m
            .unwrap_or(pricing.input_usd_per_1m * 0.5);
        total += (c_read as f64 * read_rate) / 1_000_000.0;

        // Remaining un-cached input tokens
        let uncached_in = tokens_in.saturating_sub(c_read);
        total += (uncached_in as f64 * pricing.input_usd_per_1m) / 1_000_000.0;
    } else {
        total += (tokens_in as f64 * pricing.input_usd_per_1m) / 1_000_000.0;
    }

    if c_write > 0 {
        let write_rate = pricing
            .cache_write_usd_per_1m
            .unwrap_or(pricing.input_usd_per_1m * 1.25);
        total += (c_write as f64 * write_rate) / 1_000_000.0;
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_2_5_flash_pricing() {
        let cost = calculate_cost("gemini-2.5-flash", 12, 7, None, None);
        // (12 * 0.30 + 7 * 2.50) / 1M = (3.6 + 17.5) / 1M = 21.1 / 1M = 2.11e-5
        let expected = 2.11e-5;
        assert!(
            (cost - expected).abs() < 1e-9,
            "expected {expected}, got {cost}"
        );
    }

    #[test]
    fn test_gpt_4o_pricing() {
        let cost = calculate_cost("openai:gpt-4o", 1000, 500, None, None);
        // (1000 * 2.50 + 500 * 10.00) / 1M = (2500 + 5000) / 1M = 0.0075
        assert!((cost - 0.0075).abs() < 1e-9);
    }

    #[test]
    fn test_claude_cache_pricing() {
        let cost = calculate_cost("claude-3-5-sonnet", 1000, 200, Some(800), None);
        // cache read: 800 * 0.30 / 1M = 240 / 1M = 0.00024
        // uncached input: 200 * 3.00 / 1M = 600 / 1M = 0.0006
        // output: 200 * 15.00 / 1M = 3000 / 1M = 0.003
        // total: 0.00024 + 0.0006 + 0.003 = 0.00384
        assert!((cost - 0.00384).abs() < 1e-9);
    }
}
