use std::fs;
use std::process::Command;

#[test]
fn test_standalone_generator_main_rs_content() {
    let main_rs = nudgec::standalone::generate_main_rs("/path/to/test.ndg", "run");
    assert!(main_rs.contains("include_str!(\"/path/to/test.ndg\")"));
    assert!(main_rs.contains("const ENTRY: &str = \"run\";"));
    assert!(main_rs.contains("nudgec::compile(SRC)"));
    assert!(main_rs.contains("nudgec::check::check(&items)"));
    assert!(main_rs.contains("nudgec::eval::parse_cli_arg(&a)"));
    assert!(main_rs.contains("nudgec::eval::run_program(&items, ENTRY, &cli_args)"));
    assert!(main_rs.contains("nudgec::nudge_runtime::trace::enable_default_sink()"));
}

#[test]
fn test_standalone_generator_cargo_toml_content() {
    let cargo_toml = nudgec::standalone::generate_cargo_toml("demo_app", "/tmp/nudgec");
    assert!(cargo_toml.contains("name = \"demo_app\""));
    assert!(cargo_toml.contains("path = \"/tmp/nudgec\""));
    assert!(cargo_toml.contains("opt-level = \"z\""));
    assert!(cargo_toml.contains("lto = true"));
    assert!(cargo_toml.contains("strip = true"));
}

#[test]
fn test_standalone_sanitize_crate_name() {
    assert_eq!(
        nudgec::standalone::sanitize_crate_name("simple_add"),
        "simple_add"
    );
    assert_eq!(
        nudgec::standalone::sanitize_crate_name("test-prog"),
        "test-prog"
    );
    assert_eq!(
        nudgec::standalone::sanitize_crate_name("99bottles"),
        "app_99bottles"
    );
    assert_eq!(
        nudgec::standalone::sanitize_crate_name("hello.world"),
        "hello_world"
    );
    assert_eq!(nudgec::standalone::sanitize_crate_name(""), "app_");
}

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
fn test_standalone_end_to_end_when_enabled() {
    if std::env::var("NUDGE_STANDALONE_TEST").as_deref() != Ok("1") {
        // Skip slow full nested cargo build unless explicitly enabled
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
