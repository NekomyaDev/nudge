//! `nudgec clean` — purge local cache and temporary trace files.
//!
//! Cleans:
//! - `.nudge/cache`
//! - `.nudge/traces`
//! - `traces/` (if it contains temporary `.jsonl` trace outputs)
//! - `out/` (intermediate transpiled Python/TS outputs)

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq)]
pub struct CleanReport {
    pub files_removed: usize,
    pub bytes_reclaimed: u64,
    pub dirs_removed: usize,
}

impl CleanReport {
    pub fn to_text(&self) -> String {
        let size_str = format_bytes(self.bytes_reclaimed);
        format!(
            "cleaned {} file(s) across {} directory(ies) ({size_str} reclaimed)",
            self.files_removed, self.dirs_removed
        )
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn clean_dir_recursive(dir: &Path, report: &mut CleanReport) {
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                clean_dir_recursive(&path, report);
                if fs::remove_dir(&path).is_ok() {
                    report.dirs_removed += 1;
                }
            } else if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    report.bytes_reclaimed += meta.len();
                }
                if fs::remove_file(&path).is_ok() {
                    report.files_removed += 1;
                }
            }
        }
    }
}

/// Run clean starting at the specified workspace root.
pub fn run_clean_at(root: &Path) -> Result<CleanReport, String> {
    let mut report = CleanReport::default();

    let targets: Vec<PathBuf> = vec![
        root.join(".nudge").join("cache"),
        root.join(".nudge").join("traces"),
        root.join("out"),
    ];

    for target in targets {
        if target.exists() {
            clean_dir_recursive(&target, &mut report);
            if fs::remove_dir(&target).is_ok() {
                report.dirs_removed += 1;
            }
        }
    }

    Ok(report)
}

/// Run clean from current working directory.
pub fn run() -> Result<CleanReport, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot determine current dir: {e}"))?;
    run_clean_at(&cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_cache_and_reports_bytes() {
        let tmp = std::env::temp_dir().join(format!("nudge_clean_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".nudge/cache")).unwrap();
        fs::create_dir_all(tmp.join(".nudge/traces")).unwrap();
        fs::create_dir_all(tmp.join("out")).unwrap();

        fs::write(tmp.join(".nudge/cache/item.bin"), "cached data payload").unwrap();
        fs::write(tmp.join(".nudge/traces/trace1.jsonl"), "{\"v\":1}\n").unwrap();
        fs::write(tmp.join("out/agent.py"), "print(1)\n").unwrap();

        let report = run_clean_at(&tmp).unwrap();
        assert_eq!(report.files_removed, 3);
        assert!(report.bytes_reclaimed > 0);
        assert!(!tmp.join(".nudge/cache").exists());
        assert!(!tmp.join(".nudge/traces").exists());
        assert!(!tmp.join("out").exists());

        let text = report.to_text();
        assert!(text.contains("cleaned 3 file(s)"));
        assert!(text.contains("reclaimed"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
