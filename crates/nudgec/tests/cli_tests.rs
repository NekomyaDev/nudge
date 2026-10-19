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

#[test]
fn test_cli_trace_check_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_trace_check_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let valid_trace_path = temp_dir.join("valid_trace.jsonl");
    fs::write(
        &valid_trace_path,
        r#"{"v":1,"seq":1,"kind":"llm.call","model":"m","params":{},"input":"p","output":"o","tokens":{"in":1,"out":1},"cost_usd":0.001,"repair_round":0,"outcome":"ok","provider":"fake"}
{"v":1,"seq":2,"kind":"tool.call","tool":"web_search","input":["q"],"output":[],"server":"search"}
{"v":1,"seq":3,"kind":"fn.return","fn":"main","output":{"x":1}}
"#,
    )
    .expect("write valid trace");

    let invalid_trace_path = temp_dir.join("invalid_trace.jsonl");
    fs::write(
        &invalid_trace_path,
        r#"{"v":2,"seq":1,"kind":"llm.call"}
{"v":1,"seq":5,"kind":"unknown.kind"}
"#,
    )
    .expect("write invalid trace");

    // Flag after file: nudgec trace-check valid_trace.jsonl --json
    let out1 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg(&valid_trace_path)
        .arg("--json")
        .output()
        .expect("run trace-check");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["valid"], true);
    assert_eq!(parsed1["records"], 3);
    assert!(parsed1["errors"].as_array().unwrap().is_empty());

    // Flag before file: nudgec trace-check --json valid_trace.jsonl
    let out2 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg("--json")
        .arg(&valid_trace_path)
        .output()
        .expect("run trace-check");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["valid"], true);
    assert_eq!(parsed2["records"], 3);

    // Standard mode without --json on valid trace exits 0 and prints OK to stderr
    let out3 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg(&valid_trace_path)
        .output()
        .expect("run trace-check");
    assert!(out3.status.success());
    let stderr3 = String::from_utf8_lossy(&out3.stderr);
    assert!(stderr3.contains("-- trace OK (3 record(s), frozen v1 schema)"));

    // Invalid trace with --json emits JSON and exits 1
    let out4 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg(&invalid_trace_path)
        .arg("--json")
        .output()
        .expect("run trace-check");
    assert_eq!(out4.status.code(), Some(1));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["valid"], false);
    assert_eq!(parsed4["records"], 2);
    assert!(!parsed4["errors"].as_array().unwrap().is_empty());

    // Invalid trace without --json exits 1
    let out5 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg(&invalid_trace_path)
        .output()
        .expect("run trace-check");
    assert_eq!(out5.status.code(), Some(1));

    // Missing file: nudgec trace-check
    let out6 = Command::new(nudgec_bin())
        .arg("trace-check")
        .output()
        .expect("run trace-check");
    assert_eq!(out6.status.code(), Some(64));

    // Missing file with flag: nudgec trace-check --json
    let out7 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg("--json")
        .output()
        .expect("run trace-check");
    assert_eq!(out7.status.code(), Some(64));

    // Unknown flag fail-closed: nudgec trace-check file.jsonl --badflag
    let out8 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg(&valid_trace_path)
        .arg("--badflag")
        .output()
        .expect("run trace-check");
    assert_eq!(out8.status.code(), Some(64));

    // Help includes --json
    let out9 = Command::new(nudgec_bin())
        .arg("trace-check")
        .arg("--help")
        .output()
        .expect("run trace-check help");
    assert!(out9.status.success());
    let help_text = String::from_utf8_lossy(&out9.stdout);
    assert!(help_text.contains("[--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_policy_sweep_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_policy_sweep_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let trace_path = temp_dir.join("decisions.jsonl");
    fs::write(
        &trace_path,
        r#"{"v":1,"seq":1,"kind":"decision.call","model":"m","provider":"laya","questions":{},"answers":{"dept":{"winner":"a","p":0.900,"distribution":{},"confidence":0.900}},"latency_ms":30.0,"outcome":"ok"}
{"v":1,"seq":2,"kind":"decision.call","model":"m","provider":"laya","questions":{},"answers":{"dept":{"winner":"a","p":0.750,"distribution":{},"confidence":0.750}},"latency_ms":35.0,"outcome":"ok"}
{"v":1,"seq":3,"kind":"decision.call","model":"m","provider":"laya","questions":{},"answers":{"dept":{"winner":"a","p":0.550,"distribution":{},"confidence":0.550}},"latency_ms":40.0,"outcome":"ok"}
{"v":1,"seq":4,"kind":"decision.call","model":"m","provider":"laya","questions":{},"answers":{"dept":{"winner":"a","p":0.200,"distribution":{},"confidence":0.200}},"latency_ms":90.0,"outcome":"deadline_missed"}
"#,
    )
    .expect("write decisions trace");

    // Flag after args: nudgec policy-sweep decisions.jsonl --question dept --thresholds 0.5,0.8 --json
    let out1 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg(&trace_path)
        .arg("--question")
        .arg("dept")
        .arg("--thresholds")
        .arg("0.5,0.8")
        .arg("--json")
        .output()
        .expect("run policy-sweep");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["question"], "dept");
    assert_eq!(parsed1["metric"], "confidence");
    assert_eq!(parsed1["total_records"], 4);
    assert_eq!(parsed1["records"], 4);
    assert_eq!(parsed1["distribution"][0]["threshold"], 0.5);
    assert_eq!(parsed1["distribution"][0]["coverage"], 0.75);
    assert_eq!(parsed1["distribution"][0]["records"], 3);
    assert_eq!(parsed1["distribution"][1]["threshold"], 0.8);
    assert_eq!(parsed1["distribution"][1]["coverage"], 0.25);
    assert_eq!(parsed1["distribution"][1]["records"], 1);

    // Flag before file: nudgec policy-sweep --json decisions.jsonl --question dept
    let out2 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg("--json")
        .arg(&trace_path)
        .arg("--question")
        .arg("dept")
        .output()
        .expect("run policy-sweep");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["question"], "dept");
    assert_eq!(parsed2["total_records"], 4);

    // Standard mode without --json prints text
    let out3 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg(&trace_path)
        .arg("--question")
        .arg("dept")
        .arg("--thresholds")
        .arg("0.5,0.8")
        .output()
        .expect("run policy-sweep");
    assert!(out3.status.success());
    let text3 = String::from_utf8_lossy(&out3.stdout);
    assert!(text3.contains("4 recorded decision(s)"));

    // Missing required --question: exits 64
    let out4 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg(&trace_path)
        .output()
        .expect("run policy-sweep");
    assert_eq!(out4.status.code(), Some(64));

    // Missing file: exits 64
    let out5 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .output()
        .expect("run policy-sweep");
    assert_eq!(out5.status.code(), Some(64));

    // Missing file with flags: exits 64
    let out6 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg("--json")
        .arg("--question")
        .arg("dept")
        .output()
        .expect("run policy-sweep");
    assert_eq!(out6.status.code(), Some(64));

    // Unknown flag fail-closed: exits 64
    let out7 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg(&trace_path)
        .arg("--question")
        .arg("dept")
        .arg("--badflag")
        .output()
        .expect("run policy-sweep");
    assert_eq!(out7.status.code(), Some(64));

    // Help includes --json
    let out8 = Command::new(nudgec_bin())
        .arg("policy-sweep")
        .arg("--help")
        .output()
        .expect("run policy-sweep help");
    assert!(out8.status.success());
    let help_text = String::from_utf8_lossy(&out8.stdout);
    assert!(help_text.contains("[--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_check_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_check_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let valid_path = temp_dir.join("valid.ndg");
    fs::write(
        &valid_path,
        r#"
fn greet(name: string) -> string {
    "Hello " + name
}
"#,
    )
    .expect("write valid ndg");

    let invalid_path = temp_dir.join("invalid.ndg");
    fs::write(
        &invalid_path,
        r#"fn bad() -> int costs < 10 {
    1
}
"#,
    )
    .expect("write invalid ndg");

    let syntax_err_path = temp_dir.join("syntax_err.ndg");
    fs::write(&syntax_err_path, "fn broken(").expect("write syntax err ndg");

    // 1. Valid file with flag after file: nudgec check valid.ndg --json
    let out1 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&valid_path)
        .arg("--json")
        .output()
        .expect("run check");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["ok"], true);
    assert_eq!(parsed1["items_count"], 1);
    assert_eq!(parsed1["errors_count"], 0);
    assert!(parsed1["errors"].as_array().unwrap().is_empty());

    // 2. Valid file with flag before file: nudgec check --json valid.ndg
    let out2 = Command::new(nudgec_bin())
        .arg("check")
        .arg("--json")
        .arg(&valid_path)
        .output()
        .expect("run check");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["ok"], true);
    assert_eq!(parsed2["items_count"], 1);
    assert_eq!(parsed2["errors_count"], 0);

    // 3. Valid file standard check (no --json) exits 0 and prints OK to stderr
    let out3 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&valid_path)
        .output()
        .expect("run check");
    assert!(out3.status.success());
    let stderr3 = String::from_utf8_lossy(&out3.stderr);
    assert!(stderr3.contains("-- checked 1 item(s): OK"));

    // 4. Failing file (type/cost error) with --json exits 1 and emits structured JSON
    let out4 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&invalid_path)
        .arg("--json")
        .output()
        .expect("run check");
    assert_eq!(out4.status.code(), Some(1));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["ok"], false);
    assert_eq!(parsed4["items_count"], 1);
    assert_eq!(parsed4["errors_count"], 1);
    let err4 = &parsed4["errors"][0];
    assert_eq!(err4["code"], "E0201");
    assert_eq!(err4["line"], 1);
    assert!(err4["col"].as_u64().unwrap() >= 1);
    assert!(err4["span"].is_object());

    // 5. Failing file without --json exits 1
    let out5 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&invalid_path)
        .output()
        .expect("run check");
    assert_eq!(out5.status.code(), Some(1));
    let stderr5 = String::from_utf8_lossy(&out5.stderr);
    assert!(stderr5.contains("error[E0201]"));

    // 6. Syntax error with --json exits 1 and emits E0002 structured JSON with span null
    let out6 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&syntax_err_path)
        .arg("--json")
        .output()
        .expect("run check");
    assert_eq!(out6.status.code(), Some(1));
    let parsed6: serde_json::Value =
        serde_json::from_slice(&out6.stdout).expect("valid json output");
    assert_eq!(parsed6["ok"], false);
    assert_eq!(parsed6["items_count"], 0);
    assert_eq!(parsed6["errors_count"], 1);
    let err6 = &parsed6["errors"][0];
    assert_eq!(err6["code"], "E0002");
    assert_eq!(err6["line"], 1);
    assert_eq!(err6["col"], 11);
    assert!(err6["span"].is_null());

    // 7. Missing file exits 64 with usage
    let out7 = Command::new(nudgec_bin())
        .arg("check")
        .output()
        .expect("run check");
    assert_eq!(out7.status.code(), Some(64));
    let stderr7 = String::from_utf8_lossy(&out7.stderr);
    assert!(stderr7.contains("usage: nudgec check <file.ndg> [--json] [--watch]"));

    // 8. Missing file with --json exits 64 with usage
    let out8 = Command::new(nudgec_bin())
        .arg("check")
        .arg("--json")
        .output()
        .expect("run check");
    assert_eq!(out8.status.code(), Some(64));
    let stderr8 = String::from_utf8_lossy(&out8.stderr);
    assert!(stderr8.contains("usage: nudgec check <file.ndg> [--json] [--watch]"));

    // 9. Unknown flag exits 64
    let out9 = Command::new(nudgec_bin())
        .arg("check")
        .arg(&valid_path)
        .arg("--badflag")
        .output()
        .expect("run check");
    assert_eq!(out9.status.code(), Some(64));
    let stderr9 = String::from_utf8_lossy(&out9.stderr);
    assert!(stderr9.contains("unknown flag '--badflag'"));

    // 10. Help command exits 0 with usage
    let out10 = Command::new(nudgec_bin())
        .arg("check")
        .arg("--help")
        .output()
        .expect("run check help");
    assert!(out10.status.success());
    let stdout10 = String::from_utf8_lossy(&out10.stdout);
    assert!(stdout10.contains("usage: nudgec check <file.ndg> [--json] [--watch]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_lex_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_lex_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let valid_path = temp_dir.join("valid.ndg");
    fs::write(
        &valid_path,
        r#"fn greet(name: string) -> string {
    "Hello "
}
"#,
    )
    .expect("write valid.ndg");

    let invalid_path = temp_dir.join("invalid.ndg");
    fs::write(&invalid_path, "let x = \"unterminated string").expect("write invalid.ndg");

    // 1. Valid file with flag after file: nudgec lex valid.ndg --json
    let out1 = Command::new(nudgec_bin())
        .arg("lex")
        .arg(&valid_path)
        .arg("--json")
        .output()
        .expect("run lex");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["ok"], true);
    assert!(parsed1["tokens_count"].as_u64().unwrap() > 0);
    let tokens = parsed1["tokens"].as_array().expect("tokens array");
    assert_eq!(tokens[0]["kind"], "Fn");
    assert_eq!(tokens[0]["text"], "fn");
    assert_eq!(tokens[0]["start"], 0);
    assert_eq!(tokens[0]["end"], 2);

    // 2. Valid file with flag before file: nudgec lex --json valid.ndg
    let out2 = Command::new(nudgec_bin())
        .arg("lex")
        .arg("--json")
        .arg(&valid_path)
        .output()
        .expect("run lex");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["ok"], true);
    assert_eq!(parsed2["tokens_count"], parsed1["tokens_count"]);

    // 3. Valid file standard lex (no --json) exits 0 and prints formatted token lines
    let out3 = Command::new(nudgec_bin())
        .arg("lex")
        .arg(&valid_path)
        .output()
        .expect("run lex");
    assert!(out3.status.success());
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    assert!(stdout3.contains("Fn"));
    assert!(stdout3.contains("0..2"));

    // 4. Failing file (lex error) with --json exits 1 and emits structured JSON
    let out4 = Command::new(nudgec_bin())
        .arg("lex")
        .arg(&invalid_path)
        .arg("--json")
        .output()
        .expect("run lex");
    assert_eq!(out4.status.code(), Some(1));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["ok"], false);
    assert_eq!(parsed4["error"]["code"], "E0001");
    assert_eq!(parsed4["error"]["line"], 1);
    assert_eq!(parsed4["error"]["col"], 9);
    assert_eq!(parsed4["error"]["at"], 8);
    assert!(parsed4["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unterminated"));

    // 5. Failing file without --json exits 1 and prints human error to stderr
    let out5 = Command::new(nudgec_bin())
        .arg("lex")
        .arg(&invalid_path)
        .output()
        .expect("run lex");
    assert_eq!(out5.status.code(), Some(1));
    let stderr5 = String::from_utf8_lossy(&out5.stderr);
    assert!(stderr5.contains("error[E0001]"));

    // 6. Missing file exits 64
    let out6 = Command::new(nudgec_bin())
        .arg("lex")
        .output()
        .expect("run lex");
    assert_eq!(out6.status.code(), Some(64));
    let stderr6 = String::from_utf8_lossy(&out6.stderr);
    assert!(stderr6.contains("usage: nudgec lex <file.ndg> [--json]"));

    // 7. Missing file with --json exits 64
    let out7 = Command::new(nudgec_bin())
        .arg("lex")
        .arg("--json")
        .output()
        .expect("run lex");
    assert_eq!(out7.status.code(), Some(64));
    let stderr7 = String::from_utf8_lossy(&out7.stderr);
    assert!(stderr7.contains("usage: nudgec lex <file.ndg> [--json]"));

    // 8. Unknown flag exits 64
    let out8 = Command::new(nudgec_bin())
        .arg("lex")
        .arg(&valid_path)
        .arg("--badflag")
        .output()
        .expect("run lex");
    assert_eq!(out8.status.code(), Some(64));
    let stderr8 = String::from_utf8_lossy(&out8.stderr);
    assert!(stderr8.contains("unknown flag '--badflag'"));

    // 9. Help flag exits 0 with usage
    let out9 = Command::new(nudgec_bin())
        .arg("lex")
        .arg("--help")
        .output()
        .expect("run lex");
    assert!(out9.status.success());
    let stdout9 = String::from_utf8_lossy(&out9.stdout);
    assert!(stdout9.contains("usage: nudgec lex <file.ndg> [--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_parse_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_parse_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let valid_path = temp_dir.join("valid.ndg");
    fs::write(
        &valid_path,
        r#"fn greet(name: string) -> string {
    "Hello "
}
"#,
    )
    .expect("write valid.ndg");

    let syntax_err_path = temp_dir.join("syntax_err.ndg");
    fs::write(&syntax_err_path, "fn bad syntax").expect("write syntax_err.ndg");

    // 1. Valid file with flag after file: nudgec parse valid.ndg --json
    let out1 = Command::new(nudgec_bin())
        .arg("parse")
        .arg(&valid_path)
        .arg("--json")
        .output()
        .expect("run parse");
    assert!(out1.status.success());
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["ok"], true);
    assert_eq!(parsed1["items_count"], 1);
    let items1 = parsed1["items"].as_array().expect("items array");
    assert_eq!(items1.len(), 1);
    assert_eq!(items1[0]["Fn"]["name"], "greet");

    // 2. Valid file with flag before file: nudgec parse --json valid.ndg
    let out2 = Command::new(nudgec_bin())
        .arg("parse")
        .arg("--json")
        .arg(&valid_path)
        .output()
        .expect("run parse");
    assert!(out2.status.success());
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["ok"], true);
    assert_eq!(parsed2["items_count"], 1);

    // 3. Valid file standard parse (no --json) exits 0 and prints items and parsed count
    let out3 = Command::new(nudgec_bin())
        .arg("parse")
        .arg(&valid_path)
        .output()
        .expect("run parse");
    assert!(out3.status.success());
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    assert!(stdout3.contains("Fn"));
    let stderr3 = String::from_utf8_lossy(&out3.stderr);
    assert!(stderr3.contains("-- parsed 1 item(s) OK"));

    // 4. Failing file (syntax error) with --json exits 1 and emits structured JSON
    let out4 = Command::new(nudgec_bin())
        .arg("parse")
        .arg(&syntax_err_path)
        .arg("--json")
        .output()
        .expect("run parse");
    assert_eq!(out4.status.code(), Some(1));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["ok"], false);
    assert_eq!(parsed4["error"]["code"], "E0002");
    assert_eq!(parsed4["error"]["line"], 1);
    assert!(parsed4["error"]["col"].as_u64().unwrap() >= 1);
    assert!(parsed4["error"]["message"].is_string());

    // 5. Failing file without --json exits 1 and prints human error to stderr
    let out5 = Command::new(nudgec_bin())
        .arg("parse")
        .arg(&syntax_err_path)
        .output()
        .expect("run parse");
    assert_eq!(out5.status.code(), Some(1));
    let stderr5 = String::from_utf8_lossy(&out5.stderr);
    assert!(stderr5.contains("error[E0002]"));

    // 6. Missing file exits 64
    let out6 = Command::new(nudgec_bin())
        .arg("parse")
        .output()
        .expect("run parse");
    assert_eq!(out6.status.code(), Some(64));
    let stderr6 = String::from_utf8_lossy(&out6.stderr);
    assert!(stderr6.contains("usage: nudgec parse <file.ndg> [--json]"));

    // 7. Missing file with --json exits 64
    let out7 = Command::new(nudgec_bin())
        .arg("parse")
        .arg("--json")
        .output()
        .expect("run parse");
    assert_eq!(out7.status.code(), Some(64));
    let stderr7 = String::from_utf8_lossy(&out7.stderr);
    assert!(stderr7.contains("usage: nudgec parse <file.ndg> [--json]"));

    // 8. Unknown flag exits 64
    let out8 = Command::new(nudgec_bin())
        .arg("parse")
        .arg(&valid_path)
        .arg("--badflag")
        .output()
        .expect("run parse");
    assert_eq!(out8.status.code(), Some(64));
    let stderr8 = String::from_utf8_lossy(&out8.stderr);
    assert!(stderr8.contains("unknown flag '--badflag'"));

    // 9. Help flag exits 0 with usage
    let out9 = Command::new(nudgec_bin())
        .arg("parse")
        .arg("--help")
        .output()
        .expect("run parse");
    assert!(out9.status.success());
    let stdout9 = String::from_utf8_lossy(&out9.stdout);
    assert!(stdout9.contains("usage: nudgec parse <file.ndg> [--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_usage_includes_json_for_lex_and_parse() {
    let out = Command::new(nudgec_bin())
        .arg("--help")
        .output()
        .expect("run --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nudgec lex   <file.ndg> [--json]   dump token stream"));
    assert!(stdout.contains("nudgec parse <file.ndg> [--json]   dump AST"));
    assert!(stdout.contains("nudgec fmt   <file.ndg> [--check] [--json]   normalize indentation, trim trailing ws, collapse blanks"));
}

