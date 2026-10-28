//! In-memory ephemeral and durable key-value store for Nudge applications (uses MemStore).
//!
//! Provides ultra-fast in-memory storage residing strictly within the process heap
//! for $O(1)$ nanosecond access, with an optional crash-safe Write-Ahead Log (WAL)
//! and atomic snapshot engine (`nvfs.wal` / `nvfs.snap`).

use crate::store::{json_to_value, value_to_json};
use crate::value::Value;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Standard IEEE 802.3 32-bit Cyclic Redundancy Check (CRC32).
/// Zero external crates, deterministic polynomial 0xEDB88320.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (!((crc & 1).wrapping_sub(1))));
        }
    }
    !crc
}

/// Ephemeral in-memory store state with optional WAL and Snapshot durability.
#[derive(Debug, Default)]
pub struct MemStore {
    entries: HashMap<String, Value>,
    persistence_dir: Option<PathBuf>,
    wal_writer: Option<File>,
    wal_seq: u64,
    auto_snapshot_threshold: Option<u64>,
}

impl MemStore {
    pub fn new() -> Self {
        let auto_snapshot_threshold = std::env::var("NUDGE_WAL_AUTO_SNAPSHOT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        let mut store = Self {
            entries: HashMap::new(),
            persistence_dir: None,
            wal_writer: None,
            wal_seq: 0,
            auto_snapshot_threshold,
        };

        if let Some(dir) = Self::detect_default_persistence_dir() {
            let _ = store.enable_persistence(dir);
        }

        store
    }

    /// Configure automatic compaction threshold (takes snapshot every N operations).
    pub fn set_auto_snapshot_threshold(&mut self, threshold: Option<u64>) {
        self.auto_snapshot_threshold = threshold;
    }

    /// Detect if persistence should be auto-enabled from environment or workspace convention.
    pub fn detect_default_persistence_dir() -> Option<PathBuf> {
        if let Ok(dir_str) = std::env::var("NUDGE_STATE_DIR") {
            if !dir_str.is_empty() {
                return Some(PathBuf::from(dir_str));
            }
        }
        if let Ok(persist_flag) = std::env::var("NUDGE_PERSIST_MEM_STORE") {
            if persist_flag == "1" || persist_flag.eq_ignore_ascii_case("true") {
                let ws_root = crate::workspace::get_workspace_root();
                return Some(ws_root.join(".nudge").join("state"));
            }
        }
        None
    }

