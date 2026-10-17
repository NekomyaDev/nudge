//! Safe workspace filesystem confinement and operations.
//!
//! Provides a sandboxed filesystem interface confined to an explicit workspace root
//! (`--workspace` flag or `NUDGE_WORKSPACE_DIR` env var, defaulting to cwd).
//!
//! Containment pipeline (design §5):
//! 1. Lexically normalise the input path (strip `.` and evaluate `..`).
//! 2. Canonicalize both workspace root and target (or nearest existing ancestor for new files).
//! 3. Verify `canonical_target.starts_with(canonical_root)`.
//! 4. If any path component is a symlink, verify the link target resolves inside the root.
//!
//! Atomic writes use a hidden temp sibling, fsync, then atomic rename.

use crate::value::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Get the configured workspace root directory from `NUDGE_WORKSPACE_DIR`
/// or fall back to the current working directory.
pub fn get_workspace_root() -> PathBuf {
    if let Ok(dir) = std::env::var("NUDGE_WORKSPACE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Lexically normalize a path, resolving `.` and `..` components without touching disk.
/// This is the first-pass before canonicalization.
pub fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(c) => {
                normalized.push(c);
            }
            Component::RootDir => {
                // Keep root for absolute paths — containment check will catch escapes.
                normalized.push("/");
            }
            Component::CurDir | Component::Prefix(_) => {
                // Skip redundant current-dir markers.
            }
        }
    }
    normalized
}

