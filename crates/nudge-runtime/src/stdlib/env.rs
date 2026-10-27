//! std/env - Environment variable operations
//!
//! Provides env.get, env.set, and env.has for interacting with environment variables.

use crate::value::Value;
use std::collections::HashMap;
use std::env;

/// Register all environment variable functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    functions.insert("env.get".to_string(), Value::Native("env.get"));
    functions.insert("env.set".to_string(), Value::Native("env.set"));
    functions.insert("env.has".to_string(), Value::Native("env.has"));
    functions.insert("env.delete".to_string(), Value::Native("env.delete"));
    functions.insert("env.remove".to_string(), Value::Native("env.remove"));
    functions.insert("env.vars".to_string(), Value::Native("env.vars"));
    functions.insert("env.all".to_string(), Value::Native("env.all"));

    functions
}

/// Execute an environment variable function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "env.get" => {
            if args.len() != 1 {
                return Err("env.get requires 1 argument (key)".to_string());
            }
            match &args[0] {
                Value::String(key) => match env::var(key) {
                    Ok(val) => Ok(Value::String(val)),
                    Err(_) => Ok(Value::None),
                },
                _ => Err("env.get: key must be a string".to_string()),
            }
        }

        "env.set" => {
            if args.len() != 2 {
                return Err("env.set requires 2 arguments (key, value)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(key), Value::String(val)) => {
                    env::set_var(key, val);
                    Ok(Value::None)
                }
                _ => Err("env.set: key and value must be strings".to_string()),
            }
        }

        "env.has" => {
            if args.len() != 1 {
                return Err("env.has requires 1 argument (key)".to_string());
            }
            match &args[0] {
                Value::String(key) => {
                    let exists = env::var_os(key).is_some();
                    Ok(Value::Bool(exists))
                }
                _ => Err("env.has: key must be a string".to_string()),
            }
        }

        "env.delete" | "env.remove" => {
            if args.len() != 1 {
                return Err("env.delete requires 1 argument (key)".to_string());
            }
            match &args[0] {
                Value::String(key) => {
                    env::remove_var(key);
                    Ok(Value::None)
                }
                _ => Err("env.delete: key must be a string".to_string()),
            }
        }

        "env.vars" | "env.all" => {
            if !args.is_empty() {
                return Err("env.all takes no arguments".to_string());
            }
            let mut map = HashMap::new();
            for (k, v) in env::vars() {
                map.insert(k, Value::String(v));
            }
            Ok(Value::Map(map))
        }

        _ => Err(format!("Unknown environment function: {}", name)),
    }
}
