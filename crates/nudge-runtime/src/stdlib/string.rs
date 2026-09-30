//! std/string - String operations
//!
//! Provides string manipulation, formatting, and conversion functions.

use crate::vm::Value;
use std::collections::HashMap;

/// Register all string functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    // String operations
    functions.insert("str.length".to_string(), Value::Native("str.length"));
    functions.insert("str.upper".to_string(), Value::Native("str.upper"));
    functions.insert("str.lower".to_string(), Value::Native("str.lower"));
    functions.insert("str.trim".to_string(), Value::Native("str.trim"));
    functions.insert("str.split".to_string(), Value::Native("str.split"));
    functions.insert("str.join".to_string(), Value::Native("str.join"));
    functions.insert("str.contains".to_string(), Value::Native("str.contains"));
    functions.insert(
        "str.starts_with".to_string(),
        Value::Native("str.starts_with"),
    );
    functions.insert("str.ends_with".to_string(), Value::Native("str.ends_with"));
    functions.insert("str.replace".to_string(), Value::Native("str.replace"));
    functions.insert("str.substring".to_string(), Value::Native("str.substring"));
    functions.insert("str.index_of".to_string(), Value::Native("str.index_of"));
    functions.insert("str.reverse".to_string(), Value::Native("str.reverse"));
    functions.insert("str.repeat".to_string(), Value::Native("str.repeat"));

    // Conversion
    functions.insert("str.to_string".to_string(), Value::Native("str.to_string"));
    functions.insert("str.to_int".to_string(), Value::Native("str.to_int"));
    functions.insert("str.to_float".to_string(), Value::Native("str.to_float"));
    functions.insert("str.parse".to_string(), Value::Native("str.parse"));

    // Formatting
    functions.insert("str.format".to_string(), Value::Native("str.format"));
    functions.insert("str.pad_left".to_string(), Value::Native("str.pad_left"));
    functions.insert("str.pad_right".to_string(), Value::Native("str.pad_right"));

    functions
}