/// Resolve and verify that a target path is strictly contained within the workspace root.
///
/// Returns the canonicalized (or partially-canonicalized) target path on success.
/// Returns `Err("error[E0910]: workspace path escape denied: <path>")` on any escape attempt.
///
/// Location: `crates/nudge-runtime/src/workspace.rs`, function `resolve_and_validate_path`.
pub fn resolve_and_validate_path(user_path: &str) -> Result<PathBuf, String> {
    let raw_root = get_workspace_root();
    let canonical_root = fs::canonicalize(&raw_root).map_err(|e| {
        format!(
            "workspace path error: cannot canonicalize workspace root '{}': {}",
            raw_root.display(),
            e
        )
    })?;

    let input_path = Path::new(user_path);

    // For absolute paths: check if they start with the workspace root.
    // If not, they escape the workspace. We re-join the relative remainder
    // under the root so containment stays enforced.
    let target_path = if input_path.is_absolute() {
        // Attempt to strip the root prefix: if the absolute path is inside,
        // this yields the relative tail; otherwise we build a path that will
        // fail the starts_with check below.
        match input_path.strip_prefix(&canonical_root) {
            Ok(tail) => canonical_root.join(tail),
            Err(_) => {
                // Absolute path that doesn't start with the workspace root.
                return Err(format!(
                    "error[E0910]: workspace path escape denied: {}",
                    user_path
                ));
            }
        }
    } else {
        raw_root.join(input_path)
    };

    // Canonicalize by walking up to the nearest existing ancestor,
    // collecting missing components along the way (for new-file writes).
    let mut current = target_path.as_path();
    let mut missing_components: Vec<std::ffi::OsString> = Vec::new();

    let canonical_target = loop {
        match fs::canonicalize(current) {
            Ok(canon) => {
                // Reconstruct the full path by appending missing components.
                let mut rebuilt = canon;
                for comp in missing_components.into_iter().rev() {
                    rebuilt.push(comp);
                }
                break rebuilt;
            }
            Err(_) => {
                match current.parent() {
                    Some(parent) if parent != current => {
                        if let Some(file_name) = current.file_name() {
                            missing_components.push(file_name.to_owned());
                        }
                        current = parent;
                    }
                    _ => {
                        // Reached the filesystem root without finding any existing ancestor.
                        return Err(format!(
                            "error[E0910]: workspace path escape denied: {}",
                            user_path
                        ));
                    }
                }
            }
        }
    };

    // (3) Verify containment: target must start with the canonical root.
    if !canonical_target.starts_with(&canonical_root) {
        return Err(format!(
            "error[E0910]: workspace path escape denied: {}",
            user_path
        ));
    }

    // (4) Symlink check: walk the existing portion under the root and verify
    //     every symlink resolves inside the workspace.
    let relative = canonical_target
        .strip_prefix(&canonical_root)
        .expect("just verified starts_with");

    let mut check_path = canonical_root.clone();
    for comp in relative.components() {
        if let Component::Normal(s) = comp {
            check_path.push(s);
            match fs::symlink_metadata(&check_path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    let link_target = fs::read_link(&check_path).map_err(|e| {
                        format!("workspace symlink read failed for '{}': {}", user_path, e)
                    })?;
                    let resolved = if link_target.is_absolute() {
                        link_target
                    } else {
                        check_path
                            .parent()
                            .expect("symlink always has a parent")
                            .join(link_target)
                    };
                    match fs::canonicalize(&resolved) {
                        Ok(canon_link) => {
                            if !canon_link.starts_with(&canonical_root) {
                                return Err(format!(
                                    "error[E0910]: workspace path escape denied: symlink escape: {}",
                                    user_path
                                ));
                            }
                        }
                        Err(_) => {
                            return Err(format!(
                                "workspace symlink resolution failed: {}",
                                user_path
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(canonical_target)
}

/// Register workspace standard library functions in the environment.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert(
        "workspace.read".to_string(),
        Value::Native("workspace.read"),
    );
    functions.insert(
        "workspace.write".to_string(),
        Value::Native("workspace.write"),
    );
    functions.insert(
        "workspace.append".to_string(),
        Value::Native("workspace.append"),
    );
    functions.insert(
        "workspace.exists".to_string(),
        Value::Native("workspace.exists"),
    );
    functions.insert(
        "workspace.list".to_string(),
        Value::Native("workspace.list"),
    );
    functions.insert(
        "workspace.delete".to_string(),
        Value::Native("workspace.delete"),
    );
    functions
}

/// Execute a workspace function call.
///
/// All operations enforce containment via `resolve_and_validate_path`.
/// `workspace.write` uses atomic temp+fsync+rename to prevent torn files.
/// `workspace.delete` requires `recursive: true` for directories and forbids deleting the root.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "workspace.read" => {
            if args.len() != 1 {
                return Err("workspace.read requires 1 argument (path)".to_string());
            }
            let path_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.read: path must be a string".to_string()),
            };
            let safe_path = resolve_and_validate_path(&path_str)?;
            match fs::read_to_string(&safe_path) {
                Ok(content) => Ok(Value::String(content)),
                Err(e) => Err(format!("workspace.read: cannot read '{}': {}", path_str, e)),
            }
        }

        "workspace.write" => {
            if args.len() != 2 {
                return Err("workspace.write requires 2 arguments (path, content)".to_string());
            }
            let path_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.write: path must be a string".to_string()),
            };
            let content = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.write: content must be a string".to_string()),
            };

            let safe_path = resolve_and_validate_path(&path_str)?;

            // Ensure parent directory exists.
            if let Some(parent) = safe_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("workspace.write: cannot create parent directory: {}", e)
                })?;
            }

            // Atomic write: write to hidden sibling temp file, fsync, then rename.
            let file_name = safe_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let temp_name = format!(
                ".{}.tmp.{}.{}",
                file_name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let temp_path = safe_path.with_file_name(temp_name);

            let mut file = fs::File::create(&temp_path).map_err(|e| {
                format!(
                    "workspace.write: cannot create temp file for '{}': {}",
                    path_str, e
                )
            })?;
            if let Err(e) = file.write_all(content.as_bytes()) {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "workspace.write: cannot write to temp file for '{}': {}",
                    path_str, e
                ));
            }
            if let Err(e) = file.flush() {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "workspace.write: cannot flush temp file for '{}': {}",
                    path_str, e
                ));
            }
            if let Err(e) = file.sync_all() {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "workspace.write: cannot sync temp file for '{}': {}",
                    path_str, e
                ));
            }
            drop(file);

            if let Err(e) = fs::rename(&temp_path, &safe_path) {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "workspace.write: cannot commit atomic write for '{}': {}",
                    path_str, e
                ));
            }

            Ok(Value::None)
        }

        "workspace.append" => {
            if args.len() != 2 {
                return Err("workspace.append requires 2 arguments (path, content)".to_string());
            }
            let path_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.append: path must be a string".to_string()),
            };
            let content = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.append: content must be a string".to_string()),
            };

            let raw_path = get_workspace_root().join(&path_str);
            if let Ok(meta) = raw_path.symlink_metadata() {
                if meta.file_type().is_symlink() {
                    return Err(format!(
                        "error[E0910]: workspace path escape denied: symlink escape for '{}'",
                        path_str
                    ));
                }
            }

            let safe_path = resolve_and_validate_path(&path_str)?;
            if let Some(parent) = safe_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("workspace.append: cannot create parent directory: {}", e)
                })?;
            }

            // Verify path is not a symlink prior to opening
            if let Ok(meta) = safe_path.symlink_metadata() {
                if meta.file_type().is_symlink() {
                    return Err(format!(
                        "error[E0910]: workspace path escape denied: symlink escape for '{}'",
                        path_str
                    ));
                }
            }

            // Defense-in-depth against a PARENT-directory symlink swap: re-check
            // that the canonicalized parent still resolves under the workspace
            // root right before opening (the O_NOFOLLOW on open only covers the
            // final component). A full fd-based openat2 resolution is the robust
            // fix; this narrows the race window.
            if let Some(parent) = safe_path.parent() {
                if let Ok(canon_parent) = parent.canonicalize() {
                    let root = get_workspace_root()
                        .canonicalize()
                        .unwrap_or_else(|_| get_workspace_root());
                    if !canon_parent.starts_with(&root) {
                        return Err(format!(
                            "error[E0910]: workspace path escape denied: parent symlink escape for '{}'",
                            path_str
                        ));
                    }
                }
            }

            let mut opts = fs::OpenOptions::new();
            opts.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.custom_flags(libc::O_NOFOLLOW);
            }

            match opts.open(&safe_path) {
                Ok(mut file) => {
                    let meta = safe_path.symlink_metadata().map_err(|e| {
                        format!("workspace.append: cannot inspect '{}': {}", path_str, e)
                    })?;
                    if meta.file_type().is_symlink() {
                        return Err(format!(
                            "error[E0910]: workspace path escape denied: symlink escape for '{}'",
                            path_str
                        ));
                    }

                    file.write_all(content.as_bytes()).map_err(|e| {
                        format!("workspace.append: cannot append to '{}': {}", path_str, e)
                    })?;
                    file.sync_all().unwrap_or(());
                    Ok(Value::None)
                }
                Err(e) => Err(format!(
                    "workspace.append: cannot open '{}': {}",
                    path_str, e
                )),
            }
        }

        "workspace.exists" => {
            if args.len() != 1 {
                return Err("workspace.exists requires 1 argument (path)".to_string());
            }
            let path_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.exists: path must be a string".to_string()),
            };
            // A path escape => does not exist within the workspace.
            match resolve_and_validate_path(&path_str) {
                Ok(safe_path) => Ok(Value::Bool(safe_path.exists())),
                Err(_) => Ok(Value::Bool(false)),
            }
        }

        "workspace.list" => {
            // `dir` defaults to "." if not provided.
            let path_str = if args.is_empty() {
                ".".to_string()
            } else {
                match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("workspace.list: path must be a string".to_string()),
                }
            };
            let safe_path = resolve_and_validate_path(&path_str)?;
            match fs::read_dir(&safe_path) {
                Ok(entries) => {
                    let mut result = Vec::new();
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            result.push(name.to_string());
                        }
                    }
                    result.sort();
                    Ok(Value::List(result.into_iter().map(Value::String).collect()))
                }
                Err(e) => Err(format!(
                    "workspace.list: cannot list directory '{}': {}",
                    path_str, e
                )),
            }
        }

        "workspace.delete" => {
            if args.is_empty() {
                return Err("workspace.delete requires at least 1 argument (path)".to_string());
            }
            let path_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("workspace.delete: path must be a string".to_string()),
            };
            let recursive = if args.len() > 1 {
                match &args[1] {
                    Value::Bool(b) => *b,
                    _ => {
                        return Err("workspace.delete: recursive flag must be a boolean".to_string())
                    }
                }
            } else {
                false
            };

            let safe_path = resolve_and_validate_path(&path_str)?;

            // Forbid deleting the workspace root itself.
            let raw_root = get_workspace_root();
            if let Ok(canon_root) = fs::canonicalize(&raw_root) {
                if safe_path == canon_root {
                    return Err(
                        "workspace.delete: deleting the workspace root itself is forbidden"
                            .to_string(),
                    );
                }
            }

            let metadata = fs::metadata(&safe_path)
                .map_err(|e| format!("workspace.delete: cannot access '{}': {}", path_str, e))?;

            if metadata.is_dir() {
                if !recursive {
                    return Err(format!(
                        "workspace.delete: directory '{}' requires recursive: true",
                        path_str
                    ));
                }
                fs::remove_dir_all(&safe_path).map_err(|e| {
                    format!(
                        "workspace.delete: cannot delete directory '{}': {}",
                        path_str, e
                    )
                })?;
            } else {
                fs::remove_file(&safe_path).map_err(|e| {
                    format!("workspace.delete: cannot delete file '{}': {}", path_str, e)
                })?;
            }

            Ok(Value::None)
        }

        _ => Err(format!("Unknown workspace function: {}", name)),
    }
}

