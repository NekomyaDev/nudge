//! Standalone native binary generator for Nudge programs.
//!
//! Produces a standalone agent without invoking Cargo by copying a pre-built
//! `nudge-agent` runtime binary and appending the serialized AST bundle with
//! footer metadata.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::StandaloneBundle;
use crate::{check, compile};

/// Locates the pre-built `nudge-agent` runtime binary.
///
/// Priority:
/// 1. `NUDGE_AGENT_RUNTIME` environment variable (a path to the binary).
/// 2. Sibling `nudge-agent` (or `nudge-agent.exe` on Windows) next to the
///    currently running executable (`std::env::current_exe()`).
///
/// Returns an error if not found.
pub fn locate_agent_runtime() -> Result<PathBuf, String> {
    if let Ok(env_path) = std::env::var("NUDGE_AGENT_RUNTIME") {
        let trimmed = env_path.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return Ok(p);
            }
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let bin_name = if cfg!(windows) {
                "nudge-agent.exe"
            } else {
                "nudge-agent"
            };
            let sibling = parent.join(bin_name);
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }

    Err("nudge-agent runtime not found; set NUDGE_AGENT_RUNTIME or install nudge-agent next to nudgec".to_string())
}

/// Builds a self-contained native binary for a Nudge program by stamping the
/// compiled AST bundle into a pre-built `nudge-agent` runtime binary.
pub fn build_standalone(
    src_path: impl AsRef<Path>,
    entry: &str,
    out_path: impl AsRef<Path>,
) -> Result<(), String> {
    let src_path = src_path.as_ref();
    let out_path = out_path.as_ref();

    let abs_src_path = fs::canonicalize(src_path)
        .map_err(|e| format!("cannot resolve source path {}: {e}", src_path.display()))?;

    // 1. Validate the program at build time
    let src = fs::read_to_string(&abs_src_path)
        .map_err(|e| format!("cannot read {}: {e}", abs_src_path.display()))?;
    let items = compile(&src).map_err(|(msg, at)| format!("error[E0002]: {msg} at byte {at}"))?;
    let errs = check::check(&items);
    if !errs.is_empty() {
        let mut err_msg = String::new();
        for e in errs {
            err_msg.push_str(&format!("error[{}]: {}\n", e.code, e.msg));
        }
        return Err(err_msg.trim_end().to_string());
    }

    // 2. Locate runtime binary before modifying filesystem
    let runtime_bin = locate_agent_runtime()?;

    // 3. Build bundle
    let bundle = StandaloneBundle {
        entry: entry.to_string(),
        items,
    };

    // 4. Ensure output parent directory exists
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
        }
    }

    // 5. Copy the runtime binary to out_path
    fs::copy(&runtime_bin, out_path).map_err(|e| {
        format!(
            "failed to copy runtime binary from {} to {}: {e}",
            runtime_bin.display(),
            out_path.display()
        )
    })?;

    // 6. Append the bundle payload: [bundle bytes][bundle_len: u64 LE][STANDALONE_MAGIC]
    if let Err(e) = bundle.append_to_file(out_path) {
        let _ = fs::remove_file(out_path);
        return Err(e);
    }

    // 7. Set executable permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(out_path) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = fs::set_permissions(out_path, perms);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locate_agent_runtime_missing_returns_clear_error() {
        // In an isolated test where NUDGE_AGENT_RUNTIME is not pointing to an existing file,
        // and no sibling exists in the test directory, locate_agent_runtime gives the expected error.
        let res = locate_agent_runtime();
        if let Err(msg) = res {
            assert_eq!(
                msg,
                "nudge-agent runtime not found; set NUDGE_AGENT_RUNTIME or install nudge-agent next to nudgec"
            );
        }
    }
}
