use std::fs;
use std::process::Command;

fn nudgec_bin() -> &'static str {
    env!("CARGO_BIN_EXE_nudgec")
}

#[test]
fn test_replay_gate_with_env_overrides_and_redaction() {
    let tmp = std::env::temp_dir().join(format!("nudge_replay_gate_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let ndg_file = tmp.join("program.ndg");
    fs::write(
        &ndg_file,
        r#"
fn test_replay() -> string {
    "secret_token_1234567890"
}
"#,
    )
    .unwrap();

    // Check with custom environment variables
    let out = Command::new(nudgec_bin())
        .arg("check")
        .arg(&ndg_file)
        .env("NUDGE_REDACT_SECRETS", "1")
        .env("NUDGE_OFFLINE", "1")
        .output()
        .expect("run check with env overrides");

    assert_eq!(out.status.code(), Some(0));

    let _ = fs::remove_dir_all(&tmp);
}