/// Serialize tests that share process-global environment variables like
/// `NUDGE_WORKSPACE_DIR` and `NUDGE_STORE_PATH`.
pub static WS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up a fresh temp directory and set NUDGE_WORKSPACE_DIR to it.
    /// Returns the temp directory path. Caller is responsible for cleanup.
    fn setup_temp_workspace(suffix: &str) -> PathBuf {
        let temp_dir =
            std::env::temp_dir().join(format!("nudge_ws_{}_{}", suffix, std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        // Safety: tests run in the same process; use per-test unique dirs to avoid collisions.
        // NUDGE_WORKSPACE_DIR is set here; each test should restore it.
        std::env::set_var("NUDGE_WORKSPACE_DIR", &temp_dir);
        temp_dir
    }

    fn teardown(temp_dir: &PathBuf) {
        let _ = fs::remove_dir_all(temp_dir);
        std::env::remove_var("NUDGE_WORKSPACE_DIR");
    }

    #[test]
    fn workspace_round_trip_inside_root() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("roundtrip");

        // Write
        let res = execute(
            "workspace.write",
            vec![
                Value::String("hello.txt".to_string()),
                Value::String("hello workspace".to_string()),
            ],
        );
        assert!(res.is_ok(), "write failed: {:?}", res);

        // Exists
        let res = execute(
            "workspace.exists",
            vec![Value::String("hello.txt".to_string())],
        );
        assert_eq!(res.unwrap(), Value::Bool(true));

        // Read
        let res = execute(
            "workspace.read",
            vec![Value::String("hello.txt".to_string())],
        );
        assert_eq!(
            res.unwrap(),
            Value::String("hello workspace".to_string()),
            "round-trip content mismatch"
        );

        // List
        let res = execute("workspace.list", vec![Value::String(".".to_string())]);
        assert_eq!(
            res.unwrap(),
            Value::List(vec![Value::String("hello.txt".to_string())])
        );

        // Delete file (single file — no recursive)
        let res = execute(
            "workspace.delete",
            vec![Value::String("hello.txt".to_string())],
        );
        assert!(res.is_ok(), "delete failed: {:?}", res);
        assert!(!dir.join("hello.txt").exists());

        teardown(&dir);
    }

    #[test]
    fn workspace_dotdot_escape_is_denied() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("escape_dotdot");

        // Relative path traversal
        let res = execute(
            "workspace.read",
            vec![Value::String("../outside.txt".to_string())],
        );
        assert!(res.is_err(), "expected escape denial for ../outside.txt");
        let msg = res.unwrap_err();
        assert!(
            msg.contains("error[E0910]") && msg.contains("workspace path escape denied"),
            "unexpected error: {}",
            msg
        );

        teardown(&dir);
    }

    #[test]
    fn workspace_absolute_escape_is_denied() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("escape_abs");

        // Absolute path outside workspace
        let res = execute(
            "workspace.read",
            vec![Value::String("/etc/hosts".to_string())],
        );
        assert!(res.is_err(), "expected escape denial for /etc/hosts");
        let msg = res.unwrap_err();
        assert!(
            msg.contains("error[E0910]") && msg.contains("workspace path escape denied"),
            "unexpected error: {}",
            msg
        );

        teardown(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_symlink_escape_is_denied() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("escape_sym");

        // Create a symlink inside workspace pointing to /etc (outside)
        let sym = dir.join("evil_link");
        std::os::unix::fs::symlink("/etc", &sym).unwrap();

        let res = execute(
            "workspace.read",
            vec![Value::String("evil_link/hosts".to_string())],
        );
        assert!(
            res.is_err(),
            "expected symlink escape denial, got: {:?}",
            res
        );
        let msg = res.unwrap_err();
        assert!(
            msg.contains("escape denied") || msg.contains("symlink"),
            "unexpected error: {}",
            msg
        );

        teardown(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_append_symlink_denied() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("append_sym");

        let target = dir.join("target.txt");
        fs::write(&target, "initial").unwrap();

        let sym = dir.join("link_to_target.txt");
        std::os::unix::fs::symlink(&target, &sym).unwrap();

        let res = execute(
            "workspace.append",
            vec![
                Value::String("link_to_target.txt".to_string()),
                Value::String(" appended".to_string()),
            ],
        );
        assert!(res.is_err(), "expected append through symlink to be denied");
        let msg = res.unwrap_err();
        assert!(
            msg.contains("symlink") || msg.contains("cannot open"),
            "expected symlink denial message, got: {msg}"
        );

        teardown(&dir);
    }

    #[test]
    fn workspace_delete_without_recursive_fails_on_dir() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("delete_norecursive");

        let subdir = dir.join("subdir");
        fs::create_dir(&subdir).unwrap();

        let res = execute(
            "workspace.delete",
            vec![Value::String("subdir".to_string())],
        );
        assert!(
            res.is_err(),
            "expected error deleting dir without recursive"
        );
        let msg = res.unwrap_err();
        assert!(
            msg.contains("recursive"),
            "error should mention recursive: {}",
            msg
        );

        teardown(&dir);
    }

    #[test]
    fn workspace_delete_root_is_forbidden() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("delete_root");

        // Trying to delete "." (the workspace root)
        let res = execute("workspace.delete", vec![Value::String(".".to_string())]);
        assert!(res.is_err(), "expected root deletion to be forbidden");
        let msg = res.unwrap_err();
        assert!(
            msg.contains("root") || msg.contains("forbidden"),
            "unexpected error: {}",
            msg
        );

        teardown(&dir);
    }

    #[test]
    fn workspace_atomic_write_uses_temp_rename() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("atomic_write");

        // Pre-populate target file with known content.
        let target = dir.join("report.txt");
        fs::write(&target, "valid content").unwrap();

        // Normal write should atomically replace the file.
        let res = execute(
            "workspace.write",
            vec![
                Value::String("report.txt".to_string()),
                Value::String("new content".to_string()),
            ],
        );
        assert!(res.is_ok(), "atomic write failed: {:?}", res);

        // The original temp file should NOT exist (it was renamed away).
        let temps: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with(".report.txt.tmp."))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            temps.is_empty(),
            "temp file should have been renamed away, but found: {:?}",
            temps.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );

        // The file contains the new content.
        let content = fs::read_to_string(&target).unwrap();
        assert_eq!(content, "new content");

        teardown(&dir);
    }

    #[test]
    fn workspace_list_defaults_to_dot() {
        let _guard = WS_TEST_LOCK.lock().unwrap();
        let dir = setup_temp_workspace("list_default");

        // Write a file
        execute(
            "workspace.write",
            vec![
                Value::String("a.txt".to_string()),
                Value::String("a".to_string()),
            ],
        )
        .unwrap();

        // List with no args (default ".")
        let res = execute("workspace.list", vec![]);
        assert!(res.is_ok(), "workspace.list() failed: {:?}", res);
        let Value::List(entries) = res.unwrap() else {
            panic!("expected list")
        };
        assert!(
            entries.contains(&Value::String("a.txt".to_string())),
            "a.txt missing from list"
        );

        teardown(&dir);
    }
}
