//! Safe, persistent key-value store for Nudge applications.
//!
//! Provides a crash-safe key-value store rooted under the workspace:
//! `<workspace_root>/.nudge/store.json` (or `NUDGE_STORE_PATH` override).
//!
//! Crash-consistent writes:
//! 1. Serialize store entries to formatted JSON.
//! 2. Write to a temporary sibling file: `.<filename>.tmp.<pid>.<timestamp>`.
//! 3. Flush and sync to disk (fsync).
//! 4. Atomically commit replacement via rename.
//!
//! Values are cached in memory and lazily loaded on first access.
//! Modifications flush synchronously to disk.

use crate::value::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Resolve the path to the store file.
///
/// Uses `NUDGE_STORE_PATH` if configured; otherwise resolves to
/// `<workspace_root>/.nudge/store.json`.
pub fn get_store_path() -> PathBuf {
    if let Ok(path_str) = std::env::var("NUDGE_STORE_PATH") {
        if !path_str.is_empty() {
            return PathBuf::from(path_str);
        }
    }
    let ws_root = crate::workspace::get_workspace_root();
    ws_root.join(".nudge").join("store.json")
}

/// Convert a runtime `Value` to a `serde_json::Value`.
///
/// Primitives, lists, and maps/records are serialized.
/// Non-serializable values (functions, native handles) return an error.
pub fn value_to_json(val: &Value) -> Result<serde_json::Value, String> {
    match val {
        Value::Int(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Float(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                Ok(serde_json::Value::Number(n))
            } else {
                Err(format!("cannot serialize non-finite float {f} to JSON"))
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::None => Ok(serde_json::Value::Null),
        Value::List(xs) => {
            let mut arr = Vec::with_capacity(xs.len());
            for x in xs {
                arr.push(value_to_json(x)?);
            }
            Ok(serde_json::Value::Array(arr))
        }
        Value::Map(m) => {
            let mut obj = serde_json::Map::with_capacity(m.len());
            for (k, v) in m {
                obj.insert(k.clone(), value_to_json(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        Value::Function(idx) => Err(format!(
            "cannot serialize function (index {idx}) to store JSON"
        )),
        Value::Native(name) => Err(format!(
            "cannot serialize native function '{name}' to store JSON"
        )),
    }
}

/// Convert a `serde_json::Value` back to a runtime `Value`.
pub fn json_to_value(j: serde_json::Value) -> Result<Value, String> {
    match j {
        serde_json::Value::Null => Ok(Value::None),
        serde_json::Value::Bool(b) => Ok(Value::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(u) = n.as_u64() {
                Ok(Value::Int(u as i64))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(format!("cannot parse JSON number: {n}"))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s)),
        serde_json::Value::Array(arr) => {
            let mut list = Vec::with_capacity(arr.len());
            for item in arr {
                list.push(json_to_value(item)?);
            }
            Ok(Value::List(list))
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::with_capacity(obj.len());
            for (k, v) in obj {
                map.insert(k, json_to_value(v)?);
            }
            Ok(Value::Map(map))
        }
    }
}

/// In-memory key-value store with atomic disk persistence.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    entries: HashMap<String, Value>,
}

impl Store {
    /// Open a store at the specified path.
    ///
    /// If the file exists, it is read and deserialized.
    /// If the file does not exist, an empty store is returned.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let entries = if path.exists() {
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("store: cannot read '{}': {}", path.display(), e))?;
            let json: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|e| format!("store: invalid JSON in '{}': {}", path.display(), e))?;
            match json {
                serde_json::Value::Object(map) => {
                    let mut entries = HashMap::with_capacity(map.len());
                    for (k, v) in map {
                        entries.insert(k, json_to_value(v)?);
                    }
                    entries
                }
                _ => {
                    return Err(format!(
                        "store: root JSON must be an object in '{}'",
                        path.display()
                    ))
                }
            }
        } else {
            HashMap::new()
        };

        Ok(Store { path, entries })
    }

    /// Retrieve the value stored at `key`, or return `default` if absent.
    pub fn get(&self, key: &str, default: Value) -> Value {
        self.entries.get(key).cloned().unwrap_or(default)
    }

    /// Check if `key` exists in the store.
    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Store `val` under `key` and flush atomically to disk.
    pub fn set(&mut self, key: &str, val: Value) -> Result<(), String> {
        // Pre-validate serializability before mutating in-memory state
        let _ = value_to_json(&val)?;
        self.entries.insert(key.to_string(), val);
        self.flush()
    }

    /// Delete `key` from the store and flush atomically to disk.
    /// Returns `true` if key was present, `false` otherwise.
    pub fn delete(&mut self, key: &str) -> Result<bool, String> {
        if self.entries.remove(key).is_some() {
            self.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Return all keys starting with `prefix` in sorted order.
    pub fn keys(&self, prefix: &str) -> Vec<String> {
        let mut res: Vec<String> = self
            .entries
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        res.sort();
        res
    }

    /// Atomically flush the current store state to disk.
    pub fn flush(&self) -> Result<(), String> {
        let mut obj = serde_json::Map::new();
        let mut sorted_keys: Vec<&String> = self.entries.keys().collect();
        sorted_keys.sort();
        for k in sorted_keys {
            let v = self.entries.get(k).unwrap();
            let json_v = value_to_json(v)?;
            obj.insert(k.clone(), json_v);
        }
        let json_val = serde_json::Value::Object(obj);
        let serialized = serde_json::to_string_pretty(&json_val)
            .map_err(|e| format!("store: failed to serialize store to JSON: {e}"))?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "store: cannot create directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
        }

        // Crash-safe atomic write: temp sibling + fsync + rename
        let file_name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("store.json");
        let temp_name = format!(
            ".{}.tmp.{}.{}",
            file_name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let temp_path = self.path.with_file_name(temp_name);

        let mut file = fs::File::create(&temp_path).map_err(|e| {
            format!(
                "store: cannot create temp file '{}': {}",
                temp_path.display(),
                e
            )
        })?;

        if let Err(e) = file.write_all(serialized.as_bytes()) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "store: cannot write temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }

        if let Err(e) = file.flush() {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "store: cannot flush temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }

        if let Err(e) = file.sync_all() {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "store: cannot sync temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }

        drop(file);

        if let Err(e) = fs::rename(&temp_path, &self.path) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "store: cannot commit atomic write to '{}': {}",
                self.path.display(),
                e
            ));
        }

        Ok(())
    }
}

// Global cached store instance for standard library execution
static GLOBAL_STORE: Mutex<Option<Store>> = Mutex::new(None);

/// Reset the global in-memory store cache.
///
/// Forces the next store access to reload from disk.
pub fn reset() {
    let mut lock = GLOBAL_STORE.lock().unwrap_or_else(|e| e.into_inner());
    *lock = None;
}

fn with_store<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Store) -> Result<R, String>,
{
    let target_path = get_store_path();
    let mut lock = GLOBAL_STORE.lock().unwrap_or_else(|e| e.into_inner());
    let needs_load = match &*lock {
        Some(s) => s.path != target_path,
        None => true,
    };
    if needs_load {
        let store = Store::open(target_path)?;
        *lock = Some(store);
    }
    let store = lock.as_mut().expect("store initialized");
    f(store)
}

/// Register store functions in the standard library table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("store.get".to_string(), Value::Native("store.get"));
    functions.insert("store.set".to_string(), Value::Native("store.set"));
    functions.insert("store.has".to_string(), Value::Native("store.has"));
    functions.insert("store.delete".to_string(), Value::Native("store.delete"));
    functions.insert("store.keys".to_string(), Value::Native("store.keys"));
    functions
}

