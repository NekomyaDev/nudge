//! Controlled process management runtime effect implementation.
//!
//! Provides bounded, authorized child process execution gated by grants
//! and cancellation checkpoints.

use crate::value::Value;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, Command, Stdio};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Default timeout for process execution in milliseconds.
const DEFAULT_PROCESS_TIMEOUT_MS: u64 = 5000;
/// Maximum allowed process timeout in milliseconds.
const MAX_PROCESS_TIMEOUT_MS: u64 = 120000;

/// Test lock to serialize tests touching `NUDGE_PROCESS_GRANTS` or process registry.
pub static PROC_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(not(target_arch = "wasm32"))]
static PROC_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(not(target_arch = "wasm32"))]
struct ManagedChild {
    cmd: String,
    child: Child,
    stdout_handle: Option<std::thread::JoinHandle<String>>,
    stderr_handle: Option<std::thread::JoinHandle<String>>,
    start_time: Instant,
    timeout: Duration,
    task_token: Option<crate::task::TaskToken>,
}

#[cfg(not(target_arch = "wasm32"))]
static PROCESS_REGISTRY: Mutex<Option<HashMap<String, ManagedChild>>> = Mutex::new(None);

#[cfg(not(target_arch = "wasm32"))]
fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, ManagedChild>) -> R,
{
    let mut guard = PROCESS_REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    f(map)
}

/// Register proc.* standard library functions.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("proc.run".to_string(), Value::Native("proc.run"));
    functions.insert("proc.spawn".to_string(), Value::Native("proc.spawn"));
    functions.insert("proc.await".to_string(), Value::Native("proc.await"));
    functions.insert("proc.wait".to_string(), Value::Native("proc.wait"));
    functions.insert("proc.kill".to_string(), Value::Native("proc.kill"));
    functions.insert(
        "proc.terminate".to_string(),
        Value::Native("proc.terminate"),
    );
    functions.insert("proc.alive".to_string(), Value::Native("proc.alive"));
    functions.insert("proc.is_alive".to_string(), Value::Native("proc.is_alive"));
    functions
}

/// Execute a proc.* function call.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "proc.run" => execute_run(args),
        "proc.spawn" => execute_spawn(args),
        "proc.await" | "proc.wait" => execute_await(args),
        "proc.kill" | "proc.terminate" => execute_kill(args),
        "proc.alive" | "proc.is_alive" => execute_alive(args),
        _ => Err(format!("unknown proc function '{name}'")),
    }
}

