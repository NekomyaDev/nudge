//! Minimal runtime binary for standalone Nudge agent executables.
//!
//! An executable produced by `nudgec build --standalone` consists of this
//! binary followed by an appended serialized `Bundle`, a 64-bit little-endian
//! length, and the 8-byte magic tag `b"NDGAGNT1"`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process;

use serde::{Deserialize, Serialize};

/// Magic footer constant identifying a standalone Nudge agent binary.
pub const MAGIC: &[u8; 8] = b"NDGAGNT1";

/// Length of the footer: 8 bytes payload length (u64 LE) + 8 bytes MAGIC.
pub const FOOTER_LEN: u64 = 16;

/// Standalone agent payload bundle containing the entry point and compiled AST items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub entry: String,
    pub items: Vec<nudgec::ast::Item>,
}

/// Extracts an embedded `Bundle` from the end of an executable file.
///
/// Returns:
/// - `Ok(Some(bundle))` if a valid embedded bundle with magic tag is present.
/// - `Ok(None)` if the file is smaller than 16 bytes or does not end with `NDGAGNT1`.
/// - `Err(msg)` if the footer indicates a bundle is present but reading or deserialization fails.
pub fn extract_bundle(exe_path: &Path) -> Result<Option<Bundle>, String> {
    let mut file =
        File::open(exe_path).map_err(|e| format!("cannot open {}: {e}", exe_path.display()))?;
    let len = file
        .metadata()
        .map_err(|e| format!("cannot read metadata for {}: {e}", exe_path.display()))?
        .len();

    if len < FOOTER_LEN {
        return Ok(None);
    }

    file.seek(SeekFrom::Start(len - FOOTER_LEN))
        .map_err(|e| format!("cannot seek footer: {e}"))?;

    let mut footer = [0u8; 16];
    file.read_exact(&mut footer)
        .map_err(|e| format!("cannot read footer: {e}"))?;

    let magic = &footer[8..16];
    if magic != MAGIC {
        return Ok(None);
    }

    let bundle_len = u64::from_le_bytes(
        footer[0..8]
            .try_into()
            .map_err(|_| "failed to parse bundle length bytes".to_string())?,
    );

    if bundle_len > len - FOOTER_LEN {
        return Err(format!(
            "invalid bundle length: {bundle_len} exceeds available payload size {}",
            len - FOOTER_LEN
        ));
    }

    let bundle_len_usize = usize::try_from(bundle_len)
        .map_err(|_| format!("bundle length {bundle_len} exceeds usize capacity"))?;

    file.seek(SeekFrom::Start(len - FOOTER_LEN - bundle_len))
        .map_err(|e| format!("cannot seek to bundle payload: {e}"))?;

    let mut payload = vec![0u8; bundle_len_usize];
    file.read_exact(&mut payload)
        .map_err(|e| format!("cannot read bundle payload: {e}"))?;

    let bundle: Bundle = serde_json::from_slice(&payload)
        .map_err(|e| format!("failed to deserialize embedded bundle: {e}"))?;

    Ok(Some(bundle))
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
            if val != nudgec::eval::NudgeVal::Unit {
                println!("{val}");
            }
        }
        Err(e) => {
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
        let bundle_bytes = serde_json::to_vec(bundle).expect("failed to serialize bundle");
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

        // Case 2: footer has MAGIC, bundle_len fits within file, but payload is corrupted JSON
        let mut corrupt_json = b"corrupted payload that is not json!!".to_vec();
        let payload_len = corrupt_json.len() as u64;
        corrupt_json.extend_from_slice(&payload_len.to_le_bytes());
        corrupt_json.extend_from_slice(MAGIC);
        let temp_corrupt = TempTestFile::new("corrupt_json", &corrupt_json);

        let res = extract_bundle(&temp_corrupt.path);
        assert!(res.is_err(), "expected error when payload is invalid json");
        assert!(res.unwrap_err().contains("deserialize"));

        // Case 3: non-existent file returns error, no panic
        let non_existent = std::path::Path::new("/tmp/this_file_surely_does_not_exist_xyz123");
        assert!(extract_bundle(non_existent).is_err());
    }
}
