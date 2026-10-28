//! Shared testing utilities, unique directory generators, and process-wide locks.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Global unified mutex for serializing tests that mutate process-wide global state.
pub static GLOBAL_TEST_LOCK: Mutex<()> = Mutex::new(());

static UNIQUE_SEQ: AtomicU64 = AtomicU64::new(1);

/// Generate a unique temporary directory path incorporating the process ID,
/// thread identifier, timestamp, and a monotonic counter.
pub fn unique_temp_dir(prefix: &str) -> PathBuf {
    let seq = UNIQUE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tid = format!("{:?}", std::thread::current().id())
        .replace("ThreadId(", "")
        .replace(')', "");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}_{}",
        prefix,
        std::process::id(),
        tid,
        ts,
        seq
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_temp_dirs_are_strictly_distinct() {
        let p1 = unique_temp_dir("nudge_test");
        let p2 = unique_temp_dir("nudge_test");
        assert_ne!(p1, p2);
    }
}
