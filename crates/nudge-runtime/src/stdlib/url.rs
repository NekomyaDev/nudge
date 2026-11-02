//! std/url - URL encoding and decoding (RFC 3986)
//!
//! Provides percent-encoding and decoding for query parameters and URLs.

use crate::value::Value;
use std::collections::HashMap;

/// Register all URL functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("url.encode".to_string(), Value::Native("url.encode"));
    functions.insert("url.decode".to_string(), Value::Native("url.decode"));
    functions
}

/// Execute a URL function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "url.encode" => {
            if args.len() != 1 {
                return Err("url.encode requires 1 string argument".to_string());
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => return Err("url.encode: argument must be a string".to_string()),
            };
            let mut encoded = String::with_capacity(s.len());
            for b in s.as_bytes() {
                match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        encoded.push(*b as char);
                    }
                    _ => {
                        encoded.push_str(&format!("%{:02X}", b));
                    }
                }
            }
            Ok(Value::String(encoded))
        }

        "url.decode" => {
            if args.len() != 1 {
                return Err("url.decode requires 1 string argument".to_string());
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => return Err("url.decode: argument must be a string".to_string()),
            };
            let bytes = s.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' {
                    if i + 2 >= bytes.len() {
                        return Err("invalid percent encoding in url.decode".to_string());
                    }
                    let hex = match std::str::from_utf8(&bytes[i + 1..i + 3]) {
                        Ok(h) => h,
                        Err(_) => return Err("invalid percent encoding in url.decode".to_string()),
                    };
                    match u8::from_str_radix(hex, 16) {
                        Ok(b) => out.push(b),
                        Err(_) => return Err("invalid hex digits in url.decode".to_string()),
                    }
                    i += 3;
                } else if bytes[i] == b'+' {
                    out.push(b' ');
                    i += 1;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            match String::from_utf8(out) {
                Ok(decoded) => Ok(Value::String(decoded)),
                Err(e) => Err(format!("url.decode: invalid UTF-8: {e}")),
            }
        }

        _ => Err(format!("unknown URL function: {name}")),
    }
}
