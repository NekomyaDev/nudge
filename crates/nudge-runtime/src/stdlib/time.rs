//! std/time - Time and duration functions
//!
//! Provides time.now, time.sleep, and time.format.

use crate::vm::Value;
use chrono::{TimeZone, Utc};
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const METHODS: [&str; 3] = ["time.now", "time.sleep", "time.format"];

pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    for name in METHODS {
        functions.insert(name.to_string(), Value::Native(name));
    }
    functions
}

pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "time.now" => {
            if !args.is_empty() {
                return Err("time.now takes 0 arguments".to_string());
            }
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| format!("time.now error: {}", e))?;
            let secs = duration.as_secs_f64();
            Ok(Value::Float(secs))
        }

        "time.sleep" => {
            if args.len() != 1 {
                return Err("time.sleep requires 1 argument (ms)".to_string());
            }
            let ms = match &args[0] {
                Value::Int(v) => *v as f64,
                Value::Float(v) => *v,
                _ => return Err("time.sleep: argument must be a number (int or float)".to_string()),
            };
            if ms < 0.0 {
                return Err("time.sleep: duration cannot be negative".to_string());
            }
            let duration = Duration::from_secs_f64(ms / 1000.0);
            thread::sleep(duration);
            Ok(Value::None)
        }

        "time.format" => {
            if args.len() != 2 {
                return Err(
                    "time.format requires 2 arguments (timestamp, format string)".to_string(),
                );
            }
            let ts = match &args[0] {
                Value::Float(v) => *v,
                Value::Int(v) => *v as f64,
                _ => {
                    return Err(
                        "time.format: first argument (timestamp) must be a number".to_string()
                    )
                }
            };
            let fmt = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err("time.format: second argument (format) must be a string".to_string())
                }
            };

            let secs = ts.floor() as i64;
            let nsecs = ((ts - ts.floor()) * 1_000_000_000.0) as u32;
            let datetime = Utc
                .timestamp_opt(secs, nsecs)
                .single()
                .ok_or_else(|| format!("time.format: invalid timestamp {}", ts))?;

            let formatted = datetime.format(&fmt).to_string();
            Ok(Value::String(formatted))
        }

        _ => Err(format!("Unknown time function: {}", name)),
    }
}
