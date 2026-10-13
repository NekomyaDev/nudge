//! std/llm - LLM operations
//!
//! Provides llm.call, llm.models, llm.cost, and llm.tokens.

use crate::provider;
use crate::value::Value;
use std::collections::HashMap;

/// Register all LLM functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("llm.call".to_string(), Value::Native("llm.call"));
    functions.insert("llm.models".to_string(), Value::Native("llm.models"));
    functions.insert("llm.cost".to_string(), Value::Native("llm.cost"));
    functions.insert("llm.tokens".to_string(), Value::Native("llm.tokens"));
    functions
}

/// Execute an LLM function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "llm.call" => {
            if args.is_empty() || args.len() > 2 {
                return Err(
                    "llm.call requires 1 or 2 arguments (prompt: string, [model: string])"
                        .to_string(),
                );
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
                std::env::var("NUDGE_DEFAULT_MODEL")
                    .unwrap_or_else(|_| "gemini-3-flash".to_string())
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

            let mut map = HashMap::new();
            map.insert("text".to_string(), Value::String(resp.text.clone()));
            map.insert("model".to_string(), Value::String(resp.model.clone()));
            map.insert("tokens_in".to_string(), Value::Int(resp.tokens_in as i64));
            map.insert("tokens_out".to_string(), Value::Int(resp.tokens_out as i64));
            map.insert("cost_usd".to_string(), Value::Float(resp.cost_usd));
            if let Some(cr) = resp.cache_read {
                map.insert("cache_read".to_string(), Value::Int(cr as i64));
            }
            if let Some(cw) = resp.cache_write {
                map.insert("cache_write".to_string(), Value::Int(cw as i64));
            }
            Ok(Value::Map(map))
        }
        "llm.models" => {
            let provider = provider::get_active_provider();
            let models = provider.list_models()?;
            let mut list = Vec::new();
            for m in models {
                let mut map = HashMap::new();
                map.insert("id".to_string(), Value::String(m.id));
                map.insert("name".to_string(), Value::String(m.name));
                map.insert("provider".to_string(), Value::String(m.provider));
                if let Some(inp) = m.input_price_per_1m {
                    map.insert("input_price_per_1m".to_string(), Value::Float(inp));
                }
                if let Some(outp) = m.output_price_per_1m {
                    map.insert("output_price_per_1m".to_string(), Value::Float(outp));
                }
                if let Some(cw) = m.context_window {
                    map.insert("context_window".to_string(), Value::Int(cw as i64));
                }
                list.push(Value::Map(map));
            }
            Ok(Value::List(list))
        }
        "llm.cost" => {
            if args.len() < 3 {
                return Err(
                    "llm.cost requires model: string, tokens_in: int, tokens_out: int".to_string(),
                );
            }
            let model = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("llm.cost: model must be a string".to_string()),
            };
            let tin = match &args[1] {
                Value::Int(n) => *n as u64,
                _ => return Err("llm.cost: tokens_in must be an integer".to_string()),
            };
            let tout = match &args[2] {
                Value::Int(n) => *n as u64,
                _ => return Err("llm.cost: tokens_out must be an integer".to_string()),
            };
            let cread = if args.len() > 3 {
                match &args[3] {
                    Value::Int(n) => Some(*n as u64),
                    _ => None,
                }
            } else {
                None
            };
            let cwrite = if args.len() > 4 {
                match &args[4] {
                    Value::Int(n) => Some(*n as u64),
                    _ => None,
                }
            } else {
                None
            };

            let cost = provider::pricing::calculate_cost(model, tin, tout, cread, cwrite);
            Ok(Value::Float(cost))
        }
        "llm.tokens" => {
            if args.is_empty() {
                return Err("llm.tokens requires text: string".to_string());
            }
            let text = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("llm.tokens: text must be a string".to_string()),
            };
            let count = provider::tokenizer::count_tokens(text);
            Ok(Value::Int(count as i64))
        }
        _ => Err(format!("Unknown llm function: {}", name)),
    }
}
