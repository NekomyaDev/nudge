//! Nudge agent runtime library.
//!
//! Provides utilities for loading standalone bundles and executing offline replays.

use std::path::Path;

pub use nudgec::ast::{
    StandaloneBundle, STANDALONE_FOOTER_LEN as FOOTER_LEN, STANDALONE_MAGIC as MAGIC,
};

/// Standalone agent payload bundle alias for backwards compatibility.
pub type Bundle = StandaloneBundle;

/// Arguments parsed for WASI replay execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayArgs {
    pub bundle_path: String,
    pub trace_path: String,
    pub program_args: Vec<String>,
}

/// Parses command line arguments for WASI replay runner.
///
/// Expects:
/// - `args[1]`: path to bundle JSON file
/// - `args[2]`: path to NTF trace JSONL file
/// - `args[3..]`: optional arguments forwarded to the program
pub fn parse_replay_args<T: AsRef<str>>(args: &[T]) -> Result<ReplayArgs, String> {
    if args.len() < 3 {
        return Err("usage: nudge-replay-wasi <bundle_path> <trace_path> [args...]".to_string());
    }
    Ok(ReplayArgs {
        bundle_path: args[1].as_ref().to_string(),
        trace_path: args[2].as_ref().to_string(),
        program_args: args[3..].iter().map(|s| s.as_ref().to_string()).collect(),
    })
}

/// Extracts an embedded `StandaloneBundle` from the end of an executable file.
pub fn extract_bundle(exe_path: &Path) -> Result<Option<StandaloneBundle>, String> {
    StandaloneBundle::extract_from_file(exe_path)
}

/// Loads a serialized `StandaloneBundle` from a JSON file.
pub fn load_bundle(path: &Path) -> Result<StandaloneBundle, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read bundle file '{}': {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| {
        format!(
            "cannot deserialize bundle JSON from '{}': {e}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempTestFile {
        path: std::path::PathBuf,
    }

    impl TempTestFile {
        fn new(suffix: &str, bytes: &[u8]) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let pid = process::id();
            let path =
                std::env::temp_dir().join(format!("nudge_agent_lib_test_{pid}_{id}_{suffix}"));
            std::fs::write(&path, bytes).expect("failed to write temp test file");
            Self { path }
        }
    }

    impl Drop for TempTestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn test_parse_replay_args_valid_no_extra_args() {
        let args = [
            "nudge-replay-wasi",
            "path/to/bundle.json",
            "path/to/trace.jsonl",
        ];
        let parsed = parse_replay_args(&args).expect("should parse valid arguments");
        assert_eq!(
            parsed,
            ReplayArgs {
                bundle_path: "path/to/bundle.json".to_string(),
                trace_path: "path/to/trace.jsonl".to_string(),
                program_args: vec![],
            }
        );
    }

    #[test]
    fn test_parse_replay_args_valid_with_args() {
        let args = [
            "nudge-replay-wasi",
            "bundle.json",
            "trace.jsonl",
            "arg1",
            "42",
            "hello world",
        ];
        let parsed = parse_replay_args(&args).expect("should parse arguments with program args");
        assert_eq!(
            parsed,
            ReplayArgs {
                bundle_path: "bundle.json".to_string(),
                trace_path: "trace.jsonl".to_string(),
                program_args: vec![
                    "arg1".to_string(),
                    "42".to_string(),
                    "hello world".to_string(),
                ],
            }
        );
    }

    #[test]
    fn test_parse_replay_args_insufficient() {
        let empty: [&str; 0] = [];
        assert!(parse_replay_args(&empty).is_err());

        let one = ["nudge-replay-wasi"];
        assert!(parse_replay_args(&one).is_err());

        let two = ["nudge-replay-wasi", "bundle.json"];
        assert!(parse_replay_args(&two).is_err());
    }

    #[test]
    fn test_load_bundle_round_trip() {
        let original_bundle = StandaloneBundle {
            entry: "main".to_string(),
            items: vec![nudgec::ast::Item::Fn {
                name: "main".to_string(),
                params: vec![],
                ret: nudgec::ast::TypeExpr::Named("unit".to_string()),
                effects: vec![],
                body: vec![],
            }],
        };

        let json_bytes =
            serde_json::to_vec(&original_bundle).expect("failed to serialize bundle to JSON");
        let temp_file = TempTestFile::new("bundle_round_trip.json", &json_bytes);

        let loaded = load_bundle(&temp_file.path).expect("failed to load valid bundle");
        assert_eq!(loaded, original_bundle);
    }

    #[test]
    fn test_load_bundle_missing_file() {
        let missing_path = Path::new("/tmp/nonexistent_bundle_file_12345.json");
        let err = load_bundle(missing_path).unwrap_err();
        assert!(err.contains("cannot read bundle file"));
    }

    #[test]
    fn test_load_bundle_invalid_json() {
        let corrupt_data = b"{ this is definitely not valid json }";
        let temp_file = TempTestFile::new("corrupt_bundle.json", corrupt_data);

        let err = load_bundle(&temp_file.path).unwrap_err();
        assert!(err.contains("cannot deserialize bundle JSON"));
    }
}
