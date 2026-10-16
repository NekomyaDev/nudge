//! File watching runtime effect implementation.
//!
//! Provides bounded, sandboxed file and directory change monitoring confined
//! to the active Nudge workspace.

use crate::value::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Default watch timeout when not specified by parameter or environment.
const DEFAULT_WATCH_TIMEOUT_MS: u64 = 1000;
/// Maximum allowed watch timeout to prevent indefinite hangs.
const MAX_WATCH_TIMEOUT_MS: u64 = 60000;
/// Polling interval during watch loop.
const POLL_INTERVAL_MS: u64 = 20;

/// Snapshot metadata for a single filesystem entity.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntrySnapshot {
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
}

/// Register fs.* standard library functions.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("fs.watch".to_string(), Value::Native("fs.watch"));
    functions.insert("fs.changed".to_string(), Value::Native("fs.changed"));
    functions
}

/// Execute an fs.* function call.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "fs.watch" => execute_watch(args),
        "fs.changed" => execute_changed(args),
        _ => Err(format!("unknown fs function '{name}'")),
    }
}

fn resolve_timeout(arg_timeout: Option<&Value>) -> Result<Duration, String> {
    let ms = if let Some(v) = arg_timeout {
        match v {
            Value::Int(i) => {
                if *i < 0 {
                    return Err("fs.watch: timeout_ms must be non-negative".to_string());
                }
                *i as u64
            }
            Value::Float(f) => {
                if *f < 0.0 {
                    return Err("fs.watch: timeout_ms must be non-negative".to_string());
                }
                *f as u64
            }
            _ => return Err("fs.watch: timeout_ms must be a number".to_string()),
        }
    } else if let Ok(env_val) = std::env::var("NUDGE_FS_WATCH_TIMEOUT") {
        env_val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid NUDGE_FS_WATCH_TIMEOUT: {e}"))?
    } else {
        DEFAULT_WATCH_TIMEOUT_MS
    };

    let clamped = ms.clamp(1, MAX_WATCH_TIMEOUT_MS);
    Ok(Duration::from_millis(clamped))
}

fn take_snapshot(target: &Path) -> BTreeMap<PathBuf, EntrySnapshot> {
    let mut map = BTreeMap::new();
    if !target.exists() {
        return map;
    }

    if let Ok(meta) = fs::symlink_metadata(target) {
        if meta.is_dir() {
            collect_dir_snapshot(target, target, &mut map);
        } else {
            map.insert(
                target.to_path_buf(),
                EntrySnapshot {
                    is_dir: false,
                    size: meta.len(),
                    modified: meta.modified().ok(),
                },
            );
        }
    }
    map
}

fn collect_dir_snapshot(root: &Path, dir: &Path, map: &mut BTreeMap<PathBuf, EntrySnapshot>) {
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        // Ignore temporary atomic-write files if any
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.starts_with(".nudge_tmp_") {
                continue;
            }
        }

        if let Ok(meta) = fs::symlink_metadata(&path) {
            let is_dir = meta.is_dir();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            map.insert(
                rel,
                EntrySnapshot {
                    is_dir,
                    size: meta.len(),
                    modified: meta.modified().ok(),
                },
            );
            if is_dir {
                collect_dir_snapshot(root, &path, map);
            }
        }
    }
}