/// Check if a command and arguments are authorized by `NUDGE_PROCESS_GRANTS`.
///
/// Fail-closed security discipline:
/// - Unset or empty policy: default DENY with error[E0920].
/// - Valid JSON object or array: authoritative allowlist.
pub fn check_process_grants(cmd: &str, args: &[String]) -> Result<(), String> {
    let raw = match std::env::var("NUDGE_PROCESS_GRANTS") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            return Err(
                format!("error[E0920]: process execution denied: command '{cmd}' is not authorized by NUDGE_PROCESS_GRANTS (fail-closed default deny)"),
            );
        }
    };

    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("NUDGE_PROCESS_GRANTS is not valid JSON: {e}"))?;

    let cmd_name = std::path::Path::new(cmd)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(cmd);

    match parsed {
        serde_json::Value::Object(map) => {
            let candidate_keys = [cmd, cmd_name, "*"];
            for key in candidate_keys {
                if let Some(val) = map.get(key) {
                    match val {
                        serde_json::Value::Array(rules) => {
                            if rules.is_empty() {
                                return Err(format!(
                                    "error[E0920]: process '{cmd}' is not granted by NUDGE_PROCESS_GRANTS"
                                ));
                            }
                            // If rules contain "*", all arguments are permitted
                            let allows_all = rules.iter().any(|r| r.as_str() == Some("*"));
                            if allows_all {
                                return Ok(());
                            }
                            // Otherwise check if args match the glob rules
                            let all_match = args.iter().all(|arg| {
                                rules.iter().any(|r| {
                                    r.as_str()
                                        .map(|pat| crate::grants::fnmatch(pat, arg))
                                        .unwrap_or(false)
                                })
                            });
                            if all_match {
                                return Ok(());
                            } else {
                                return Err(format!(
                                    "error[E0920]: process '{cmd}' with arguments is not granted by NUDGE_PROCESS_GRANTS"
                                ));
                            }
                        }
                        serde_json::Value::Bool(true) => return Ok(()),
                        _ => {
                            return Err(format!(
                                "error[E0920]: process '{cmd}' is not granted by NUDGE_PROCESS_GRANTS"
                            ));
                        }
                    }
                }
            }
            Err(format!(
                "error[E0920]: process execution denied: command '{cmd}' is not authorized by NUDGE_PROCESS_GRANTS"
            ))
        }
        serde_json::Value::Array(list) => {
            for item in list {
                if let Some(s) = item.as_str() {
                    if s == "*" || s == cmd || s == cmd_name || crate::grants::fnmatch(s, cmd) {
                        return Ok(());
                    }
                }
            }
            Err(format!(
                "error[E0920]: process execution denied: command '{cmd}' is not authorized by NUDGE_PROCESS_GRANTS"
            ))
        }
        _ => Err("NUDGE_PROCESS_GRANTS must be a JSON object or array".to_string()),
    }
}

fn resolve_timeout(arg_timeout: Option<&Value>) -> Result<Duration, String> {
    let ms = if let Some(v) = arg_timeout {
        match v {
            Value::Int(i) => {
                if *i < 0 {
                    return Err("proc: timeout_ms must be non-negative".to_string());
                }
                *i as u64
            }
            Value::Float(f) => {
                if *f < 0.0 {
                    return Err("proc: timeout_ms must be non-negative".to_string());
                }
                *f as u64
            }
            _ => return Err("proc: timeout_ms must be a number".to_string()),
        }
    } else if let Ok(env_val) = std::env::var("NUDGE_PROCESS_TIMEOUT") {
        env_val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid NUDGE_PROCESS_TIMEOUT: {e}"))?
    } else {
        DEFAULT_PROCESS_TIMEOUT_MS
    };

    let clamped = ms.clamp(1, MAX_PROCESS_TIMEOUT_MS);
    Ok(Duration::from_millis(clamped))
}

fn parse_string_list(val: &Value, fn_name: &str) -> Result<Vec<String>, String> {
    match val {
        Value::List(list) => {
            let mut out = Vec::with_capacity(list.len());
            for item in list {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    _ => {
                        return Err(format!(
                            "{fn_name}: argument array must contain only strings"
                        ))
                    }
                }
            }
            Ok(out)
        }
        _ => Err(format!("{fn_name}: arguments must be a list of strings")),
    }
}

/// Execute `proc.spawn(cmd, args, timeout_ms)` returning a unique handle string.
pub fn execute_spawn(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("proc.spawn requires at least 1 argument (cmd)".to_string());
    }
    let cmd = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("proc.spawn: cmd must be a string".to_string()),
    };

    let argv = if args.len() > 1 {
        parse_string_list(&args[1], "proc.spawn")?
    } else {
        Vec::new()
    };

    let timeout = resolve_timeout(args.get(2))?;

    spawn_child(cmd, &argv, timeout)
}

/// Execute `proc.await(handle)` (or `proc.wait(handle)`).
pub fn execute_await(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("proc.await requires 1 argument (handle)".to_string());
    }
    let handle = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("proc.await: handle must be a string".to_string()),
    };

    await_child(handle)
}

/// Execute `proc.run(cmd, args, timeout_ms)` by spawning and awaiting.
pub fn execute_run(args: Vec<Value>) -> Result<Value, String> {
    let handle = execute_spawn(args)?;
    execute_await(vec![handle])
}

