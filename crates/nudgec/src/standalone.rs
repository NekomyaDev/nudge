//! Standalone native binary generator for Nudge programs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{check, compile};

/// Sanitizes an arbitrary name into a valid Cargo package name.
pub fn sanitize_crate_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        s = format!("app_{s}");
    }
    s
}

/// Locates the path to the nudgec crate for the Cargo.toml path dependency.
pub fn find_nudgec_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("NUDGEC_CRATE_PATH") {
        let pb = PathBuf::from(p);
        if pb.join("Cargo.toml").exists() {
            return fs::canonicalize(&pb)
                .map_err(|e| format!("failed to canonicalize NUDGEC_CRATE_PATH: {e}"));
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_dir.join("Cargo.toml").exists() {
        return fs::canonicalize(&manifest_dir)
            .map_err(|e| format!("failed to canonicalize CARGO_MANIFEST_DIR: {e}"));
    }
    Err("cannot locate nudgec crate path for standalone build dependency".to_string())
}

/// Generates Cargo.toml content for the standalone wrapper crate.
pub fn generate_cargo_toml(pkg_name: &str, nudgec_path: &str) -> String {
    format!(
        r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
nudgec = {{ path = "{nudgec_path}" }}

[profile.release]
opt-level = "z"
lto = true
strip = true
"#
    )
}

/// Generates main.rs content for the standalone wrapper crate.
pub fn generate_main_rs(abs_src_path: &str, entry: &str) -> String {
    format!(
        r#"const SRC: &str = include_str!({abs_src_path:?});
const ENTRY: &str = {entry:?};

fn main() {{
    nudgec::nudge_runtime::trace::enable_default_sink();
    let items = match nudgec::compile(SRC) {{
        Ok(items) => items,
        Err((msg, at)) => {{
            eprintln!("error[E0002]: {{msg}} at byte {{at}}");
            std::process::exit(1);
        }}
    }};
    let errs = nudgec::check::check(&items);
    if !errs.is_empty() {{
        for e in &errs {{
            eprintln!("error[{{}}]: {{}}", e.code, e.msg);
        }}
        std::process::exit(1);
    }}
    let cli_args: Vec<nudgec::eval::NudgeVal> = std::env::args()
        .skip(1)
        .map(|a| nudgec::eval::parse_cli_arg(&a))
        .collect();
    match nudgec::eval::run_program(&items, ENTRY, &cli_args) {{
        Ok(val) => {{
            if val != nudgec::eval::NudgeVal::Unit {{
                println!("{{val}}");
            }}
        }}
        Err(e) => {{
            if e.starts_with("error") {{
                eprintln!("{{e}}");
            }} else {{
                eprintln!("error: {{e}}");
            }}
            std::process::exit(1);
        }}
    }}
}}
"#
    )
}

/// Helper that resolves the target directory given optional environment overrides.
pub fn resolve_target_dir_from(
    cargo_target_dir: Option<&str>,
    xdg_cache_home: Option<&str>,
    home_or_userprofile: Option<&str>,
) -> PathBuf {
    let version = env!("CARGO_PKG_VERSION");
    let dirname = format!("standalone-target-{version}");

    if let Some(val) = cargo_target_dir {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_absolute() {
                return p;
            } else if let Ok(cwd) = std::env::current_dir() {
                return cwd.join(p);
            } else {
                return p;
            }
        }
    }

    if let Some(xdg) = xdg_cache_home {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("nudge").join(&dirname);
        }
    }

    #[cfg(windows)]
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let trimmed = local_app_data.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join("nudge")
                .join("cache")
                .join(&dirname);
        }
    }

    if let Some(home) = home_or_userprofile {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join(".cache")
                .join("nudge")
                .join(&dirname);
        }
    }

    std::env::temp_dir().join("nudge_cache").join(&dirname)
}

/// Returns the default persistent target directory for standalone builds.
///
/// Path structure:
/// - Unix/Linux/macOS: `$XDG_CACHE_HOME/nudge/standalone-target-<version>` or `$HOME/.cache/nudge/standalone-target-<version>`
/// - Windows: `%LOCALAPPDATA%\nudge\cache\standalone-target-<version>` or `%USERPROFILE%\.cache\nudge\standalone-target-<version>`
/// - Fallback: `<temp_dir>/nudge_cache/standalone-target-<version>`
pub fn default_target_dir() -> PathBuf {
    let xdg = std::env::var("XDG_CACHE_HOME").ok();
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    resolve_target_dir_from(None, xdg.as_deref(), home.as_deref())
}

/// Resolves the CARGO_TARGET_DIR to use for standalone builds.
///
/// If `CARGO_TARGET_DIR` is set in the environment, it is respected and used directly.
/// Otherwise, a persistent, version-keyed cache directory under the user's cache dir is returned.
pub fn resolve_target_dir() -> PathBuf {
    let env_target = std::env::var("CARGO_TARGET_DIR").ok();
    let xdg = std::env::var("XDG_CACHE_HOME").ok();
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    resolve_target_dir_from(env_target.as_deref(), xdg.as_deref(), home.as_deref())
}

