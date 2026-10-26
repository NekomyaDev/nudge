//! In-memory ephemeral key-value store for Nudge applications (uses MemStore).
//!
//! Provides ultra-fast, zero-syscall in-memory storage residing strictly
//! within the process heap. Guarantees 0 disk operations, 0 fsync overhead,
//! and nanosecond-level access times.

use crate::value::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Ephemeral in-memory store state.
#[derive(Debug, Default)]
pub struct MemStore {
    entries: HashMap<String, Value>,
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str, default: Value) -> Value {
        self.entries.get(key).cloned().unwrap_or(default)
    }

    pub fn set(&mut self, key: String, val: Value) {
        self.entries.insert(key, val);
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    pub fn keys(&self) -> Vec<Value> {
        let mut ks: Vec<String> = self.entries.keys().cloned().collect();
        ks.sort();
        ks.into_iter().map(Value::String).collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Global process-wide memory store singleton.
static MEM_STORE: OnceLock<Mutex<MemStore>> = OnceLock::new();

fn get_mem_store() -> &'static Mutex<MemStore> {
    MEM_STORE.get_or_init(|| Mutex::new(MemStore::new()))
}

/// Helper to execute an operation on the global in-memory store.
pub fn with_mem_store<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut MemStore) -> Result<R, String>,
{
    let mutex = get_mem_store();
    let mut store = mutex
        .lock()
        .map_err(|e| format!("error[E0930]: MemStore lock poisoned: {e}"))?;
    f(&mut store)
}

/// Register MemStore functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("mem.get".to_string(), Value::Native("mem.get"));
    functions.insert("mem.set".to_string(), Value::Native("mem.set"));
    functions.insert("mem.has".to_string(), Value::Native("mem.has"));
    functions.insert("mem.delete".to_string(), Value::Native("mem.delete"));
    functions.insert("mem.keys".to_string(), Value::Native("mem.keys"));
    functions.insert("mem.clear".to_string(), Value::Native("mem.clear"));
    functions
}

/// Dispatch a call to a MemStore builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "mem.get" => {
            if args.is_empty() {
                return Err("mem.get requires at least 1 argument (key, [default])".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("mem.get: key must be a string".to_string()),
            };
            let default_val = if args.len() > 1 {
                args[1].clone()
            } else {
                Value::None
            };
            with_mem_store(|s| Ok(s.get(&key, default_val)))
        }

        "mem.set" => {
            if args.len() != 2 {
                return Err("mem.set requires 2 arguments (key, val)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("mem.set: key must be a string".to_string()),
            };
            let val = args[1].clone();
            with_mem_store(|s| {
                s.set(key, val);
                Ok(Value::None)
            })
        }

        "mem.has" => {
            if args.len() != 1 {
                return Err("mem.has requires 1 argument (key)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("mem.has: key must be a string".to_string()),
            };
            with_mem_store(|s| Ok(Value::Bool(s.has(&key))))
        }

        "mem.delete" => {
            if args.len() != 1 {
                return Err("mem.delete requires 1 argument (key)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("mem.delete: key must be a string".to_string()),
            };
            with_mem_store(|s| Ok(Value::Bool(s.delete(&key))))
        }

        "mem.keys" => with_mem_store(|s| Ok(Value::List(s.keys()))),

        "mem.clear" => with_mem_store(|s| {
            s.clear();
            Ok(Value::None)
        }),

        _ => Err(format!("unknown mem function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_store_basic_operations() {
        let mut store = MemStore::new();
        assert!(!store.has("alpha"));
        assert_eq!(store.get("alpha", Value::Int(99)), Value::Int(99));

        store.set("alpha".into(), Value::String("value_a".into()));
        assert!(store.has("alpha"));
        assert_eq!(
            store.get("alpha", Value::None),
            Value::String("value_a".into())
        );

        store.set("beta".into(), Value::Int(42));
        let keys = store.keys();
        assert_eq!(
            keys,
            vec![Value::String("alpha".into()), Value::String("beta".into())]
        );

        assert!(store.delete("alpha"));
        assert!(!store.has("alpha"));
        assert!(!store.delete("alpha"));
    }
}