/// Execute a string function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "str.length" => {
            if args.len() != 1 {
                return Err("str.length requires 1 argument".to_string());
            }
            match &args[0] {
                // char count, not bytes — `str.length("héllo")` must be 5
                // like Python's len(), not 6
                Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                _ => Err("str.length: argument must be a string".to_string()),
            }
        }

        "str.upper" => {
            if args.len() != 1 {
                return Err("str.upper requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.to_uppercase())),
                _ => Err("str.upper: argument must be a string".to_string()),
            }
        }

        "str.lower" => {
            if args.len() != 1 {
                return Err("str.lower requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.to_lowercase())),
                _ => Err("str.lower: argument must be a string".to_string()),
            }
        }

        "str.trim" => {
            if args.len() != 1 {
                return Err("str.trim requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.trim().to_string())),
                _ => Err("str.trim: argument must be a string".to_string()),
            }
        }

        "str.split" => {
            if args.len() != 2 {
                return Err("str.split requires 2 arguments (string, delimiter)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(delim)) => {
                    let parts: Vec<Value> = s
                        .split(delim.as_str())
                        .map(|p| Value::String(p.to_string()))
                        .collect();
                    Ok(Value::List(parts))
                }
                _ => Err("str.split: arguments must be strings".to_string()),
            }
        }

        "str.join" => {
            if args.len() != 2 {
                return Err("str.join requires 2 arguments (list, separator)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::List(list), Value::String(sep)) => {
                    let parts: Vec<String> = list.iter().map(|v| format!("{}", v)).collect();
                    Ok(Value::String(parts.join(sep)))
                }
                _ => Err("str.join: first argument must be a list, second a string".to_string()),
            }
        }

        "str.contains" => {
            if args.len() != 2 {
                return Err("str.contains requires 2 arguments (string, substring)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(sub)) => Ok(Value::Bool(s.contains(sub.as_str()))),
                _ => Err("str.contains: arguments must be strings".to_string()),
            }
        }

        "str.starts_with" => {
            if args.len() != 2 {
                return Err("str.starts_with requires 2 arguments (string, prefix)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(prefix)) => {
                    Ok(Value::Bool(s.starts_with(prefix.as_str())))
                }
                _ => Err("str.starts_with: arguments must be strings".to_string()),
            }
        }

        "str.ends_with" => {
            if args.len() != 2 {
                return Err("str.ends_with requires 2 arguments (string, suffix)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(suffix)) => {
                    Ok(Value::Bool(s.ends_with(suffix.as_str())))
                }
                _ => Err("str.ends_with: arguments must be strings".to_string()),
            }
        }

        "str.replace" => {
            if args.len() != 3 {
                return Err("str.replace requires 3 arguments (string, old, new)".to_string());
            }
            match (&args[0], &args[1], &args[2]) {
                (Value::String(s), Value::String(old), Value::String(new)) => {
                    Ok(Value::String(s.replace(old.as_str(), new)))
                }
                _ => Err("str.replace: arguments must be strings".to_string()),
            }
        }

        "str.substring" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(
                    "str.substring requires 2 or 3 arguments (string, start, end?)".to_string(),
                );
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::Int(start)) => {
                    // char-indexed, matching Python str[a:b]: byte slicing
                    // panics on char-boundary errors for non-ASCII input
                    let chars: Vec<char> = s.chars().collect();
                    let start = (*start).max(0) as usize;
                    let slice = |a: usize, b: usize| -> Result<Value, String> {
                        if a > b || b > chars.len() {
                            return Err("str.substring: indices out of bounds".to_string());
                        }
                        Ok(Value::String(chars[a..b].iter().collect()))
                    };
                    if args.len() == 3 {
                        match &args[2] {
                            Value::Int(end) => slice(start, (*end).max(0) as usize),
                            _ => Err("str.substring: end index must be an integer".to_string()),
                        }
                    } else {
                        slice(start, chars.len())
                    }
                }
                _ => Err(
                    "str.substring: first argument must be a string, second an integer".to_string(),
                ),
            }
        }

        "str.index_of" => {
            if args.len() != 2 {
                return Err("str.index_of requires 2 arguments (string, substring)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(sub)) => match s.find(sub.as_str()) {
                    // char index, not byte offset — aligns with str.substring and str.length
                    Some(byte_idx) => {
                        let char_idx = s[..byte_idx].chars().count() as i64;
                        Ok(Value::Int(char_idx))
                    }
                    None => Ok(Value::Int(-1)),
                },
                _ => Err("str.index_of: arguments must be strings".to_string()),
            }
        }

        "str.reverse" => {
            if args.len() != 1 {
                return Err("str.reverse requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.chars().rev().collect())),
                _ => Err("str.reverse: argument must be a string".to_string()),
            }
        }

        "str.repeat" => {
            if args.len() != 2 {
                return Err("str.repeat requires 2 arguments (string, count)".to_string());
            }
            match (&args[0], &args[1]) {
                (Value::String(s), Value::Int(count)) => {
                    if *count < 0 {
                        return Err("str.repeat: count must be non-negative".to_string());
                    }
                    Ok(Value::String(s.repeat(*count as usize)))
                }
                _ => Err(
                    "str.repeat: first argument must be a string, second an integer".to_string(),
                ),
            }
        }

        "str.to_string" => {
            if args.len() != 1 {
                return Err("str.to_string requires 1 argument".to_string());
            }
            Ok(Value::String(format!("{}", args[0])))
        }

        "str.to_int" => {
            if args.len() != 1 {
                return Err("str.to_int requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => match s.trim().parse::<i64>() {
                    Ok(v) => Ok(Value::Int(v)),
                    Err(_) => Ok(Value::None),
                },
                Value::Float(v) => Ok(Value::Int(*v as i64)),
                Value::Int(v) => Ok(Value::Int(*v)),
                _ => Ok(Value::None),
            }
        }

        "str.to_float" => {
            if args.len() != 1 {
                return Err("str.to_float requires 1 argument".to_string());
            }
            match &args[0] {
                Value::String(s) => match s.trim().parse::<f64>() {
                    Ok(v) => Ok(Value::Float(v)),
                    Err(_) => Ok(Value::None),
                },
                Value::Int(v) => Ok(Value::Float(*v as f64)),
                Value::Float(v) => Ok(Value::Float(*v)),
                _ => Ok(Value::None),
            }
        }

        "str.parse" => {
            if args.len() != 1 {
                return Err("str.parse requires 1 argument".to_string());
            }
            // minimal scalar parse (registered but previously unreachable):
            // int → float → bool → null; anything else stays a string
            match &args[0] {
                Value::String(s) => {
                    let t = s.trim();
                    if let Ok(v) = t.parse::<i64>() {
                        return Ok(Value::Int(v));
                    }
                    if let Ok(v) = t.parse::<f64>() {
                        return Ok(Value::Float(v));
                    }
                    match t {
                        "true" => Ok(Value::Bool(true)),
                        "false" => Ok(Value::Bool(false)),
                        "null" | "" => Ok(Value::None),
                        _ => Ok(Value::String(s.clone())),
                    }
                }
                _ => Err("str.parse: argument must be a string".to_string()),
            }
        }

        "str.format" => {
            if args.is_empty() {
                return Err("str.format requires at least 1 argument".to_string());
            }
            match &args[0] {
                Value::String(template) => {
                    let mut out = String::with_capacity(template.len());
                    let chars: Vec<char> = template.chars().collect();
                    let mut i = 0;
                    while i < chars.len() {
                        if chars[i] == '{' {
                            let mut j = i + 1;
                            while j < chars.len() && chars[j].is_ascii_digit() {
                                j += 1;
                            }
                            if j > i + 1 && j < chars.len() && chars[j] == '}' {
                                let idx_str: String = chars[i + 1..j].iter().collect();
                                if let Ok(idx) = idx_str.parse::<usize>() {
                                    if idx + 1 < args.len() {
                                        out.push_str(&format!("{}", args[idx + 1]));
                                        i = j + 1;
                                        continue;
                                    }
                                }
                            }
                        }
                        out.push(chars[i]);
                        i += 1;
                    }
                    Ok(Value::String(out))
                }
                _ => Err("str.format: first argument must be a string".to_string()),
            }
        }

        "str.pad_left" => {
            if args.len() != 3 {
                return Err("str.pad_left requires 3 arguments (string, width, char)".to_string());
            }
            match (&args[0], &args[1], &args[2]) {
                (Value::String(s), Value::Int(width), Value::String(pad)) => {
                    let width = (*width).max(0) as usize;
                    // char-based width: byte len pads short for non-ASCII
                    let char_len = s.chars().count();
                    if char_len >= width {
                        Ok(Value::String(s.clone()))
                    } else {
                        let pad_char = pad.chars().next().unwrap_or(' ');
                        let padding: String =
                            std::iter::repeat_n(pad_char, width - char_len).collect();
                        Ok(Value::String(format!("{}{}", padding, s)))
                    }
                }
                _ => Err("str.pad_left: arguments must be (string, int, string)".to_string()),
            }
        }

        "str.pad_right" => {
            if args.len() != 3 {
                return Err("str.pad_right requires 3 arguments (string, width, char)".to_string());
            }
            match (&args[0], &args[1], &args[2]) {
                (Value::String(s), Value::Int(width), Value::String(pad)) => {
                    let width = (*width).max(0) as usize;
                    // char-based width: byte len pads short for non-ASCII
                    let char_len = s.chars().count();
                    if char_len >= width {
                        Ok(Value::String(s.clone()))
                    } else {
                        let pad_char = pad.chars().next().unwrap_or(' ');
                        let padding: String =
                            std::iter::repeat_n(pad_char, width - char_len).collect();
                        Ok(Value::String(format!("{}{}", s, padding)))
                    }
                }
                _ => Err("str.pad_right: arguments must be (string, int, string)".to_string()),
            }
        }

        _ => Err(format!("Unknown string function: {}", name)),
    }
}
