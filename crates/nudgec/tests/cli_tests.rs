use std::fs;
use std::process::Command;

fn nudgec_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_nudgec"))
}

#[test]
fn test_cli_cost_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_cost_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("test.ndg");
    fs::write(
        &ndg_path,
        r#"
fn test_fn(s: string) -> string uses LLM {
    llm"""Hello {s}""" with { model: "fake", budget: 0.01 USD }
}
"#,
    )
    .expect("write ndg");

    // Flag after file: nudgec cost file.ndg --json
    let out1 = Command::new(nudgec_bin())
        .arg("cost")
        .arg(&ndg_path)
        .arg("--json")
        .output()
        .expect("run cost");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["functions"][0]["name"], "test_fn");

    // Flag before file: nudgec cost --json file.ndg
    let out2 = Command::new(nudgec_bin())
        .arg("cost")
        .arg("--json")
        .arg(&ndg_path)
        .output()
        .expect("run cost");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["functions"][0]["name"], "test_fn");

    // Missing file: nudgec cost
    let out3 = Command::new(nudgec_bin())
        .arg("cost")
        .output()
        .expect("run cost");
    assert_eq!(out3.status.code(), Some(64));

    // Missing file with flag: nudgec cost --json
    let out4 = Command::new(nudgec_bin())
        .arg("cost")
        .arg("--json")
        .output()
        .expect("run cost");
    assert_eq!(out4.status.code(), Some(64));

    // Unknown flag fail-closed: nudgec cost file.ndg --badflag
    let out5 = Command::new(nudgec_bin())
        .arg("cost")
        .arg(&ndg_path)
        .arg("--badflag")
        .output()
        .expect("run cost");
    assert_eq!(out5.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_explain_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_explain_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let trace_path = temp_dir.join("trace.jsonl");
    fs::write(
        &trace_path,
        r#"{"v":1,"seq":1,"ts":1000,"kind":"run_start","run_id":"r1","program":"p"}
{"v":1,"seq":2,"ts":1001,"kind":"llm.call","run_id":"r1","fn":"f","prompt":"hi","response":"there","tokens_in":5,"tokens_out":5,"cost_usd":0.001,"model":"m"}
{"v":1,"seq":3,"ts":1002,"kind":"run_end","run_id":"r1","status":"ok","value":null}
"#,
    )
    .expect("write trace");

    // Flag after file: nudgec explain trace.jsonl --json
    let out1 = Command::new(nudgec_bin())
        .arg("explain")
        .arg(&trace_path)
        .arg("--json")
        .output()
        .expect("run explain");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["llm_calls"], 1);

    // Flag before file: nudgec explain --json trace.jsonl
    let out2 = Command::new(nudgec_bin())
        .arg("explain")
        .arg("--json")
        .arg(&trace_path)
        .output()
        .expect("run explain");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["llm_calls"], 1);

    // Missing file: nudgec explain
    let out3 = Command::new(nudgec_bin())
        .arg("explain")
        .output()
        .expect("run explain");
    assert_eq!(out3.status.code(), Some(64));

    // Unknown flag fail-closed
    let out4 = Command::new(nudgec_bin())
        .arg("explain")
        .arg(&trace_path)
        .arg("--bogus")
        .output()
        .expect("run explain");
    assert_eq!(out4.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_trace_diff_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_diff_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let trace_a = temp_dir.join("a.jsonl");
    let trace_b = temp_dir.join("b.jsonl");
    let sample = r#"{"v":1,"seq":1,"ts":1000,"kind":"run_start","run_id":"r1","program":"p"}
{"v":1,"seq":2,"ts":1001,"kind":"run_end","run_id":"r1","status":"ok","value":null}
"#;
    fs::write(&trace_a, sample).expect("write trace a");
    fs::write(&trace_b, sample).expect("write trace b");

    // Flag after files: nudgec trace-diff a.jsonl b.jsonl --json
    let out1 = Command::new(nudgec_bin())
        .arg("trace-diff")
        .arg(&trace_a)
        .arg(&trace_b)
        .arg("--json")
        .output()
        .expect("run diff");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["has_regression"], false);

    // Flag before files: nudgec trace-diff --json a.jsonl b.jsonl
    let out2 = Command::new(nudgec_bin())
        .arg("trace-diff")
        .arg("--json")
        .arg(&trace_a)
        .arg(&trace_b)
        .output()
        .expect("run diff");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["has_regression"], false);

    // Insufficient args: nudgec trace-diff a.jsonl
    let out3 = Command::new(nudgec_bin())
        .arg("trace-diff")
        .arg(&trace_a)
        .output()
        .expect("run diff");
    assert_eq!(out3.status.code(), Some(64));

    // Unknown flag fail-closed
    let out4 = Command::new(nudgec_bin())
        .arg("trace-diff")
        .arg(&trace_a)
        .arg(&trace_b)
        .arg("--invalid")
        .output()
        .expect("run diff");
    assert_eq!(out4.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_compare_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_compare_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let res_a = temp_dir.join("res_a.jsonl");
    let res_b = temp_dir.join("res_b.jsonl");
    fs::write(
        &res_a,
        r#"{"row": 1, "ok": false, "expected": "x", "got": "y"}
{"row": 2, "ok": true, "expected": "y", "got": "y"}
"#,
    )
    .expect("write a");
    fs::write(
        &res_b,
        r#"{"row": 1, "ok": true, "expected": "x", "got": "x"}
{"row": 2, "ok": false, "expected": "y", "got": "z"}
"#,
    )
    .expect("write b");

    // Flag after files: nudgec compare res_a.jsonl res_b.jsonl --json
    let out1 = Command::new(nudgec_bin())
        .arg("compare")
        .arg(&res_a)
        .arg(&res_b)
        .arg("--json")
        .output()
        .expect("run compare");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["base"]["passed"], 1);
    assert_eq!(parsed1["candidate"]["passed"], 1);

    // Flag before files: nudgec compare --json res_a.jsonl res_b.jsonl
    let out2 = Command::new(nudgec_bin())
        .arg("compare")
        .arg("--json")
        .arg(&res_a)
        .arg(&res_b)
        .output()
        .expect("run compare");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["base"]["passed"], 1);

    // Insufficient args: nudgec compare res_a.jsonl
    let out3 = Command::new(nudgec_bin())
        .arg("compare")
        .arg(&res_a)
        .output()
        .expect("run compare");
    assert_eq!(out3.status.code(), Some(64));

    // Unknown flag fail-closed
    let out4 = Command::new(nudgec_bin())
        .arg("compare")
        .arg(&res_a)
        .arg(&res_b)
        .arg("--bogus")
        .output()
        .expect("run compare");
    assert_eq!(out4.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_eval_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_eval_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("eval_prog.ndg");
    let data_path = temp_dir.join("dataset.jsonl");

    fs::write(
        &ndg_path,
        r#"
fn score(x: int) -> int {
    x * 2
}
"#,
    )
    .expect("write ndg");

    fs::write(
        &data_path,
        r#"{"input": 2, "expected": 4}
{"input": 3, "expected": 6}
"#,
    )
    .expect("write dataset");

    // Flag after args: nudgec eval file.ndg --dataset data.jsonl --fn score --json
    let out1 = Command::new(nudgec_bin())
        .arg("eval")
        .arg(&ndg_path)
        .arg("--dataset")
        .arg(&data_path)
        .arg("--fn")
        .arg("score")
        .arg("--json")
        .output()
        .expect("run eval");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["total"], 2);
    assert_eq!(parsed1["passed"], 2);

    // Flag before file: nudgec eval --json file.ndg --dataset data.jsonl --fn score
    let out2 = Command::new(nudgec_bin())
        .arg("eval")
        .arg("--json")
        .arg(&ndg_path)
        .arg("--dataset")
        .arg(&data_path)
        .arg("--fn")
        .arg("score")
        .output()
        .expect("run eval");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["total"], 2);
    assert_eq!(parsed2["passed"], 2);

    // Missing --dataset fail-closed
    let out3 = Command::new(nudgec_bin())
        .arg("eval")
        .arg(&ndg_path)
        .output()
        .expect("run eval");
    assert_eq!(out3.status.code(), Some(64));

    // Unknown eval argument fail-closed
    let out4 = Command::new(nudgec_bin())
        .arg("eval")
        .arg(&ndg_path)
        .arg("--dataset")
        .arg(&data_path)
        .arg("--unknown")
        .output()
        .expect("run eval");
    assert_eq!(out4.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_fuzz_json_and_fail_closed() {
    // nudgec fuzz --json --iterations 2
    let out1 = Command::new(nudgec_bin())
        .arg("fuzz")
        .arg("--json")
        .arg("--iterations")
        .arg("2")
        .output()
        .expect("run fuzz");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["iterations"], 2);

    // Unknown flag fail-closed
    let out2 = Command::new(nudgec_bin())
        .arg("fuzz")
        .arg("--badflag")
        .output()
        .expect("run fuzz");
    assert_eq!(out2.status.code(), Some(64));

    // Help includes --json
    let out3 = Command::new(nudgec_bin())
        .arg("fuzz")
        .arg("--help")
        .output()
        .expect("run fuzz help");
    assert!(out3.status.success());
    let help_text = String::from_utf8_lossy(&out3.stdout);
    assert!(help_text.contains("[--json]"));
}

#[test]
fn test_cli_lint_and_run_flag_order() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_lint_run_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("clean.ndg");
    fs::write(
        &ndg_path,
        r#"
fn main() -> int {
    42
}
"#,
    )
    .expect("write ndg");

    // lint with --fix before file
    let out1 = Command::new(nudgec_bin())
        .arg("lint")
        .arg("--fix")
        .arg(&ndg_path)
        .output()
        .expect("run lint");
    assert!(out1.status.success());

    // run with --fn before file
    let out2 = Command::new(nudgec_bin())
        .arg("run")
        .arg("--fn")
        .arg("main")
        .arg(&ndg_path)
        .output()
        .expect("run run");
    assert!(out2.status.success());

    let _ = fs::remove_dir_all(&temp_dir);
}
