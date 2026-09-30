//! std/io - Input/Output operations
//!
//! Provides file I/O, console I/O, and filesystem operations.

use crate::vm::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};

/// Register all io functions
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    // File operations
    functions.insert("io.read".to_string(), Value::Native("io.read"));
    functions.insert("io.write".to_string(), Value::Native("io.write"));
    functions.insert("io.append".to_string(), Value::Native("io.append"));
    functions.insert("io.exists".to_string(), Value::Native("io.exists"));
    functions.insert("io.delete".to_string(), Value::Native("io.delete"));
    functions.insert("io.list_dir".to_string(), Value::Native("io.list_dir"));

    // Console operations
    functions.insert("io.print".to_string(), Value::Native("io.print"));
    functions.insert("io.println".to_string(), Value::Native("io.println"));
    functions.insert("io.read_line".to_string(), Value::Native("io.read_line"));
    functions.insert("io.flush".to_string(), Value::Native("io.flush"));

    functions
}

/// Execute an io function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "io.read" => {
            if args.len() != 1 {
                return Err("io.read requires 1 argument (path)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.read: path must be a string".to_string()),
            };
            match fs::read_to_string(&path) {
                Ok(content) => Ok(Value::String(content)),
                Err(e) => Err(format!("io.read: cannot read '{}': {}", path, e)),
            }
        }

        "io.write" => {
            if args.len() != 2 {
                return Err("io.write requires 2 arguments (path, content)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.write: path must be a string".to_string()),
            };
            let content = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("io.write: content must be a string".to_string()),
            };
            match fs::write(&path, &content) {
                Ok(()) => Ok(Value::None),
                Err(e) => Err(format!("io.write: cannot write '{}': {}", path, e)),
            }
        }

        "io.append" => {
            if args.len() != 2 {
                return Err("io.append requires 2 arguments (path, content)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.append: path must be a string".to_string()),
            };
            let content = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("io.append: content must be a string".to_string()),
            };
            use std::io::Write;
            match fs::OpenOptions::new().create(true).append(true).open(&path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(content.as_bytes()) {
                        return Err(format!("io.append: cannot write '{}': {}", path, e));
                    }
                    Ok(Value::None)
                }
                Err(e) => Err(format!("io.append: cannot open '{}': {}", path, e)),
            }
        }

        "io.exists" => {
            if args.len() != 1 {
                return Err("io.exists requires 1 argument (path)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.exists: path must be a string".to_string()),
            };
            Ok(Value::Bool(fs::metadata(&path).is_ok()))
        }

        "io.delete" => {
            if args.len() != 1 {
                return Err("io.delete requires 1 argument (path)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.delete: path must be a string".to_string()),
            };
            match fs::remove_file(&path) {
                Ok(()) => Ok(Value::None),
                Err(e) => Err(format!("io.delete: cannot delete '{}': {}", path, e)),
            }
        }

        "io.list_dir" => {
            if args.len() != 1 {
                return Err("io.list_dir requires 1 argument (path)".to_string());
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("io.list_dir: path must be a string".to_string()),
            };
            match fs::read_dir(&path) {
                Ok(entries) => {
                    let mut result = Vec::new();
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            result.push(Value::String(name.to_string()));
                        }
                    }
                    Ok(Value::List(result))
                }
                Err(e) => Err(format!("io.list_dir: cannot list '{}': {}", path, e)),
            }
        }

        "io.print" => {
            for arg in &args {
                print!("{}", arg);
            }
            Ok(Value::None)
        }

        "io.println" => {
            for arg in &args {
                print!("{}", arg);
            }
            println!();
            Ok(Value::None)
        }

        "io.read_line" => {
            if args.len() > 1 {
                return Err("io.read_line requires 0 or 1 argument (prompt)".to_string());
            }
            if args.len() == 1 {
                if let Value::String(prompt) = &args[0] {
                    print!("{}", prompt);
                    let _ = io::stdout().flush();
                }
            }
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("io.read_line: cannot read from stdin: {}", e))?;
            Ok(Value::String(input.trim().to_string()))
        }

        "io.flush" => {
            io::stdout()
                .flush()
                .map_err(|e| format!("io.flush: cannot flush stdout: {}", e))?;
            Ok(Value::None)
        }

        _ => Err(format!("Unknown io function: {}", name)),
    }
}
