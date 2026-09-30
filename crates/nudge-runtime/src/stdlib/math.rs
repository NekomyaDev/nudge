//! std/math - Mathematical operations
//!
//! Provides basic math, trigonometry, statistics, and random numbers.

use crate::vm::Value;
use std::collections::HashMap;

/// Register all math functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    // Basic math
    functions.insert("math.abs".to_string(), Value::Native("math.abs"));
    functions.insert("math.min".to_string(), Value::Native("math.min"));
    functions.insert("math.max".to_string(), Value::Native("math.max"));
    functions.insert("math.sqrt".to_string(), Value::Native("math.sqrt"));
    functions.insert("math.pow".to_string(), Value::Native("math.pow"));
    functions.insert("math.log".to_string(), Value::Native("math.log"));
    functions.insert("math.ceil".to_string(), Value::Native("math.ceil"));
    functions.insert("math.floor".to_string(), Value::Native("math.floor"));
    functions.insert("math.round".to_string(), Value::Native("math.round"));

    // Trigonometry
    functions.insert("math.sin".to_string(), Value::Native("math.sin"));
    functions.insert("math.cos".to_string(), Value::Native("math.cos"));
    functions.insert("math.tan".to_string(), Value::Native("math.tan"));
    functions.insert("math.asin".to_string(), Value::Native("math.asin"));
    functions.insert("math.acos".to_string(), Value::Native("math.acos"));
    functions.insert("math.atan".to_string(), Value::Native("math.atan"));
    functions.insert("math.atan2".to_string(), Value::Native("math.atan2"));

    // Random
    functions.insert("math.random".to_string(), Value::Native("math.random"));
    functions.insert(
        "math.random_int".to_string(),
        Value::Native("math.random_int"),
    );

    // Statistics
    functions.insert("math.mean".to_string(), Value::Native("math.mean"));
    functions.insert("math.median".to_string(), Value::Native("math.median"));
    functions.insert("math.std_dev".to_string(), Value::Native("math.std_dev"));
    functions.insert("math.sum".to_string(), Value::Native("math.sum"));

    // Constants
    functions.insert("math.PI".to_string(), Value::Float(std::f64::consts::PI));
    functions.insert("math.E".to_string(), Value::Float(std::f64::consts::E));

    functions
}