/// Dispatch a call to a store builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "store.get" => {
            if args.len() != 2 {
                return Err("store.get requires 2 arguments (key, default)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("store.get: key must be a string".to_string()),
            };
            let default_val = args[1].clone();
            with_store(|s| Ok(s.get(&key, default_val)))
        }

        "store.set" => {
            if args.len() != 2 {
                return Err("store.set requires 2 arguments (key, val)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("store.set: key must be a string".to_string()),
            };
            let val = args[1].clone();
            with_store(|s| {
                s.set(&key, val)?;
                Ok(Value::None)
            })
        }

        "store.has" => {
            if args.len() != 1 {
                return Err("store.has requires 1 argument (key)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("store.has: key must be a string".to_string()),
            };
            with_store(|s| Ok(Value::Bool(s.has(&key))))
        }

        "store.delete" => {
            if args.len() != 1 {
                return Err("store.delete requires 1 argument (key)".to_string());
            }
            let key = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("store.delete: key must be a string".to_string()),
            };
            with_store(|s| Ok(Value::Bool(s.delete(&key)?)))
        }

        "store.keys" => {
            let prefix = match args.len() {
                0 => "".to_string(),
                1 => match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("store.keys: prefix must be a string".to_string()),
                },
                n => return Err(format!("store.keys takes at most 1 argument, got {n}")),
            };
            with_store(|s| {
                let keys = s.keys(&prefix);
                Ok(Value::List(keys.into_iter().map(Value::String).collect()))
            })
        }

        other => Err(format!("unknown store function '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_temp_workspace(suffix: &str) -> PathBuf {
        let temp_dir =
            std::env::temp_dir().join(format!("nudge_store_{}_{}", suffix, std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        std::env::set_var("NUDGE_WORKSPACE_DIR", &temp_dir);
        std::env::remove_var("NUDGE_STORE_PATH");
        reset();
        temp_dir
    }

    fn teardown(temp_dir: &PathBuf) {
        let _ = fs::remove_dir_all(temp_dir);
        std::env::remove_var("NUDGE_WORKSPACE_DIR");
        std::env::remove_var("NUDGE_STORE_PATH");
        reset();
    }

    #[test]
    fn test_store_round_trip_primitives() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("primitives");

        // Set int
        let res = execute(
            "store.set",
            vec![Value::String("count".into()), Value::Int(42)],
        );
        assert!(res.is_ok(), "set int failed: {:?}", res);

        // Get int
        let res = execute(
            "store.get",
            vec![Value::String("count".into()), Value::Int(0)],
        );
        assert_eq!(res.unwrap(), Value::Int(42));

        // Set string
        let res = execute(
            "store.set",
            vec![Value::String("theme".into()), Value::String("dark".into())],
        );
        assert!(res.is_ok(), "set string failed: {:?}", res);

        // Get string
        let res = execute(
            "store.get",
            vec![Value::String("theme".into()), Value::String("light".into())],
        );
        assert_eq!(res.unwrap(), Value::String("dark".into()));

        // Set bool
        let res = execute(
            "store.set",
            vec![Value::String("enabled".into()), Value::Bool(true)],
        );
        assert!(res.is_ok(), "set bool failed: {:?}", res);

        // Get bool
        let res = execute(
            "store.get",
            vec![Value::String("enabled".into()), Value::Bool(false)],
        );
        assert_eq!(res.unwrap(), Value::Bool(true));

        teardown(&dir);
    }

    #[test]
    fn test_store_round_trip_map_and_list() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("structured");

        // Map/record
        let mut rec = HashMap::new();
        rec.insert("user".into(), Value::String("alice".into()));
        rec.insert("score".into(), Value::Int(100));
        let res = execute(
            "store.set",
            vec![Value::String("profile".into()), Value::Map(rec.clone())],
        );
        assert!(res.is_ok(), "set map failed: {:?}", res);

        let res = execute(
            "store.get",
            vec![Value::String("profile".into()), Value::None],
        );
        assert_eq!(res.unwrap(), Value::Map(rec));

        // List
        let list = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        let res = execute(
            "store.set",
            vec![Value::String("nums".into()), Value::List(list.clone())],
        );
        assert!(res.is_ok(), "set list failed: {:?}", res);

        let res = execute(
            "store.get",
            vec![Value::String("nums".into()), Value::List(vec![])],
        );
        assert_eq!(res.unwrap(), Value::List(list));

        teardown(&dir);
    }

    #[test]
    fn test_store_missing_key_returns_default() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("missing");

        let res = execute(
            "store.get",
            vec![
                Value::String("unknown_key".into()),
                Value::String("fallback".into()),
            ],
        );
        assert_eq!(res.unwrap(), Value::String("fallback".into()));

        let res_num = execute(
            "store.get",
            vec![Value::String("missing_num".into()), Value::Int(-1)],
        );
        assert_eq!(res_num.unwrap(), Value::Int(-1));

        teardown(&dir);
    }

    #[test]
    fn test_store_persistence_survives_fresh_instance() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("persistence");
        let store_file = dir.join(".nudge").join("store.json");

        // 1. First store instance writes a value
        let mut store1 = Store::open(store_file.clone()).expect("open store1");
        store1
            .set("session_count", Value::Int(42))
            .expect("store1 set");
        drop(store1);

        // Verify the file was physically created on disk
        assert!(
            store_file.exists(),
            "store.json must exist physically on disk"
        );

        // 2. Fresh store instance (simulating a separate process run)
        let store2 = Store::open(store_file.clone()).expect("open store2");
        assert_eq!(
            store2.get("session_count", Value::Int(0)),
            Value::Int(42),
            "fresh store instance must read value written by prior instance"
        );

        // 3. Test through execute + reset
        let res = execute(
            "store.set",
            vec![
                Value::String("proc_token".into()),
                Value::String("xyz123".into()),
            ],
        );
        assert!(res.is_ok());

        // Invalidate in-memory cache to force re-reading from disk
        reset();

        let res2 = execute(
            "store.get",
            vec![
                Value::String("proc_token".into()),
                Value::String("none".into()),
            ],
        );
        assert_eq!(
            res2.unwrap(),
            Value::String("xyz123".into()),
            "cached reset must reload from persisted file"
        );

        teardown(&dir);
    }

    #[test]
    fn test_store_crash_safety_atomic_write() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("crash_safety");
        let store_file = dir.join(".nudge").join("store.json");

        // Set initial valid content
        let mut store = Store::open(store_file.clone()).expect("open store");
        store
            .set("initial", Value::String("valid_data".into()))
            .expect("initial set");

        let content_before = fs::read_to_string(&store_file).expect("read content");
        assert!(content_before.contains("valid_data"));

        // Attempt to write a non-serializable value (function index)
        let res_err = store.set("bad", Value::Function(99));
        assert!(res_err.is_err(), "function must not be serializable");

        // Verify store.json remains completely unmodified and valid
        let content_after = fs::read_to_string(&store_file).expect("read content after err");
        assert_eq!(
            content_before, content_after,
            "failed write must not alter or truncate existing store file"
        );

        // Assert temp siblings are cleaned up and no stray temp files remain
        let nudge_dir = dir.join(".nudge");
        let entries = fs::read_dir(&nudge_dir).expect("read nudge dir");
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(
                !name.contains(".tmp."),
                "stray temp file found: {}",
                path.display()
            );
        }

        teardown(&dir);
    }

    #[test]
    fn test_store_has_delete_keys_behave() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("operations");

        // Populate entries
        let _ = execute(
            "store.set",
            vec![
                Value::String("user:1".into()),
                Value::String("alice".into()),
            ],
        );
        let _ = execute(
            "store.set",
            vec![Value::String("user:2".into()), Value::String("bob".into())],
        );
        let _ = execute(
            "store.set",
            vec![Value::String("app:version".into()), Value::Int(1)],
        );

        // Test has
        let has1 = execute("store.has", vec![Value::String("user:1".into())]).unwrap();
        assert_eq!(has1, Value::Bool(true));
        let has_none = execute("store.has", vec![Value::String("user:3".into())]).unwrap();
        assert_eq!(has_none, Value::Bool(false));

        // Test keys with prefix
        let user_keys = execute("store.keys", vec![Value::String("user:".into())]).unwrap();
        assert_eq!(
            user_keys,
            Value::List(vec![
                Value::String("user:1".into()),
                Value::String("user:2".into()),
            ])
        );

        // Test keys all
        let all_keys = execute("store.keys", vec![]).unwrap();
        assert_eq!(
            all_keys,
            Value::List(vec![
                Value::String("app:version".into()),
                Value::String("user:1".into()),
                Value::String("user:2".into()),
            ])
        );

        // Test delete
        let del_res = execute("store.delete", vec![Value::String("user:1".into())]).unwrap();
        assert_eq!(del_res, Value::Bool(true));

        // Delete non-existent returns false
        let del_again = execute("store.delete", vec![Value::String("user:1".into())]).unwrap();
        assert_eq!(del_again, Value::Bool(false));

        // Verify has is now false
        let has_after = execute("store.has", vec![Value::String("user:1".into())]).unwrap();
        assert_eq!(has_after, Value::Bool(false));

        // Verify keys after delete
        let keys_after = execute("store.keys", vec![Value::String("user:".into())]).unwrap();
        assert_eq!(
            keys_after,
            Value::List(vec![Value::String("user:2".into())])
        );

        teardown(&dir);
    }

    #[test]
    fn test_store_mutex_poisoning_recovery() {
        let _guard = crate::workspace::WS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // Deliberately poison GLOBAL_STORE mutex in a background thread
        let handle = std::thread::spawn(|| {
            let _guard = GLOBAL_STORE.lock().unwrap_or_else(|e| e.into_inner());
            panic!("deliberate panic to poison GLOBAL_STORE mutex");
        });
        let _ = handle.join();

        // Calling reset() on the main thread must recover from poisoning without panicking
        reset();
    }
}