/// Watch a path inside the workspace for filesystem change events.
fn execute_watch(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("fs.watch requires at least 1 argument (path)".to_string());
    }
    let path_str = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("fs.watch: path must be a string".to_string()),
    };

    // Workspace confinement check: deny paths escaping workspace root
    let canonical_target = crate::workspace::resolve_and_validate_path(path_str).map_err(|e| {
        if e.starts_with("error[") {
            e
        } else {
            format!("error[E0922]: fs.watch path escape denied: {e}")
        }
    })?;

    let timeout = resolve_timeout(args.get(1))?;
    let start = Instant::now();

    let initial = take_snapshot(&canonical_target);

    loop {
        if crate::task::is_current_cancelled() {
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }

        let current = take_snapshot(&canonical_target);
        let mut events = Vec::new();

        // 1. Created: present in current but not in initial
        for rel_path in current.keys() {
            if !initial.contains_key(rel_path) {
                let mut ev = HashMap::new();
                ev.insert("kind".to_string(), Value::String("created".to_string()));
                ev.insert(
                    "path".to_string(),
                    Value::String(rel_path.to_string_lossy().to_string()),
                );
                events.push(Value::Map(ev));
            }
        }

        // 2. Removed: present in initial but not in current
        for rel_path in initial.keys() {
            if !current.contains_key(rel_path) {
                let mut ev = HashMap::new();
                ev.insert("kind".to_string(), Value::String("removed".to_string()));
                ev.insert(
                    "path".to_string(),
                    Value::String(rel_path.to_string_lossy().to_string()),
                );
                events.push(Value::Map(ev));
            }
        }

        // 3. Modified: present in both, but metadata differs
        for (rel_path, curr_snap) in &current {
            if let Some(init_snap) = initial.get(rel_path) {
                if !curr_snap.is_dir && init_snap != curr_snap {
                    let mut ev = HashMap::new();
                    ev.insert("kind".to_string(), Value::String("modified".to_string()));
                    ev.insert(
                        "path".to_string(),
                        Value::String(rel_path.to_string_lossy().to_string()),
                    );
                    events.push(Value::Map(ev));
                }
            }
        }

        if !events.is_empty() {
            // Sort events deterministically by path then kind
            events.sort_by(|a, b| {
                let get_key = |v: &Value| -> (String, String) {
                    if let Value::Map(m) = v {
                        let p = m
                            .get("path")
                            .and_then(|x| match x {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        let k = m
                            .get("kind")
                            .and_then(|x| match x {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        (p, k)
                    } else {
                        (String::new(), String::new())
                    }
                };
                get_key(a).cmp(&get_key(b))
            });
            return Ok(Value::List(events));
        }

        if start.elapsed() >= timeout {
            return Ok(Value::List(Vec::new()));
        }

        let remaining = timeout.saturating_sub(start.elapsed());
        let sleep_dur = remaining.min(Duration::from_millis(POLL_INTERVAL_MS));
        std::thread::sleep(sleep_dur);
    }
}

/// Check if a path inside the workspace changed within a bounded wait window.
fn execute_changed(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("fs.changed requires at least 1 argument (path)".to_string());
    }
    let res = execute_watch(args)?;
    match res {
        Value::List(evs) => Ok(Value::Bool(!evs.is_empty())),
        _ => Ok(Value::Bool(false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn setup_temp_workspace(suffix: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uid = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp_dir = std::env::temp_dir().join(format!(
            "nudge_fswatch_{}_{}_{}",
            suffix,
            std::process::id(),
            uid
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        std::env::set_var("NUDGE_WORKSPACE_DIR", &temp_dir);
        temp_dir
    }

    fn teardown(temp_dir: &Path) {
        let _ = fs::remove_dir_all(temp_dir);
        std::env::remove_var("NUDGE_WORKSPACE_DIR");
    }

    #[test]
    fn test_fs_watch_detects_creation_and_modification() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let ws = setup_temp_workspace("detects_create");

        let sub = ws.join("watched");
        fs::create_dir_all(&sub).unwrap();

        // Spawn thread that creates a file shortly after watching starts
        let target_file = sub.join("sample.txt");
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let mut f = File::create(&target_file).unwrap();
            f.write_all(b"initial content").unwrap();
            f.sync_all().unwrap();
        });

        let res = execute(
            "fs.watch",
            vec![Value::String("watched".to_string()), Value::Int(1500)],
        );
        handle.join().unwrap();

        assert!(res.is_ok(), "fs.watch failed: {:?}", res);
        let val = res.unwrap();
        if let Value::List(events) = val {
            assert!(!events.is_empty(), "expected events, got empty list");
            let found = events.iter().any(|e| {
                if let Value::Map(m) = e {
                    m.get("kind") == Some(&Value::String("created".to_string()))
                } else {
                    false
                }
            });
            assert!(found, "expected created event in {:?}", events);
        } else {
            panic!("expected Value::List, got {:?}", val);
        }

        teardown(&ws);
    }

    #[test]
    fn test_fs_watch_outside_workspace_is_denied() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let ws = setup_temp_workspace("outside_denied");

        let res = execute(
            "fs.watch",
            vec![
                Value::String("../outside_escape.txt".to_string()),
                Value::Int(100),
            ],
        );

        assert!(res.is_err(), "expected error for path outside workspace");
        let err = res.unwrap_err();
        assert!(
            err.contains("escape denied"),
            "expected escape denied message, got: {err}"
        );

        teardown(&ws);
    }

    #[test]
    fn test_fs_changed_returns_false_on_timeout() {
        let _guard = crate::workspace::WS_TEST_LOCK.lock().unwrap();
        let ws = setup_temp_workspace("changed_false");

        let res = execute(
            "fs.changed",
            vec![Value::String(".".to_string()), Value::Int(50)],
        );

        assert!(res.is_ok(), "fs.changed failed: {:?}", res);
        assert_eq!(res.unwrap(), Value::Bool(false));

        teardown(&ws);
    }
}