#[test]
fn test_cli_fmt_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_fmt_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let dirty_src = "fn f() -> int {\n1\n}\n";
    let clean_src = "fn f() -> int {\n    1\n}\n";
    let invalid_src = "llm\"\"\"unterminated prompt";

    let dirty_path = temp_dir.join("dirty.ndg");
    let clean_path = temp_dir.join("clean.ndg");
    let invalid_path = temp_dir.join("invalid.ndg");

    fs::write(&dirty_path, dirty_src).expect("write dirty ndg");
    fs::write(&clean_path, clean_src).expect("write clean ndg");
    fs::write(&invalid_path, invalid_src).expect("write invalid ndg");

    // 1. --check and --json on dirty file (flags after file)
    let out1 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&dirty_path)
        .arg("--check")
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out1.status.code(), Some(1));
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["file"], dirty_path.to_str().unwrap());
    assert_eq!(parsed1["needs_formatting"], true);
    assert_eq!(parsed1["changed"], false);
    assert_eq!(
        fs::read_to_string(&dirty_path).unwrap(),
        dirty_src,
        "check mode must not write file"
    );

    // 2. --check and --json with flags before file
    let out2 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg("--json")
        .arg("--check")
        .arg(&dirty_path)
        .output()
        .expect("run fmt");
    assert_eq!(out2.status.code(), Some(1));
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["file"], dirty_path.to_str().unwrap());
    assert_eq!(parsed2["needs_formatting"], true);
    assert_eq!(parsed2["changed"], false);

    // 3. Interleaved flags: fmt --check file --json
    let out3 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg("--check")
        .arg(&dirty_path)
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out3.status.code(), Some(1));
    let parsed3: serde_json::Value =
        serde_json::from_slice(&out3.stdout).expect("valid json output");
    assert_eq!(parsed3["file"], dirty_path.to_str().unwrap());
    assert_eq!(parsed3["needs_formatting"], true);
    assert_eq!(parsed3["changed"], false);

    // 4. --check and --json on clean file exits 0
    let out4 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&clean_path)
        .arg("--check")
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out4.status.code(), Some(0));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["file"], clean_path.to_str().unwrap());
    assert_eq!(parsed4["needs_formatting"], false);
    assert_eq!(parsed4["changed"], false);

    // 5. In-place formatting with --json on dirty file
    let to_format_path = temp_dir.join("to_format.ndg");
    fs::write(&to_format_path, dirty_src).expect("write to_format ndg");
    let out5 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&to_format_path)
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out5.status.code(), Some(0));
    let parsed5: serde_json::Value =
        serde_json::from_slice(&out5.stdout).expect("valid json output");
    assert_eq!(parsed5["file"], to_format_path.to_str().unwrap());
    assert_eq!(parsed5["needs_formatting"], false);
    assert_eq!(parsed5["changed"], true);
    assert_eq!(
        fs::read_to_string(&to_format_path).unwrap(),
        clean_src,
        "file must be formatted on disk"
    );

    // 6. In-place formatting with --json on already formatted file
    let out6 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&clean_path)
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out6.status.code(), Some(0));
    let parsed6: serde_json::Value =
        serde_json::from_slice(&out6.stdout).expect("valid json output");
    assert_eq!(parsed6["file"], clean_path.to_str().unwrap());
    assert_eq!(parsed6["needs_formatting"], false);
    assert_eq!(parsed6["changed"], false);

    // 7. Error during formatting with --json exits 1 and emits structured JSON
    let out7 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&invalid_path)
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out7.status.code(), Some(1));
    let parsed7: serde_json::Value =
        serde_json::from_slice(&out7.stdout).expect("valid json output");
    assert_eq!(parsed7["file"], invalid_path.to_str().unwrap());
    assert!(parsed7["error"].as_str().unwrap().contains("cannot format"));

    // 8. Missing file with --json exits 1 and emits structured JSON error
    let missing_path = temp_dir.join("nonexistent.ndg");
    let out8 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&missing_path)
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out8.status.code(), Some(1));
    let parsed8: serde_json::Value =
        serde_json::from_slice(&out8.stdout).expect("valid json output");
    assert_eq!(parsed8["file"], missing_path.to_str().unwrap());
    assert!(parsed8["error"].as_str().unwrap().contains("cannot read"));

    // 9. Standard mode without --json unchanged
    let out9_check = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&dirty_path)
        .arg("--check")
        .output()
        .expect("run fmt");
    assert_eq!(out9_check.status.code(), Some(1));
    let stdout9_check = String::from_utf8_lossy(&out9_check.stdout);
    assert!(stdout9_check.contains("needs formatting"));

    let out9_clean = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&clean_path)
        .arg("--check")
        .output()
        .expect("run fmt");
    assert_eq!(out9_clean.status.code(), Some(0));
    let stdout9_clean = String::from_utf8_lossy(&out9_clean.stdout);
    assert!(stdout9_clean.contains("already formatted"));

    // 10. Missing file positional argument exits 64 with usage
    let out10 = Command::new(nudgec_bin())
        .arg("fmt")
        .output()
        .expect("run fmt");
    assert_eq!(out10.status.code(), Some(64));
    let stderr10 = String::from_utf8_lossy(&out10.stderr);
    assert!(stderr10.contains("usage: nudgec fmt <file.ndg> [--check] [--json]"));

    let out10_flag = Command::new(nudgec_bin())
        .arg("fmt")
        .arg("--json")
        .output()
        .expect("run fmt");
    assert_eq!(out10_flag.status.code(), Some(64));
    let stderr10_flag = String::from_utf8_lossy(&out10_flag.stderr);
    assert!(stderr10_flag.contains("usage: nudgec fmt <file.ndg> [--check] [--json]"));

    // 11. Unknown flag exits 64
    let out11 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg(&clean_path)
        .arg("--unknown")
        .output()
        .expect("run fmt");
    assert_eq!(out11.status.code(), Some(64));
    let stderr11 = String::from_utf8_lossy(&out11.stderr);
    assert!(stderr11.contains("error: unknown flag '--unknown'"));

    // 12. Help flag exits 0
    let out12 = Command::new(nudgec_bin())
        .arg("fmt")
        .arg("--help")
        .output()
        .expect("run fmt");
    assert_eq!(out12.status.code(), Some(0));
    let stdout12 = String::from_utf8_lossy(&out12.stdout);
    assert!(stdout12.contains("usage: nudgec fmt <file.ndg> [--check] [--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_test_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let pass_ndg = temp_dir.join("pass.ndg");
    fs::write(
        &pass_ndg,
        r#"
test "addition works" {
    assert 10 + 20 == 30
}

test "string equality" {
    assert "hello" == "hello"
}
"#,
    )
    .expect("write pass ndg");

    let fail_ndg = temp_dir.join("fail.ndg");
    fs::write(
        &fail_ndg,
        r#"
test "subtest pass" {
    assert 1 == 1
}

test "subtest fail" {
    assert 1 == 2
}
"#,
    )
    .expect("write fail ndg");

    let syntax_err_ndg = temp_dir.join("syntax_err.ndg");
    fs::write(&syntax_err_ndg, "fn broken( {\n").expect("write syntax error ndg");

    let type_err_ndg = temp_dir.join("type_err.ndg");
    fs::write(
        &type_err_ndg,
        r#"
fn broken() -> int {
    "not an int"
}
"#,
    )
    .expect("write type error ndg");

    // 1. Passing tests with flag after: nudgec test pass.ndg --json
    let out1 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&pass_ndg)
        .arg("--json")
        .output()
        .expect("run test --json");
    assert_eq!(out1.status.code(), Some(0));
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out1.stdout).expect("valid json output");
    assert_eq!(parsed1["file"], pass_ndg.to_str().unwrap());
    assert_eq!(parsed1["total"], 2);
    assert_eq!(parsed1["passed"], 2);
    assert_eq!(parsed1["failed"], 0);
    let tests1 = parsed1["tests"].as_array().expect("tests array");
    assert_eq!(tests1.len(), 2);
    assert_eq!(tests1[0]["status"], "passed");
    assert!(tests1[0]["error"].is_null());
    assert_eq!(tests1[1]["status"], "passed");
    assert!(tests1[1]["error"].is_null());

    // 2. Passing tests with flag before: nudgec test --json pass.ndg
    let out2 = Command::new(nudgec_bin())
        .arg("test")
        .arg("--json")
        .arg(&pass_ndg)
        .output()
        .expect("run test --json flag before");
    assert_eq!(out2.status.code(), Some(0));
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out2.stdout).expect("valid json output");
    assert_eq!(parsed2["total"], 2);
    assert_eq!(parsed2["passed"], 2);
    assert_eq!(parsed2["failed"], 0);

    // 3. Passing tests without --json (human output backward compatibility)
    let out3 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&pass_ndg)
        .output()
        .expect("run test without json");
    assert_eq!(out3.status.code(), Some(0));
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    assert!(stdout3.contains("PASS nudge_test_addition_works"));
    assert!(stdout3.contains("PASS nudge_test_string_equality"));
    assert!(stdout3.contains("2/2 tests passed"));

    // 4. Failing tests with --json: exits 1 and emits structured test results
    let out4 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&fail_ndg)
        .arg("--json")
        .output()
        .expect("run test fail --json");
    assert_eq!(out4.status.code(), Some(1));
    let parsed4: serde_json::Value =
        serde_json::from_slice(&out4.stdout).expect("valid json output");
    assert_eq!(parsed4["total"], 2);
    assert_eq!(parsed4["passed"], 1);
    assert_eq!(parsed4["failed"], 1);
    let tests4 = parsed4["tests"].as_array().expect("tests array");
    let pass_item = tests4
        .iter()
        .find(|t| t["name"] == "nudge_test_subtest_pass")
        .expect("find pass test");
    assert_eq!(pass_item["status"], "passed");
    assert!(pass_item["error"].is_null());
    let fail_item = tests4
        .iter()
        .find(|t| t["name"] == "nudge_test_subtest_fail")
        .expect("find fail test");
    assert_eq!(fail_item["status"], "failed");
    assert!(fail_item["error"]
        .as_str()
        .expect("error string")
        .contains("AssertionError"));

    // 5. Failing tests without --json (human output backward compatibility)
    let out5 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&fail_ndg)
        .output()
        .expect("run test fail without json");
    assert_eq!(out5.status.code(), Some(1));
    let stdout5 = String::from_utf8_lossy(&out5.stdout);
    assert!(stdout5.contains("PASS nudge_test_subtest_pass"));
    assert!(stdout5.contains("FAIL nudge_test_subtest_fail"));
    assert!(stdout5.contains("1/2 tests passed"));

    // 6. Compile error with --json: exits 1 and emits structured error JSON
    let out6 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&syntax_err_ndg)
        .arg("--json")
        .output()
        .expect("run test compile err --json");
    assert_eq!(out6.status.code(), Some(1));
    let parsed6: serde_json::Value =
        serde_json::from_slice(&out6.stdout).expect("valid json output");
    assert_eq!(parsed6["file"], syntax_err_ndg.to_str().unwrap());
    assert_eq!(parsed6["total"], 0);
    assert_eq!(parsed6["passed"], 0);
    assert_eq!(parsed6["failed"], 1);
    assert_eq!(parsed6["errors"][0]["code"], "E0002");
    assert!(parsed6["errors"][0]["line"].is_number());
    assert!(parsed6["errors"][0]["col"].is_number());
    assert!(parsed6["errors"][0]["message"].is_string());

    // 7. Check error with --json: exits 1 and emits structured error JSON
    let out7 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&type_err_ndg)
        .arg("--json")
        .output()
        .expect("run test check err --json");
    assert_eq!(out7.status.code(), Some(1));
    let parsed7: serde_json::Value =
        serde_json::from_slice(&out7.stdout).expect("valid json output");
    assert_eq!(parsed7["file"], type_err_ndg.to_str().unwrap());
    assert_eq!(parsed7["total"], 0);
    assert_eq!(parsed7["passed"], 0);
    assert_eq!(parsed7["failed"], 1);
    assert_eq!(parsed7["errors"][0]["code"], "E0201");

    // 8. Missing file positional argument exits 64 with usage
    let out8 = Command::new(nudgec_bin())
        .arg("test")
        .output()
        .expect("run test");
    assert_eq!(out8.status.code(), Some(64));
    let stderr8 = String::from_utf8_lossy(&out8.stderr);
    assert!(stderr8.contains("usage: nudgec test <file.ndg> [--json]"));

    let out8_flag = Command::new(nudgec_bin())
        .arg("test")
        .arg("--json")
        .output()
        .expect("run test");
    assert_eq!(out8_flag.status.code(), Some(64));
    let stderr8_flag = String::from_utf8_lossy(&out8_flag.stderr);
    assert!(stderr8_flag.contains("usage: nudgec test <file.ndg> [--json]"));

    // 9. Unknown flag exits 64
    let out9 = Command::new(nudgec_bin())
        .arg("test")
        .arg(&pass_ndg)
        .arg("--unknown")
        .output()
        .expect("run test unknown flag");
    assert_eq!(out9.status.code(), Some(64));
    let stderr9 = String::from_utf8_lossy(&out9.stderr);
    assert!(stderr9.contains("error: unknown flag '--unknown'"));

    // 10. Help flag exits 0
    let out10 = Command::new(nudgec_bin())
        .arg("test")
        .arg("--help")
        .output()
        .expect("run test help");
    assert_eq!(out10.status.code(), Some(0));
    let stdout10 = String::from_utf8_lossy(&out10.stdout);
    assert!(stdout10.contains("usage: nudgec test <file.ndg> [--json]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_runs_and_auth_json_and_flag_positions() {
    // 1. nudgec runs --help
    let out_runs_help = Command::new(nudgec_bin())
        .arg("runs")
        .arg("--help")
        .output()
        .expect("runs --help");
    assert_eq!(out_runs_help.status.code(), Some(0));
    let stdout_runs_help = String::from_utf8_lossy(&out_runs_help.stdout);
    assert!(stdout_runs_help.contains("usage: nudgec runs [--run <run_id>] [--json]"));

    // 2. nudgec runs --json (lists runs as json)
    let out_runs_json = Command::new(nudgec_bin())
        .arg("runs")
        .arg("--json")
        .output()
        .expect("runs --json");
    assert_eq!(out_runs_json.status.code(), Some(0));
    let parsed_runs: serde_json::Value =
        serde_json::from_slice(&out_runs_json.stdout).expect("valid json output for runs");
    assert_eq!(parsed_runs["ok"], true);
    assert!(parsed_runs["runs"].is_array());

    // 3. nudgec runs --run unknown_id --json (fails with code 1 and error JSON)
    let out_run_missing = Command::new(nudgec_bin())
        .arg("runs")
        .arg("--run")
        .arg("definitely_nonexistent_run_404")
        .arg("--json")
        .output()
        .expect("runs --run missing --json");
    assert_eq!(out_run_missing.status.code(), Some(1));
    let parsed_missing: serde_json::Value =
        serde_json::from_slice(&out_run_missing.stdout).expect("valid error json");
    assert_eq!(parsed_missing["ok"], false);
    assert_eq!(parsed_missing["run_id"], "definitely_nonexistent_run_404");

    // 4. nudgec runs with unknown flag exits 64
    let out_runs_bad_flag = Command::new(nudgec_bin())
        .arg("runs")
        .arg("--bogus-flag")
        .output()
        .expect("runs bad flag");
    assert_eq!(out_runs_bad_flag.status.code(), Some(64));

    // 5. nudgec auth --help
    let out_auth_help = Command::new(nudgec_bin())
        .arg("auth")
        .arg("--help")
        .output()
        .expect("auth --help");
    assert_eq!(out_auth_help.status.code(), Some(0));
    let stdout_auth_help = String::from_utf8_lossy(&out_auth_help.stdout);
    assert!(stdout_auth_help.contains("usage: nudgec auth login [provider]"));
    assert!(stdout_auth_help.contains("nudgec auth status [--json]"));

    // 6. nudgec auth status --json
    let out_auth_json = Command::new(nudgec_bin())
        .arg("auth")
        .arg("status")
        .arg("--json")
        .output()
        .expect("auth status --json");
    assert_eq!(out_auth_json.status.code(), Some(0));
    let parsed_auth: serde_json::Value =
        serde_json::from_slice(&out_auth_json.stdout).expect("valid auth status json");
    assert_eq!(parsed_auth["ok"], true);
    assert!(parsed_auth["credential_store"].is_string());

    // 7. nudgec auth with unknown flag exits 64
    let out_auth_bad = Command::new(nudgec_bin())
        .arg("auth")
        .arg("--bad-option")
        .output()
        .expect("auth bad flag");
    assert_eq!(out_auth_bad.status.code(), Some(64));
}

#[test]
fn test_cli_lint_json_and_flag_positions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_cli_lint_json_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("sample.ndg");
    fs::write(
        &ndg_path,
        r#"
fn main() -> int {
    42
}
"#,
    )
    .expect("write ndg");

    // 1. nudgec lint --help
    let out_help = Command::new(nudgec_bin())
        .arg("lint")
        .arg("--help")
        .output()
        .expect("lint --help");
    assert_eq!(out_help.status.code(), Some(0));
    let stdout_help = String::from_utf8_lossy(&out_help.stdout);
    assert!(stdout_help.contains("usage: nudgec lint <file.ndg> [--fix] [--json]"));

    // 2. nudgec lint <file.ndg> --json
    let out_json1 = Command::new(nudgec_bin())
        .arg("lint")
        .arg(&ndg_path)
        .arg("--json")
        .output()
        .expect("lint --json");
    assert_eq!(out_json1.status.code(), Some(0));
    let parsed1: serde_json::Value =
        serde_json::from_slice(&out_json1.stdout).expect("valid json output");
    assert_eq!(parsed1["ok"], true);
    assert_eq!(parsed1["file"], ndg_path.to_str().unwrap());
    assert_eq!(parsed1["lints_count"], 0);

    // 3. nudgec lint --json <file.ndg> (flag before file)
    let out_json2 = Command::new(nudgec_bin())
        .arg("lint")
        .arg("--json")
        .arg(&ndg_path)
        .output()
        .expect("lint --json before");
    assert_eq!(out_json2.status.code(), Some(0));
    let parsed2: serde_json::Value =
        serde_json::from_slice(&out_json2.stdout).expect("valid json output");
    assert_eq!(parsed2["ok"], true);

    // 4. unknown flag exits 64
    let out_bad = Command::new(nudgec_bin())
        .arg("lint")
        .arg(&ndg_path)
        .arg("--unknown")
        .output()
        .expect("lint unknown flag");
    assert_eq!(out_bad.status.code(), Some(64));

    let _ = fs::remove_dir_all(&temp_dir);
}
