use std::fs;
use std::process::Command;

#[test]
fn test_standalone_rejects_invalid_program() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_invalid_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("invalid.ndg");
    let bin_path = temp_dir.join("invalid_bin");

    // Syntax error in program
    fs::write(&ndg_path, "fn broken( { 123 }").expect("write ndg");
    let res = nudgec::standalone::build_standalone(&ndg_path, "main", &bin_path);
    assert!(res.is_err());
    assert!(!bin_path.exists());

    // Type error in program (undeclared variable)
    fs::write(&ndg_path, "fn main() -> int { nonexistent }").expect("write ndg");
    let res = nudgec::standalone::build_standalone(&ndg_path, "main", &bin_path);
    assert!(res.is_err());
    assert!(!bin_path.exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_standalone_bundle_serialization_round_trip() {
    let items = nudgec::compile("fn main() -> int { 42 }").expect("compile");
    let bundle = nudgec::ast::StandaloneBundle {
        entry: "main".to_string(),
        items,
    };
    let encoded = bundle.encode_payload().expect("encode payload");
    let decoded = nudgec::ast::StandaloneBundle::extract_from_slice(&encoded)
        .expect("extract from slice")
        .expect("expected Some bundle");
    assert_eq!(decoded, bundle);
}

#[test]
fn test_standalone_locate_runtime_error_when_missing() {
    let res = nudgec::standalone::locate_agent_runtime();
    if let Err(e) = res {
        assert_eq!(
            e,
            "nudge-agent runtime not found; set NUDGE_AGENT_RUNTIME or install nudge-agent next to nudgec"
        );
    }
}

#[test]
fn test_standalone_end_to_end_when_enabled() {
    if std::env::var("NUDGE_STANDALONE_TEST").as_deref() != Ok("1") {
        // Skip unless explicitly enabled
        return;
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_e2e_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let ndg_path = temp_dir.join("add.ndg");
    let bin_path = temp_dir.join("add_bin");

    fs::write(
        &ndg_path,
        r#"
fn add(a: int, b: int) -> int {
    a + b
}

fn main() -> int {
    add(20, 22)
}
"#,
    )
    .expect("write ndg");

    let build_res = nudgec::standalone::build_standalone(&ndg_path, "main", &bin_path);
    assert!(
        build_res.is_ok(),
        "build_standalone failed: {:?}",
        build_res.err()
    );
    assert!(
        bin_path.exists(),
        "binary should exist at {}",
        bin_path.display()
    );

    let output = Command::new(&bin_path)
        .output()
        .expect("failed to execute produced binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "42");

    let _ = fs::remove_dir_all(&temp_dir);
}