/// Execute `proc.kill(handle)` (or `proc.terminate(handle)`).
pub fn execute_kill(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("proc.kill requires 1 argument (handle)".to_string());
    }
    let handle = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("proc.kill: handle must be a string".to_string()),
    };

    kill_child(handle)
}

/// Execute `proc.alive(handle)` (or `proc.is_alive(handle)`).
pub fn execute_alive(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("proc.alive requires 1 argument (handle)".to_string());
    }
    let handle = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("proc.alive: handle must be a string".to_string()),
    };

    is_child_alive(handle)
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_child(cmd: &str, argv: &[String], timeout: Duration) -> Result<Value, String> {
    // 1. Authorization check
    check_process_grants(cmd, argv)?;

    // 2. Cancellation check before spawn
    if crate::task::is_current_cancelled() {
        return Err("error[E-task-0001]: TaskCancelled".to_string());
    }

    // 3. Spawn child with piped stdout/stderr and null stdin (no shell injection)
    let mut child = Command::new(cmd)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("process spawn failed for '{cmd}': {e}"))?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = stdout_pipe.take() {
            let _ = r.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).to_string()
    });

    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = stderr_pipe.take() {
            let _ = r.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).to_string()
    });

    let id_num = PROC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let handle_id = format!("proc_{id_num}");

    let managed = ManagedChild {
        cmd: cmd.to_string(),
        child,
        stdout_handle: Some(stdout_handle),
        stderr_handle: Some(stderr_handle),
        start_time: Instant::now(),
        timeout,
        task_token: crate::task::current_task(),
    };

    with_registry(|reg| {
        reg.insert(handle_id.clone(), managed);
    });

    Ok(Value::String(handle_id))
}

