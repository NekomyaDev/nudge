//! `nudgec check --watch` (B4): re-run the checker on every save. A
//! 300 ms mtime poll — no dependencies, no filesystem-event plumbing —
//! and each pass spawns `nudgec check <file>` as a subprocess so the
//! watched loop can never drift from the real checker (flags, lints and
//! error formatting included). Ctrl-C stops it.

use std::process::Command;
use std::time::{Duration, SystemTime};

const POLL: Duration = Duration::from_millis(300);

fn mtime(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn run_check(path: &str) -> bool {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot locate nudgec: {e}");
            return false;
        }
    };
    run_check_with(&exe, path)
}

fn run_check_with(exe: &std::path::Path, path: &str) -> bool {
    Command::new(exe)
        .args(["check", path])
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

pub fn run(path: &str) -> ! {
    if std::fs::metadata(path).is_err() {
        eprintln!("error: cannot read {path}: no such file");
        std::process::exit(1);
    }
    let mut last = mtime(path);
    eprintln!("watching {path} — checking every change (Ctrl-C to stop)");
    run_check(path);
    loop {
        std::thread::sleep(POLL);
        let now = mtime(path);
        if now != last {
            last = now;
            eprintln!("── change detected, re-checking …");
            run_check(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NB: no test spawns `current_exe` — under `cargo test` that is the
    // test harness, and invoking it with CLI args recurses into the
    // suite (a hang, not a failure). The plumbing is tested with stub
    // executables instead; the real binary path is covered by smoke runs.

    #[test]
    fn check_plumbing_reports_exit_status() {
        let ok = run_check_with(std::path::Path::new("/bin/true"), "whatever");
        assert!(ok);
        let fail = run_check_with(std::path::Path::new("/bin/false"), "whatever");
        assert!(!fail);
        let missing = run_check_with(std::path::Path::new("/nonexistent/binary"), "w");
        assert!(!missing);
    }

    #[test]
    fn missing_files_are_reported_before_looping() {
        assert!(mtime("/nonexistent/nope.ndg").is_none());
    }
}