/// Builds a self-contained native binary for a Nudge program.
pub fn build_standalone(
    src_path: impl AsRef<Path>,
    entry: &str,
    out_path: impl AsRef<Path>,
) -> Result<(), String> {
    let src_path = src_path.as_ref();
    let out_path = out_path.as_ref();

    let abs_src_path = fs::canonicalize(src_path)
        .map_err(|e| format!("cannot resolve source path {}: {e}", src_path.display()))?;
    let abs_src_str = abs_src_path
        .to_str()
        .ok_or_else(|| "source path contains non-UTF-8 characters".to_string())?;

    // Validate the program at build time
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

    let nudgec_path = find_nudgec_path()?;
    let nudgec_path_str = nudgec_path
        .to_str()
        .ok_or_else(|| "nudgec crate path contains non-UTF-8 characters".to_string())?;

    let out_file_stem = out_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app");
    let pkg_name = sanitize_crate_name(out_file_stem);

    // Create unique temp directory
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let temp_dir_name = format!("nudge_standalone_{pid}_{now}");
    let temp_dir = std::env::temp_dir().join(temp_dir_name);

    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir)
        .map_err(|e| format!("cannot create temp directory {}: {e}", src_dir.display()))?;

    let cargo_toml_content = generate_cargo_toml(&pkg_name, nudgec_path_str);
    let main_rs_content = generate_main_rs(abs_src_str, entry);

    if let Err(e) = fs::write(temp_dir.join("Cargo.toml"), &cargo_toml_content) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("cannot write Cargo.toml: {e}"));
    }
    if let Err(e) = fs::write(src_dir.join("main.rs"), &main_rs_content) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("cannot write main.rs: {e}"));
    }

    let target_dir = resolve_target_dir();
    if let Err(e) = fs::create_dir_all(&target_dir) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "cannot create target directory {}: {e}",
            target_dir.display()
        ));
    }

    let lockfile_cache = target_dir.join("standalone.lock");
    if lockfile_cache.exists() {
        let _ = fs::copy(&lockfile_cache, temp_dir.join("Cargo.lock"));
    }

    // Run cargo build --release
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(temp_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .map_err(|e| {
            let _ = fs::remove_dir_all(&temp_dir);
            format!("failed to invoke cargo: {e}")
        })?;

    if !status.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("cargo build failed with status {status}"));
    }

    let temp_lockfile = temp_dir.join("Cargo.lock");
    if temp_lockfile.exists() {
        let _ = fs::copy(&temp_lockfile, &lockfile_cache);
    }

    let bin_filename = if cfg!(windows) {
        format!("{pkg_name}.exe")
    } else {
        pkg_name.clone()
    };
    let built_bin = target_dir.join("release").join(&bin_filename);
    if !built_bin.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("built binary not found at {}", built_bin.display()));
    }

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(format!("cannot create directory {}: {e}", parent.display()));
            }
        }
    }

    if let Err(e) = fs::copy(&built_bin, out_path) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "failed to copy binary to {}: {e}",
            out_path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(out_path) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = fs::set_permissions(out_path, perms);
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_crate_name() {
        assert_eq!(sanitize_crate_name("my_app"), "my_app");
        assert_eq!(sanitize_crate_name("my-app"), "my-app");
        assert_eq!(sanitize_crate_name("123app"), "app_123app");
        assert_eq!(sanitize_crate_name("-leading"), "app_-leading");
        assert_eq!(sanitize_crate_name("foo.bar"), "foo_bar");
        assert_eq!(sanitize_crate_name(""), "app_");
    }

    #[test]
    fn test_generate_cargo_toml_content() {
        let toml = generate_cargo_toml("demo", "/path/to/nudgec");
        assert!(toml.contains("name = \"demo\""));
        assert!(toml.contains("path = \"/path/to/nudgec\""));
        assert!(toml.contains("strip = true"));
        assert!(toml.contains("lto = true"));
        assert!(toml.contains("opt-level = \"z\""));
    }

    #[test]
    fn test_generate_main_rs_content() {
        let main = generate_main_rs("/path/to/prog.ndg", "run");
        assert!(main.contains("include_str!(\"/path/to/prog.ndg\")"));
        assert!(main.contains("const ENTRY: &str = \"run\";"));
        assert!(main.contains("nudgec::compile(SRC)"));
        assert!(main.contains("nudgec::check::check(&items)"));
        assert!(main.contains("nudgec::eval::parse_cli_arg(&a)"));
        assert!(main.contains("nudgec::eval::run_program(&items, ENTRY, &cli_args)"));
    }

    #[test]
    fn test_resolve_target_dir_deterministic_and_versioned() {
        let version = env!("CARGO_PKG_VERSION");
        let default_dir = default_target_dir();
        assert!(default_dir
            .to_string_lossy()
            .contains(&format!("standalone-target-{version}")));
        assert!(!default_dir.to_string_lossy().contains("nudge_standalone_"));

        let custom = resolve_target_dir_from(Some("/custom/target"), None, None);
        assert_eq!(custom, PathBuf::from("/custom/target"));
    }
}