/// Execute a math function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "math.abs" => {
            if args.len() != 1 {
                return Err("math.abs requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Int(v) => Ok(Value::Int(v.abs())),
                Value::Float(v) => Ok(Value::Float(v.abs())),
                _ => Err("math.abs: argument must be a number".to_string()),
            }
        }

        "math.min" => {
            if args.len() != 2 {
                return Err("math.min requires 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.min(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).min(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.min(*b as f64))),
                _ => Err("math.min: arguments must be numbers".to_string()),
            }
        }

        "math.max" => {
            if args.len() != 2 {
                return Err("math.max requires 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.max(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).max(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.max(*b as f64))),
                _ => Err("math.max: arguments must be numbers".to_string()),
            }
        }

        "math.sqrt" => {
            if args.len() != 1 {
                return Err("math.sqrt requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Int(v) => Ok(Value::Float((*v as f64).sqrt())),
                Value::Float(v) => Ok(Value::Float(v.sqrt())),
                _ => Err("math.sqrt: argument must be a number".to_string()),
            }
        }

        "math.pow" => {
            if args.len() != 2 {
                return Err("math.pow requires 2 arguments (base, exp)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b < 0 {
                        Ok(Value::Float((*a as f64).powi(*b as i32)))
                    } else if let Ok(exp) = u32::try_from(*b) {
                        if let Some(v) = a.checked_pow(exp) {
                            Ok(Value::Int(v))
                        } else {
                            Ok(Value::Float((*a as f64).powf(*b as f64)))
                        }
                    } else {
                        Ok(Value::Float((*a as f64).powf(*b as f64)))
                    }
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(*b as i32))),
                _ => Err("math.pow: arguments must be numbers".to_string()),
            }
        }

        "math.log" => {
            if args.is_empty() || args.len() > 2 {
                return Err("math.log requires 1 or 2 arguments (x, base)".to_string());
            }
            match &args[0] {
                Value::Float(x) => {
                    if args.len() == 2 {
                        match &args[1] {
                            Value::Float(base) => Ok(Value::Float(x.log(*base))),
                            Value::Int(base) => Ok(Value::Float(x.log(*base as f64))),
                            _ => Err("math.log: base must be a number".to_string()),
                        }
                    } else {
                        Ok(Value::Float(x.ln()))
                    }
                }
                Value::Int(x) => {
                    let x = *x as f64;
                    if args.len() == 2 {
                        match &args[1] {
                            Value::Float(base) => Ok(Value::Float(x.log(*base))),
                            Value::Int(base) => Ok(Value::Float(x.log(*base as f64))),
                            _ => Err("math.log: base must be a number".to_string()),
                        }
                    } else {
                        Ok(Value::Float(x.ln()))
                    }
                }
                _ => Err("math.log: argument must be a number".to_string()),
            }
        }

        "math.ceil" => {
            if args.len() != 1 {
                return Err("math.ceil requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.ceil())),
                Value::Int(v) => Ok(Value::Int(*v)),
                _ => Err("math.ceil: argument must be a number".to_string()),
            }
        }

        "math.floor" => {
            if args.len() != 1 {
                return Err("math.floor requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.floor())),
                Value::Int(v) => Ok(Value::Int(*v)),
                _ => Err("math.floor: argument must be a number".to_string()),
            }
        }

        "math.round" => {
            if args.len() != 1 {
                return Err("math.round requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.round())),
                Value::Int(v) => Ok(Value::Int(*v)),
                _ => Err("math.round: argument must be a number".to_string()),
            }
        }

        "math.sin" => {
            if args.len() != 1 {
                return Err("math.sin requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.sin())),
                Value::Int(v) => Ok(Value::Float((*v as f64).sin())),
                _ => Err("math.sin: argument must be a number".to_string()),
            }
        }

        "math.cos" => {
            if args.len() != 1 {
                return Err("math.cos requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.cos())),
                Value::Int(v) => Ok(Value::Float((*v as f64).cos())),
                _ => Err("math.cos: argument must be a number".to_string()),
            }
        }

        "math.tan" => {
            if args.len() != 1 {
                return Err("math.tan requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.tan())),
                Value::Int(v) => Ok(Value::Float((*v as f64).tan())),
                _ => Err("math.tan: argument must be a number".to_string()),
            }
        }

        "math.asin" => {
            if args.len() != 1 {
                return Err("math.asin requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.asin())),
                Value::Int(v) => Ok(Value::Float((*v as f64).asin())),
                _ => Err("math.asin: argument must be a number".to_string()),
            }
        }

        "math.acos" => {
            if args.len() != 1 {
                return Err("math.acos requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.acos())),
                Value::Int(v) => Ok(Value::Float((*v as f64).acos())),
                _ => Err("math.acos: argument must be a number".to_string()),
            }
        }

        "math.atan" => {
            if args.len() != 1 {
                return Err("math.atan requires 1 argument".to_string());
            }
            match &args[0] {
                Value::Float(v) => Ok(Value::Float(v.atan())),
                Value::Int(v) => Ok(Value::Float((*v as f64).atan())),
                _ => Err("math.atan: argument must be a number".to_string()),
            }
        }

        "math.atan2" => {
            if args.len() != 2 {
                return Err("math.atan2 requires 2 arguments (y, x)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::Float(y), Value::Float(x)) => Ok(Value::Float(y.atan2(*x))),
                (Value::Int(y), Value::Int(x)) => Ok(Value::Float((*y as f64).atan2(*x as f64))),
                (Value::Int(y), Value::Float(x)) => Ok(Value::Float((*y as f64).atan2(*x))),
                (Value::Float(y), Value::Int(x)) => Ok(Value::Float(y.atan2(*x as f64))),
                _ => Err("math.atan2: arguments must be numbers".to_string()),
            }
        }

        "math.random" => {
            if !args.is_empty() {
                return Err("math.random requires no arguments".to_string());
            }
            Ok(Value::Float(rand::random::<f64>()))
        }

        "math.random_int" => {
            if args.len() != 2 {
                return Err("math.random_int requires 2 arguments (min, max)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::Int(min), Value::Int(max)) => {
                    if min >= max {
                        return Err("math.random_int: min must be less than max".to_string());
                    }
                    let range = (*max as i128) - (*min as i128);
                    let val = *min as i128 + (rand::random::<u128>() % range as u128) as i128;
                    Ok(Value::Int(val as i64))
                }
                _ => Err("math.random_int: arguments must be integers".to_string()),
            }
        }

        "math.mean" => {
            if args.len() != 1 {
                return Err("math.mean requires 1 argument (list)".to_string());
            }
            match &args[0] {
                Value::List(list) => {
                    if list.is_empty() {
                        return Err("math.mean: list cannot be empty".to_string());
                    }
                    let sum: f64 = list
                        .iter()
                        .map(|v| match v {
                            Value::Int(n) => *n as f64,
                            Value::Float(n) => *n,
                            _ => 0.0,
                        })
                        .sum();
                    Ok(Value::Float(sum / list.len() as f64))
                }
                _ => Err("math.mean: argument must be a list".to_string()),
            }
        }

        "math.median" => {
            if args.len() != 1 {
                return Err("math.median requires 1 argument (list)".to_string());
            }
            match &args[0] {
                Value::List(list) => {
                    if list.is_empty() {
                        return Err("math.median: list cannot be empty".to_string());
                    }
                    let mut values: Vec<f64> = list
                        .iter()
                        .map(|v| match v {
                            Value::Int(n) => *n as f64,
                            Value::Float(n) => *n,
                            _ => 0.0,
                        })
                        .collect();
                    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let mid = values.len() / 2;
                    if values.len().is_multiple_of(2) {
                        Ok(Value::Float((values[mid - 1] + values[mid]) / 2.0))
                    } else {
                        Ok(Value::Float(values[mid]))
                    }
                }
                _ => Err("math.median: argument must be a list".to_string()),
            }
        }

        "math.std_dev" => {
            if args.len() != 1 {
                return Err("math.std_dev requires 1 argument (list)".to_string());
            }
            match &args[0] {
                Value::List(list) => {
                    if list.is_empty() {
                        return Err("math.std_dev: list cannot be empty".to_string());
                    }
                    let values: Vec<f64> = list
                        .iter()
                        .map(|v| match v {
                            Value::Int(n) => *n as f64,
                            Value::Float(n) => *n,
                            _ => 0.0,
                        })
                        .collect();
                    let mean = values.iter().sum::<f64>() / values.len() as f64;
                    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                        / values.len() as f64;
                    Ok(Value::Float(variance.sqrt()))
                }
                _ => Err("math.std_dev: argument must be a list".to_string()),
            }
        }

        "math.sum" => {
            if args.len() != 1 {
                return Err("math.sum requires 1 argument (list)".to_string());
            }
            match &args[0] {
                Value::List(list) => {
                    let sum: f64 = list
                        .iter()
                        .map(|v| match v {
                            Value::Int(n) => *n as f64,
                            Value::Float(n) => *n,
                            _ => 0.0,
                        })
                        .sum();
                    Ok(Value::Float(sum))
                }
                _ => Err("math.sum: argument must be a list".to_string()),
            }
        }

        _ => Err(format!("Unknown math function: {}", name)),
    }
}
