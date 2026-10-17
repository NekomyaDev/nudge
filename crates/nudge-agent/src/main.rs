//! Minimal runtime binary for standalone Nudge agent executables.
//!
//! An executable produced by `nudgec build --standalone` consists of this
//! binary followed by an appended serialized `StandaloneBundle`, a 64-bit little-endian
//! length, and the 8-byte magic tag `b"NDGAGNT1"`.

use std::path::Path;
use std::process;

pub use nudgec::ast::{
    StandaloneBundle, STANDALONE_FOOTER_LEN as FOOTER_LEN, STANDALONE_MAGIC as MAGIC,
};

/// Standalone agent payload bundle alias for backwards compatibility.
pub type Bundle = StandaloneBundle;

/// Extracts an embedded `StandaloneBundle` from the end of an executable file.
///
/// Returns:
/// - `Ok(Some(bundle))` if a valid embedded bundle with magic tag is present.
/// - `Ok(None)` if the file is smaller than 16 bytes or does not end with `NDGAGNT1`.
/// - `Err(msg)` if the footer indicates a bundle is present but reading or deserialization fails.
pub fn extract_bundle(exe_path: &Path) -> Result<Option<StandaloneBundle>, String> {
    StandaloneBundle::extract_from_file(exe_path)
}

fn main() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine executable path: {e}");
            process::exit(1);
        }
    };

    let bundle = match extract_bundle(&exe_path) {
        Ok(Some(b)) => b,
        Ok(None) => {
            eprintln!(
                "nudge-agent: no embedded Nudge program (this binary was not produced by `nudgec build --standalone`)"
            );
            process::exit(1);
        }
        Err(e) => {
            if e.starts_with("error") {
                eprintln!("{e}");
            } else {
                eprintln!("error: {e}");
            }
            process::exit(1);
        }
    };

    nudge_runtime::trace::enable_default_sink();

    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let cli_args: Vec<nudgec::eval::NudgeVal> = raw_args
        .iter()
        .map(|a| nudgec::eval::parse_cli_arg(a))
        .collect();

    match nudgec::eval::run_program(&bundle.items, &bundle.entry, &cli_args) {
        Ok(val) => {
            let _ = nudge_runtime::trace::flush_sink();
            if val != nudgec::eval::NudgeVal::Unit {
                println!("{val}");
            }
        }
        Err(e) => {
            let _ = nudge_runtime::trace::flush_sink();
            if e.starts_with("error") {
                eprintln!("{e}");
            } else {
                eprintln!("error: {e}");
            }
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempTestFile {
        path: std::path::PathBuf,
    }

    impl TempTestFile {
        fn new(suffix: &str, bytes: &[u8]) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let pid = process::id();
            let path = std::env::temp_dir().join(format!("nudge_agent_test_{pid}_{id}_{suffix}"));
            std::fs::write(&path, bytes).expect("failed to write temp test file");
            Self { path }
        }
    }

    impl Drop for TempTestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn build_test_executable(prefix: &[u8], bundle: &Bundle) -> Vec<u8> {
        let bundle_bytes = bincode::serialize(bundle).expect("failed to serialize bundle");
        let bundle_len = bundle_bytes.len() as u64;
        let mut data = Vec::with_capacity(prefix.len() + bundle_bytes.len() + 16);
        data.extend_from_slice(prefix);
        data.extend_from_slice(&bundle_bytes);
        data.extend_from_slice(&bundle_len.to_le_bytes());
        data.extend_from_slice(MAGIC);
        data
    }

    #[test]
    fn test_extract_bundle_valid_round_trip_type_alias() {
        let prefix = b"\x7fELFfakeexecutablebinaryprefix";
        let original_bundle = Bundle {
            entry: "main".to_string(),
            items: vec![nudgec::ast::Item::TypeAlias {
                name: "UserId".to_string(),
                ty: nudgec::ast::TypeExpr::Named("int".to_string()),
            }],
        };
        let file_data = build_test_executable(prefix, &original_bundle);
        let temp_file = TempTestFile::new("valid_alias", &file_data);

        let extracted = extract_bundle(&temp_file.path).expect("extraction should succeed");
        assert_eq!(extracted, Some(original_bundle));
    }

    #[test]
    fn test_extract_bundle_valid_round_trip_fn() {
        let prefix = b"some_other_prefix_data";
        let original_bundle = Bundle {
            entry: "agent_entry".to_string(),
            items: vec![nudgec::ast::Item::Fn {
                name: "agent_entry".to_string(),
                params: vec![],
                ret: nudgec::ast::TypeExpr::Named("unit".to_string()),
                effects: vec![],
                cost_bound: None,
                body: vec![],
            }],
        };
        let file_data = build_test_executable(prefix, &original_bundle);
        let temp_file = TempTestFile::new("valid_fn", &file_data);

        let extracted = extract_bundle(&temp_file.path).expect("extraction should succeed");
        assert_eq!(extracted, Some(original_bundle));
    }

    #[test]
    fn test_extract_bundle_without_magic_returns_none() {
        let dummy_data = b"0123456789abcdefTHISISNOTTHEMAGICFOOTER!";
        let temp_file = TempTestFile::new("no_magic", dummy_data);

        let result = extract_bundle(&temp_file.path).expect("should not error on plain binary");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_bundle_short_files_return_none() {
        let empty = TempTestFile::new("empty", b"");
        assert_eq!(extract_bundle(&empty.path), Ok(None));

        let short = TempTestFile::new("short", b"short_data_15b!");
        assert_eq!(short.path.metadata().unwrap().len(), 15);
        assert_eq!(extract_bundle(&short.path), Ok(None));
    }

    #[test]
    fn test_extract_bundle_truncated_or_garbage_returns_error_or_none() {
        // Case 1: footer has MAGIC, but declared bundle_len exceeds file size
        let mut bad_len_data = vec![0u8; 32];
        let bad_len: u64 = 10_000;
        bad_len_data[16..24].copy_from_slice(&bad_len.to_le_bytes());
        bad_len_data[24..32].copy_from_slice(MAGIC);
        let temp_bad_len = TempTestFile::new("bad_len", &bad_len_data);

        let res = extract_bundle(&temp_bad_len.path);
        assert!(
            res.is_err(),
            "expected error when bundle_len exceeds file size"
        );
        assert!(res.unwrap_err().contains("exceeds"));

        // Case 2: footer has MAGIC, bundle_len fits within file, but payload is corrupted data
        let mut corrupt_payload = b"corrupted payload that is not valid bincode or json!!".to_vec();
        let payload_len = corrupt_payload.len() as u64;
        corrupt_payload.extend_from_slice(&payload_len.to_le_bytes());
        corrupt_payload.extend_from_slice(MAGIC);
        let temp_corrupt = TempTestFile::new("corrupt_payload", &corrupt_payload);

        let res = extract_bundle(&temp_corrupt.path);
        assert!(res.is_err(), "expected error when payload is invalid");
        assert!(res.unwrap_err().contains("deserialize"));

        // Case 3: non-existent file returns error, no panic
        let non_existent = std::path::Path::new("/tmp/this_file_surely_does_not_exist_xyz123");
        assert!(extract_bundle(non_existent).is_err());
    }

    #[test]
    fn test_shared_helper_append_and_extract_recovers_bundle() {
        let prefix = b"\x7fELFprebuiltbinarycontents";
        let original_bundle = StandaloneBundle {
            entry: "shared_entry".to_string(),
            items: vec![nudgec::ast::Item::Fn {
                name: "shared_entry".to_string(),
                params: vec![],
                ret: nudgec::ast::TypeExpr::Named("int".to_string()),
                effects: vec![],
                cost_bound: None,
                body: vec![],
            }],
        };

        let temp_file = TempTestFile::new("shared_helper", prefix);
        original_bundle
            .append_to_file(&temp_file.path)
            .expect("append_to_file should succeed");

        let extracted = extract_bundle(&temp_file.path)
            .expect("extraction should succeed")
            .expect("embedded bundle should be found");

        assert_eq!(extracted, original_bundle);
    }
}
