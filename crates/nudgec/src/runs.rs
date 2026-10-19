//! `nudgec runs` (C8): inspect the local run store that agent
//! checkpoint/resume already writes (.nudge/runs/<run_id>/ with
//! `program`, `trace`, `checkpoint.json`). Listing and detail views —
//! the SQLite backend remains future work; the JSON store is the
//! compatibility boundary.

use crate::json::{parse, Json};
use std::path::Path;

fn runs_dir() -> std::path::PathBuf {
    Path::new(".nudge").join("runs")
}

fn run_ids() -> Vec<String> {
    let dir = runs_dir();
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

fn checkpoint_fields(dir: &Path) -> Vec<(String, String)> {
    let Ok(src) = std::fs::read_to_string(dir.join("checkpoint.json")) else {
        return Vec::new();
    };
    let Ok(Json::Obj(fields)) = parse(&src) else {
        return Vec::new();
    };
    fields
        .iter()
        .map(|(k, v)| (k.clone(), crate::json::dumps(v)))
        .collect()
}

pub fn run(detail: Option<&str>) -> i32 {
    if let Some(id) = detail {
        let dir = runs_dir().join(id);
        if !dir.is_dir() {
            eprintln!("error: unknown run_id '{id}' (no {}/)", dir.display());
            return 1;
        }
        println!("run {id}");
        for name in ["program", "trace"] {
            if let Ok(v) = std::fs::read_to_string(dir.join(name)) {
                let v = v.trim();
                println!("  {name}: {v}");
            }
        }
        let fields = checkpoint_fields(&dir);
        if !fields.is_empty() {
            println!("  checkpoint state:");
            for (k, v) in fields {
                println!("    {k} = {v}");
            }
        }
        if let Ok(path) = std::fs::read_to_string(dir.join("trace")) {
            if let Ok(src) = std::fs::read_to_string(path.trim()) {
                let records = src.lines().filter(|l| !l.trim().is_empty()).count();
                println!("  trace records: {records}");
            }
        }
        return 0;
    }
    let ids = run_ids();
    if ids.is_empty() {
        println!("no runs recorded (agent state writes create .nudge/runs/)");
        return 0;
    }
    println!("{} run(s) in .nudge/runs/:", ids.len());
    for id in &ids {
        let dir = runs_dir().join(id);
        let has = |n: &str| dir.join(n).exists();
        println!(
            "  {id}  program={} trace={} checkpoint={}",
            has("program"),
            has("trace"),
            has("checkpoint.json")
        );
    }
    0
}

pub fn run_json(detail: Option<&str>) -> (i32, String) {
    if let Some(id) = detail {
        let dir = runs_dir().join(id);
        if !dir.is_dir() {
            let out = serde_json::json!({
                "ok": false,
                "error": format!("unknown run_id '{id}' (no {}/)", dir.display()),
                "run_id": id,
            });
            return (
                1,
                serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string()),
            );
        }
        let program = std::fs::read_to_string(dir.join("program"))
            .ok()
            .map(|s| s.trim().to_string());
        let trace = std::fs::read_to_string(dir.join("trace"))
            .ok()
            .map(|s| s.trim().to_string());
        let checkpoint_raw = std::fs::read_to_string(dir.join("checkpoint.json")).ok();
        let checkpoint_json = checkpoint_raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        let trace_records = if let Some(ref t_path) = trace {
            std::fs::read_to_string(t_path)
                .ok()
                .map(|src| src.lines().filter(|l| !l.trim().is_empty()).count())
        } else {
            None
        };

        let out = serde_json::json!({
            "ok": true,
            "run_id": id,
            "program": program,
            "trace": trace,
            "trace_records": trace_records,
            "checkpoint": checkpoint_json,
        });
        (
            0,
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string()),
        )
    } else {
        let ids = run_ids();
        let mut runs_list = Vec::new();
        for id in &ids {
            let dir = runs_dir().join(id);
            let has_program = dir.join("program").exists();
            let has_trace = dir.join("trace").exists();
            let has_checkpoint = dir.join("checkpoint.json").exists();
            runs_list.push(serde_json::json!({
                "run_id": id,
                "has_program": has_program,
                "has_trace": has_trace,
                "has_checkpoint": has_checkpoint,
            }));
        }
        let out = serde_json::json!({
            "ok": true,
            "total": ids.len(),
            "runs": runs_list,
        });
        (
            0,
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_run_reports_cleanly() {
        let code = run(Some("definitely-missing-run"));
        assert_eq!(code, 1);
    }

    #[test]
    fn empty_store_lists_nothing() {
        // runs_dir may not exist in the test cwd — must not panic
        let _ = run(None);
    }

    #[test]
    fn run_json_smoke_test() {
        let (code, json_str) = run_json(None);
        assert_eq!(code, 0);
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(val["ok"], true);
        assert!(val["runs"].is_array());

        let (code_missing, json_missing) = run_json(Some("non_existent_run_id"));
        assert_eq!(code_missing, 1);
        let val_missing: serde_json::Value = serde_json::from_str(&json_missing).unwrap();
        assert_eq!(val_missing["ok"], false);
        assert!(val_missing["error"]
            .as_str()
            .unwrap()
            .contains("unknown run_id"));
    }
}
