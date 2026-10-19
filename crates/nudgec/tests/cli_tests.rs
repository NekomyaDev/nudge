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
}