#[cfg(not(target_arch = "wasm32"))]
fn await_child(handle: &str) -> Result<Value, String> {
    loop {
        // Clean up and check cancellation
        let is_cancelled = crate::task::is_current_cancelled()
            || with_registry(|reg| {
                reg.get(handle)
                    .and_then(|m| m.task_token.as_ref())
                    .map(|t| t.is_cancelled())
                    .unwrap_or(false)
            });

        if is_cancelled {
            if let Some(mut managed) = with_registry(|reg| reg.remove(handle)) {
                let _ = managed.child.kill();
                let _ = managed.child.wait();
                if let Some(h) = managed.stdout_handle.take() {
                    let _ = h.join();
                }
                if let Some(h) = managed.stderr_handle.take() {
                    let _ = h.join();
                }
            }
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }

        // Check timeout
        let is_timed_out = with_registry(|reg| {
            reg.get(handle)
                .map(|m| m.start_time.elapsed() >= m.timeout)
                .unwrap_or(false)
        });

        if is_timed_out {
            if let Some(mut managed) = with_registry(|reg| reg.remove(handle)) {
                let _ = managed.child.kill();
                let _ = managed.child.wait();
                if let Some(h) = managed.stdout_handle.take() {
                    let _ = h.join();
                }
                if let Some(h) = managed.stderr_handle.take() {
                    let _ = h.join();
                }
                return Err(format!(
                    "error[E0921]: process '{}' timed out after {}ms and was killed",
                    managed.cmd,
                    managed.timeout.as_millis()
                ));
            }
        }

        // Check if finished
        let maybe_status = with_registry(|reg| {
            if let Some(managed) = reg.get_mut(handle) {
                managed.child.try_wait().ok().flatten()
            } else {
                None
            }
        });

        if let Some(status) = maybe_status {
            if let Some(mut managed) = with_registry(|reg| reg.remove(handle)) {
                let _ = managed.child.wait(); // Guarantee reaped
                let stdout = managed
                    .stdout_handle
                    .take()
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                let stderr = managed
                    .stderr_handle
                    .take()
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                let code = status.code().unwrap_or(-1) as i64;

                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("stdout".to_string(), Value::String(stdout));
                map.insert("stderr".to_string(), Value::String(stderr));
                return Ok(Value::Map(map));
            }
        }

        // Verify handle still exists
        let exists = with_registry(|reg| reg.contains_key(handle));
        if !exists {
            return Err(format!("process handle '{handle}' not found"));
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn kill_child(handle: &str) -> Result<Value, String> {
    if let Some(mut managed) = with_registry(|reg| reg.remove(handle)) {
        let _ = managed.child.kill();
        let _ = managed.child.wait(); // Reaped immediately
        if let Some(h) = managed.stdout_handle.take() {
            let _ = h.join();
        }
        if let Some(h) = managed.stderr_handle.take() {
            let _ = h.join();
        }
        Ok(Value::Bool(true))
    } else {
        Ok(Value::Bool(false))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn is_child_alive(handle: &str) -> Result<Value, String> {
    with_registry(|reg| {
        if let Some(managed) = reg.get_mut(handle) {
            match managed.child.try_wait() {
                Ok(None) => Ok(Value::Bool(true)),
                _ => Ok(Value::Bool(false)),
            }
        } else {
            Ok(Value::Bool(false))
        }
    })
}

/// Query the number of active child processes currently tracked.
#[cfg(not(target_arch = "wasm32"))]
pub fn active_process_count() -> usize {
    with_registry(|reg| reg.len())
}

/// Reap and kill all active child processes registered in the runtime.
#[cfg(not(target_arch = "wasm32"))]
pub fn reap_all_processes() {
    let mut processes = with_registry(std::mem::take);
    for (_, mut managed) in processes.drain() {
        let _ = managed.child.kill();
        let _ = managed.child.wait();
        if let Some(h) = managed.stdout_handle.take() {
            let _ = h.join();
        }
        if let Some(h) = managed.stderr_handle.take() {
            let _ = h.join();
        }
    }
}

/// Reap and kill active child processes whose task token is cancelled.
#[cfg(not(target_arch = "wasm32"))]
pub fn reap_cancelled_processes() {
    let cancelled_handles: Vec<String> = with_registry(|reg| {
        reg.iter()
            .filter_map(|(handle, child)| {
                if child
                    .task_token
                    .as_ref()
                    .map(|t| t.is_cancelled())
                    .unwrap_or(false)
                {
                    Some(handle.clone())
                } else {
                    None
                }
            })
            .collect()
    });
    for handle in cancelled_handles {
        if let Some(mut managed) = with_registry(|reg| reg.remove(&handle)) {
            let _ = managed.child.kill();
            let _ = managed.child.wait();
            if let Some(h) = managed.stdout_handle.take() {
                let _ = h.join();
            }
            if let Some(h) = managed.stderr_handle.take() {
                let _ = h.join();
            }
        }
    }
}

/// Reap and kill active child processes spawned by a specific task.
#[cfg(not(target_arch = "wasm32"))]
pub fn reap_processes_for_task(task_id: &crate::task::TaskId) {
    let matching: Vec<String> = with_registry(|reg| {
        reg.iter()
            .filter_map(|(handle, child)| {
                if child
                    .task_token
                    .as_ref()
                    .map(|t| &t.id == task_id)
                    .unwrap_or(false)
                {
                    Some(handle.clone())
                } else {
                    None
                }
            })
            .collect()
    });
    for handle in matching {
        if let Some(mut managed) = with_registry(|reg| reg.remove(&handle)) {
            let _ = managed.child.kill();
            let _ = managed.child.wait();
            if let Some(h) = managed.stdout_handle.take() {
                let _ = h.join();
            }
            if let Some(h) = managed.stderr_handle.take() {
                let _ = h.join();
            }
        }
    }
}

/// Get the OS process ID (PID) of a managed child process, if still active.
#[cfg(not(target_arch = "wasm32"))]
pub fn get_process_pid(handle: &str) -> Option<u32> {
    with_registry(|reg| reg.get(handle).map(|m| m.child.id()))
}

#[cfg(target_arch = "wasm32")]
fn spawn_child(_cmd: &str, _argv: &[String], _timeout: Duration) -> Result<Value, String> {
    Err("process spawning is not available on wasm32".to_string())
}

#[cfg(target_arch = "wasm32")]
fn await_child(_handle: &str) -> Result<Value, String> {
    Err("process spawning is not available on wasm32".to_string())
}

#[cfg(target_arch = "wasm32")]
fn kill_child(_handle: &str) -> Result<Value, String> {
    Err("process spawning is not available on wasm32".to_string())
}

#[cfg(target_arch = "wasm32")]
fn is_child_alive(_handle: &str) -> Result<Value, String> {
    Err("process spawning is not available on wasm32".to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn active_process_count() -> usize {
    0
}

#[cfg(target_arch = "wasm32")]
pub fn reap_all_processes() {}

#[cfg(target_arch = "wasm32")]
pub fn reap_cancelled_processes() {}

#[cfg(target_arch = "wasm32")]
pub fn reap_processes_for_task(_task_id: &crate::task::TaskId) {}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(val: &str) -> Self {
            let lock = PROC_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reap_all_processes();
            std::env::set_var("NUDGE_PROCESS_GRANTS", val);
            Self { _lock: lock }
        }

        fn unset() -> Self {
            let lock = PROC_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reap_all_processes();
            std::env::remove_var("NUDGE_PROCESS_GRANTS");
            Self { _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            reap_all_processes();
            std::env::remove_var("NUDGE_PROCESS_GRANTS");
        }
    }

    #[test]
    fn test_proc_run_without_grants_fails_closed() {
        let _guard = EnvGuard::unset();

        let res = execute(
            "proc.run",
            vec![
                Value::String("echo".to_string()),
                Value::List(vec![Value::String("hello".to_string())]),
            ],
        );

        assert!(res.is_err(), "expected fail-closed error without grants");
        let err = res.unwrap_err();
        assert!(
            err.contains("denied") && err.contains("E0920"),
            "expected denied in error message, got: {err}"
        );
    }

    #[test]
    fn test_proc_run_with_grants_executes_harmless_command() {
        let _guard = EnvGuard::set(r#"{"echo": ["*"]}"#);

        let res = execute(
            "proc.run",
            vec![
                Value::String("echo".to_string()),
                Value::List(vec![Value::String("hello world".to_string())]),
                Value::Int(2000),
            ],
        );

        assert!(res.is_ok(), "proc.run failed: {:?}", res);
        if let Value::Map(m) = res.unwrap() {
            assert_eq!(m.get("code"), Some(&Value::Int(0)));
            let stdout = match m.get("stdout") {
                Some(Value::String(s)) => s.trim().to_string(),
                _ => String::new(),
            };
            assert_eq!(stdout, "hello world");
        } else {
            panic!("expected Value::Map");
        }
    }

    #[test]
    fn test_proc_spawn_and_await_returns_output() {
        let _guard = EnvGuard::set(r#"{"echo": ["*"]}"#);

        let spawn_res = execute(
            "proc.spawn",
            vec![
                Value::String("echo".to_string()),
                Value::List(vec![Value::String("nudge-spawn-test".to_string())]),
                Value::Int(3000),
            ],
        );
        assert!(spawn_res.is_ok(), "spawn failed: {:?}", spawn_res);
        let handle = spawn_res.unwrap();

        let await_res = execute("proc.await", vec![handle]);
        assert!(await_res.is_ok(), "await failed: {:?}", await_res);
        if let Value::Map(m) = await_res.unwrap() {
            assert_eq!(m.get("code"), Some(&Value::Int(0)));
            let stdout = match m.get("stdout") {
                Some(Value::String(s)) => s.trim().to_string(),
                _ => String::new(),
            };
            assert_eq!(stdout, "nudge-spawn-test");
        } else {
            panic!("expected Value::Map");
        }

        assert_eq!(
            active_process_count(),
            0,
            "registry must be empty after await"
        );
    }

    #[test]
    fn test_proc_spawn_and_kill_reaps_child_no_orphan() {
        let _guard = EnvGuard::set(r#"{"sleep": ["*"]}"#);

        let spawn_res = execute(
            "proc.spawn",
            vec![
                Value::String("sleep".to_string()),
                Value::List(vec![Value::String("30".to_string())]),
                Value::Int(10000),
            ],
        );
        assert!(spawn_res.is_ok(), "spawn failed: {:?}", spawn_res);
        let handle = spawn_res.unwrap();
        let handle_str = match &handle {
            Value::String(s) => s.clone(),
            _ => panic!("expected string handle"),
        };

        let pid = get_process_pid(&handle_str).expect("expected pid for active child");

        let alive_res = execute("proc.alive", vec![handle.clone()]);
        assert_eq!(alive_res, Ok(Value::Bool(true)));

        // Kill child process
        let kill_res = execute("proc.kill", vec![handle.clone()]);
        assert_eq!(kill_res, Ok(Value::Bool(true)));

        // Verify no orphan left: process is reaped from OS
        assert_eq!(active_process_count(), 0);
        let alive_after = execute("proc.alive", vec![handle]);
        assert_eq!(alive_after, Ok(Value::Bool(false)));

        // Check OS process table: PID should be reaped (no zombie)
        #[cfg(target_os = "linux")]
        {
            let proc_status = format!("/proc/{pid}/status");
            if let Ok(status) = std::fs::read_to_string(&proc_status) {
                assert!(
                    !status.contains("State:\tZ (zombie)"),
                    "process {pid} was left as a zombie orphan in OS!"
                );
            }
        }
    }

    #[test]
    fn test_proc_run_timeout_kills_child_without_hang() {
        let _guard = EnvGuard::set(r#"{"sleep": ["*"]}"#);

        let start = std::time::Instant::now();
        let res = execute(
            "proc.run",
            vec![
                Value::String("sleep".to_string()),
                Value::List(vec![Value::String("10".to_string())]),
                Value::Int(80),
            ],
        );
        let elapsed = start.elapsed();

        assert!(res.is_err(), "expected timeout error");
        let err = res.unwrap_err();
        assert!(
            err.contains("timed out") && err.contains("killed"),
            "expected timeout message, got: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "command should have terminated quickly on timeout, took {elapsed:?}"
        );
        assert_eq!(
            active_process_count(),
            0,
            "timed out process must be reaped"
        );
    }

    #[test]
    fn test_proc_run_cancellation_kills_child_immediately() {
        let _guard = EnvGuard::set(r#"{"sleep": ["*"]}"#);

        let task = crate::task::TaskToken::new(crate::task::TaskId::new("test_cancel_proc"));
        let task_clone = task.clone();

        // Spawn thread that cancels the task after a short delay
        let cancel_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            task_clone.cancel();
        });

        let start = std::time::Instant::now();
        let res = crate::task::with_task(task, || {
            execute(
                "proc.run",
                vec![
                    Value::String("sleep".to_string()),
                    Value::List(vec![Value::String("10".to_string())]),
                    Value::Int(5000),
                ],
            )
        });
        let elapsed = start.elapsed();
        cancel_handle.join().unwrap();

        assert!(res.is_err(), "expected cancellation error");
        let err = res.unwrap_err();
        assert!(
            err.contains("TaskCancelled") || err.contains("E-task-0001"),
            "expected TaskCancelled error, got: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "command should have terminated quickly on cancel, took {elapsed:?}"
        );
        assert_eq!(
            active_process_count(),
            0,
            "cancelled process must be reaped"
        );
    }
}
