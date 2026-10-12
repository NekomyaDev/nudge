//! std/llm - LLM operations
//!
//! Provides llm.call for calling LLM models.

use crate::vm::Value;
use crate::provider;
use std::collections::HashMap;

/// Register all LLM functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("llm.call".to_string(), Value::Native("llm.call"));
    functions
}

/// Execute an LLM function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "llm.call" => {
            // Expose llm.call(prompt: string, model: string) as a builtin function in the stdlib returning string or map.
            // Let's support either 1 argument (prompt, default model) or 2 arguments (prompt, model).
            if args.is_empty() || args.len() > 2 {
                return Err("llm.call requires 1 or 2 arguments (prompt: string, [model: string])".to_string());
            }

            let prompt = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("llm.call: prompt must be a string".to_string()),
            };

            let model = if args.len() == 2 {
                match &args[1] {
                    Value::String(m) => m.clone(),
                    _ => return Err("llm.call: model must be a string".to_string()),
                }
            } else {
                std::env::var("NUDGE_DEFAULT_MODEL").unwrap_or_else(|_| "gpt-4o".to_string())
            };

            let req = provider::LlmRequest {
                prompt,
                model,
                schema: None,
                temperature: 0.7,
                budget_usd: None,
            };

            let provider = provider::get_active_provider();
            let resp = provider.call(&req)?;

            // Returning string or map. Returning a Map containing response details (text, model, tokens_in, tokens_out, cost_usd) or String?
            // "returning string or map" -> Let's return a Map with "text", "model", "tokens_in", "tokens_out", "cost_usd" or similar, or Value::String(resp.text)? Wait, "returning string or map" suggests we can return a Map (or either depending on usage, or a Map containing text / full response info, or even a Map / String). Let's construct a Map with all useful response fields, or if someone expects string or map, a Map with text/model/tokens is extremely robust! Wait, let's check what other functions return or what tests might expect. Let's check stdlib tests or usages.
            let mut map = HashMap::new();
            map.insert("text".to_string(), Value::String(resp.text.clone()));
            map.insert("model".to_string(), Value::String(resp.model.clone()));
            map.insert("tokens_in".to_string(), Value::Int(resp.tokens_in as i64));
            map.insert("tokens_out".to_string(), Value::Int(resp.tokens_out as i64));
            map.insert("cost_usd".to_string(), Value::Float(resp.cost_usd));
            Ok(Value::Map(map))
        }
        _ => Err(format!("Unknown llm function: {}", name)),
    }
}
