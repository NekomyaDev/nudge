//! std/os - OS process operations
//!
//! Provides process-related functions such as command line arguments, current working directory, and process ID.

use crate::value::Value;
use std::collections::HashMap;

/// Register all os functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("os.args".to_string(), Value::Native("os.args"));
    functions.insert("os.cwd".to_string(), Value::Native("os.cwd"));
    functions.insert("os.pid".to_string(), Value::Native("os.pid"));
    functions.insert("os.platform".to_string(), Value::Native("os.platform"));
    functions.insert("os.arch".to_string(), Value::Native("os.arch"));
    functions.insert("os.family".to_string(), Value::Native("os.family"));
    functions
}

/// Execute an os function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "os.args" => {
            if !args.is_empty() {
                return Err("os.args takes no arguments".to_string());
            }
            let args_vec: Vec<Value> = std::env::args().map(Value::String).collect();
            Ok(Value::List(args_vec))
        }

        "os.cwd" => {
            if !args.is_empty() {
                return Err("os.cwd takes no arguments".to_string());
            }
            match std::env::current_dir() {
                Ok(path) => match path.to_str() {
                    Some(s) => Ok(Value::String(s.to_string())),
                    None => Err("os.cwd: current directory path is not valid UTF-8".to_string()),
                },
                Err(e) => Err(format!(
                    "os.cwd: cannot get current working directory: {}",
                    e
                )),
            }
        }

        "os.pid" => {
            if !args.is_empty() {
                return Err("os.pid takes no arguments".to_string());
            }
            let pid = std::process::id() as i64;
            Ok(Value::Int(pid))
        }

        "os.platform" => {
            if !args.is_empty() {
                return Err("os.platform takes no arguments".to_string());
            }
            Ok(Value::String(std::env::consts::OS.to_string()))
        }

        "os.arch" => {
            if !args.is_empty() {
                return Err("os.arch takes no arguments".to_string());
            }
            Ok(Value::String(std::env::consts::ARCH.to_string()))
        }

        "os.family" => {
            if !args.is_empty() {
                return Err("os.family takes no arguments".to_string());
            }
            Ok(Value::String(std::env::consts::FAMILY.to_string()))
        }

        _ => Err(format!("Unknown os function: {}", name)),
    }
}