    /// Enable WAL persistence and recover any existing snapshot and log entries from `dir`.
    pub fn enable_persistence<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, String> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| {
            format!(
                "cannot create persistence directory '{}': {e}",
                dir.display()
            )
        })?;

        self.persistence_dir = Some(dir.clone());
        let replayed = self.recover_from_disk(&dir)?;

        let wal_path = dir.join("nvfs.wal");
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(|e| format!("cannot open WAL '{}': {e}", wal_path.display()))?;

        self.wal_writer = Some(wal_file);
        Ok(replayed)
    }

    /// Disable persistence and close WAL.
    pub fn disable_persistence(&mut self) {
        self.wal_writer = None;
        self.persistence_dir = None;
    }

    /// Check if persistence is active.
    pub fn is_persistent(&self) -> bool {
        self.persistence_dir.is_some()
    }

    /// Get current persistence directory if enabled.
    pub fn persistence_dir(&self) -> Option<&PathBuf> {
        self.persistence_dir.as_ref()
    }

    /// Current sequence number in WAL.
    pub fn current_seq(&self) -> u64 {
        self.wal_seq
    }

    pub fn get(&self, key: &str, default: Value) -> Value {
        self.entries.get(key).cloned().unwrap_or(default)
    }

    pub fn set(&mut self, key: String, val: Value) {
        if self.persistence_dir.is_some() {
            self.wal_seq = self.wal_seq.saturating_add(1);
            let _ = self.append_wal_record("set", Some(&key), Some(&val));
        }
        self.entries.insert(key, val);
        if self.persistence_dir.is_some() {
            if let Some(threshold) = self.auto_snapshot_threshold {
                if threshold > 0 && self.wal_seq.is_multiple_of(threshold) {
                    let _ = self.take_snapshot();
                }
            }
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let removed = self.entries.remove(key).is_some();
        if removed && self.persistence_dir.is_some() {
            self.wal_seq = self.wal_seq.saturating_add(1);
            let _ = self.append_wal_record("del", Some(key), None);
        }
        removed
    }

    pub fn keys(&self) -> Vec<Value> {
        let mut ks: Vec<String> = self.entries.keys().cloned().collect();
        ks.sort();
        ks.into_iter().map(Value::String).collect()
    }

    pub fn clear(&mut self) {
        if self.persistence_dir.is_some() {
            self.wal_seq = self.wal_seq.saturating_add(1);
            let _ = self.append_wal_record("clear", None, None);
        }
        self.entries.clear();
    }

    /// Recover state from snapshot and WAL log.
    pub fn recover_from_disk(&mut self, dir: &Path) -> Result<usize, String> {
        let snap_path = dir.join("nvfs.snap");
        let mut last_seq = 0u64;

        if snap_path.exists() {
            if let Ok(snap_data) = fs::read_to_string(&snap_path) {
                if let Ok(serde_json::Value::Object(map)) =
                    serde_json::from_str::<serde_json::Value>(&snap_data)
                {
                    // Checksum verification on snapshot
                    let snap_valid = if let (Some(expected_crc), Some(entries_val)) = (
                        map.get("crc32").and_then(|v| v.as_u64()).map(|c| c as u32),
                        map.get("entries"),
                    ) {
                        let entries_ser = serde_json::to_string(entries_val).unwrap_or_default();
                        crc32(entries_ser.as_bytes()) == expected_crc
                    } else {
                        true // Legacy snapshot or unchecksummed
                    };

                    if snap_valid {
                        if let Some(s) = map.get("seq").and_then(|v| v.as_u64()) {
                            last_seq = s;
                        }
                        if let Some(serde_json::Value::Object(entries)) = map.get("entries") {
                            for (k, v) in entries {
                                if let Ok(parsed_val) = json_to_value(v.clone()) {
                                    self.entries.insert(k.clone(), parsed_val);
                                }
                            }
                        }
                    }
                }
            }
        }

        let wal_path = dir.join("nvfs.wal");
        let mut replayed = 0;

        if wal_path.exists() {
            let file = File::open(&wal_path)
                .map_err(|e| format!("cannot open WAL '{}': {e}", wal_path.display()))?;
            let reader = BufReader::new(file);

            for line_res in reader.lines() {
                let line = match line_res {
                    Ok(l) => l,
                    Err(_) => break, // Graceful stop at corrupted or incomplete tail
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Check for frame header `CRC32_HEX:JSON` or bare JSON fallback
                let (record_str, expected_crc) =
                    if let Some((crc_hex, json_part)) = trimmed.split_once(':') {
                        if crc_hex.len() == 8 {
                            if let Ok(expected) = u32::from_str_radix(crc_hex, 16) {
                                (json_part, Some(expected))
                            } else {
                                (trimmed, None)
                            }
                        } else {
                            (trimmed, None)
                        }
                    } else {
                        (trimmed, None)
                    };

                // Invariant: Verify CRC32 checksum against bit rot or torn writes
                if let Some(expected) = expected_crc {
                    if crc32(record_str.as_bytes()) != expected {
                        // Torn write or bit rot detected: stop recovery cleanly at safe boundary
                        break;
                    }
                }

                let record: serde_json::Value = match serde_json::from_str(record_str) {
                    Ok(v) => v,
                    Err(_) => break, // Truncated or corrupt record during crash
                };

                let seq = record.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                if seq <= last_seq && last_seq > 0 {
                    continue; // Already included in snapshot
                }

                let op = record.get("op").and_then(|v| v.as_str()).unwrap_or("");
                match op {
                    "set" => {
                        if let (Some(k), Some(v)) =
                            (record.get("k").and_then(|k| k.as_str()), record.get("v"))
                        {
                            if let Ok(val) = json_to_value(v.clone()) {
                                self.entries.insert(k.to_string(), val);
                                replayed += 1;
                            }
                        }
                    }
                    "del" => {
                        if let Some(k) = record.get("k").and_then(|k| k.as_str()) {
                            self.entries.remove(k);
                            replayed += 1;
                        }
                    }
                    "clear" => {
                        self.entries.clear();
                        replayed += 1;
                    }
                    _ => {}
                }

                last_seq = last_seq.max(seq);
            }
        }

        self.wal_seq = last_seq;
        Ok(replayed)
    }

    /// Compact in-memory state into a fresh snapshot (`nvfs.snap`) and truncate WAL.
    pub fn take_snapshot(&mut self) -> Result<(), String> {
        let dir = self
            .persistence_dir
            .clone()
            .ok_or("persistence is not enabled on this MemStore")?;

        let mut entries_json = serde_json::Map::with_capacity(self.entries.len());
        for (k, v) in &self.entries {
            if let Ok(jv) = value_to_json(v) {
                entries_json.insert(k.clone(), jv);
            }
        }

        let entries_ser = serde_json::to_string(&entries_json).unwrap_or_default();
        let checksum = crc32(entries_ser.as_bytes());

        let snap_obj = serde_json::json!({
            "version": 1,
            "seq": self.wal_seq,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "crc32": checksum,
            "entries": entries_json,
        });

        let serialized = serde_json::to_string_pretty(&snap_obj)
            .map_err(|e| format!("cannot serialize snapshot: {e}"))?;

        let tmp_path = dir.join(format!(
            ".nvfs.snap.tmp.{}.{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        {
            let mut file = File::create(&tmp_path)
                .map_err(|e| format!("cannot create temporary snapshot: {e}"))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| format!("cannot write snapshot: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("cannot sync snapshot to disk: {e}"))?;
        }

        let final_path = dir.join("nvfs.snap");
        fs::rename(&tmp_path, &final_path)
            .map_err(|e| format!("cannot commit snapshot rename: {e}"))?;

        // Truncate WAL to empty
        let wal_path = dir.join("nvfs.wal");
        let wal_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&wal_path)
            .map_err(|e| format!("cannot truncate WAL: {e}"))?;

        self.wal_writer = Some(wal_file);
        Ok(())
    }

    fn append_wal_record(
        &mut self,
        op: &str,
        key: Option<&str>,
        val: Option<&Value>,
    ) -> Result<(), String> {
        let writer = match &mut self.wal_writer {
            Some(w) => w,
            None => return Ok(()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut record = serde_json::json!({
            "seq": self.wal_seq,
            "op": op,
            "t": now,
        });

        if let Some(k) = key {
            record["k"] = serde_json::Value::String(k.to_string());
        }

        if let Some(v) = val {
            if let Ok(jv) = value_to_json(v) {
                record["v"] = jv;
            }
        }

        let line = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        let checksum = crc32(line.as_bytes());
        writeln!(writer, "{:08x}:{}", checksum, line)
            .map_err(|e| format!("cannot write to WAL: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("cannot flush WAL: {e}"))?;

        Ok(())
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
    functions.insert("mem.persist".to_string(), Value::Native("mem.persist"));
    functions.insert("mem.snapshot".to_string(), Value::Native("mem.snapshot"));
    functions.insert("mem.recover".to_string(), Value::Native("mem.recover"));
    functions.insert("mem.status".to_string(), Value::Native("mem.status"));
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

        "mem.persist" => {
            let dir = if let Some(arg) = args.first() {
                match arg {
                    Value::String(p) => PathBuf::from(p),
                    Value::Bool(true) => {
                        let ws_root = crate::workspace::get_workspace_root();
                        ws_root.join(".nudge").join("state")
                    }
                    Value::Bool(false) => {
                        with_mem_store(|s| {
                            s.disable_persistence();
                            Ok(Value::Bool(false))
                        })?;
                        return Ok(Value::Bool(false));
                    }
                    _ => return Err("mem.persist requires a directory path or boolean".into()),
                }
            } else {
                let ws_root = crate::workspace::get_workspace_root();
                ws_root.join(".nudge").join("state")
            };

            with_mem_store(|s| {
                let replayed = s.enable_persistence(dir)?;
                Ok(Value::Int(replayed as i64))
            })
        }

        "mem.snapshot" => with_mem_store(|s| {
            s.take_snapshot()?;
            Ok(Value::Bool(true))
        }),

        "mem.recover" => {
            let dir_opt = args.first().and_then(|a| match a {
                Value::String(p) => Some(PathBuf::from(p)),
                _ => None,
            });

            with_mem_store(|s| {
                let target_dir = dir_opt
                    .or_else(|| s.persistence_dir().cloned())
                    .ok_or("no persistence directory configured")?;
                let count = s.recover_from_disk(&target_dir)?;
                Ok(Value::Int(count as i64))
            })
        }

        "mem.status" => with_mem_store(|s| {
            let mut status = HashMap::new();
            status.insert("persisted".to_string(), Value::Bool(s.is_persistent()));
            status.insert(
                "dir".to_string(),
                Value::String(
                    s.persistence_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
            );
            status.insert("count".to_string(), Value::Int(s.entries.len() as i64));
            status.insert("seq".to_string(), Value::Int(s.current_seq() as i64));
            Ok(Value::Map(status))
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

    #[test]
    fn test_mem_store_wal_and_snapshot_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("nudge_nvfs_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let mut store = MemStore::new();
        let replayed = store
            .enable_persistence(&temp_dir)
            .expect("enable persistence succeeds");
        assert_eq!(replayed, 0);

        store.set("state/route".into(), Value::String("/dashboard".into()));
        store.set(
            "state/auth_token".into(),
            Value::String("sec_xyz123".into()),
        );
        store.set("state/retry_count".into(), Value::Int(3));

        // Crash simulation: simulate a new process opening the same state directory
        let mut recovered_store = MemStore::new();
        let count = recovered_store
            .enable_persistence(&temp_dir)
            .expect("recovery succeeds");
        assert_eq!(count, 3);
        assert_eq!(
            recovered_store.get("state/auth_token", Value::None),
            Value::String("sec_xyz123".into())
        );
        assert_eq!(
            recovered_store.get("state/retry_count", Value::None),
            Value::Int(3)
        );

        // Delete an entry and take snapshot
        recovered_store.delete("state/auth_token");
        recovered_store.take_snapshot().expect("snapshot succeeds");

        // Open third process after snapshot
        let mut post_snap_store = MemStore::new();
        post_snap_store
            .enable_persistence(&temp_dir)
            .expect("post snap recovery succeeds");
        assert!(!post_snap_store.has("state/auth_token"));
        assert_eq!(
            post_snap_store.get("state/route", Value::None),
            Value::String("/dashboard".into())
        );
        assert_eq!(post_snap_store.keys().len(), 2);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
