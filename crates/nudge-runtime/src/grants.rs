//! Native NUDGE_TOOL_GRANTS enforcement for the Nudge runtime.
//!
//! Provides fail-closed tool capability grants with exact parity to the Python reference
//! (`runtime/nudge_runtime/__init__.py` lines 503-614).
//!
//! # Policy Specification
//!
//! `NUDGE_TOOL_GRANTS` is an execution-layer tool policy represented as a JSON object:
//! - Keys: tool name (`"web_search"`), server-qualified tool (`"kb/retrieve"`),
//!   server-wide wildcard (`"kb/*"`), or global wildcard (`"*"`).
//! - Values: array of glob strings (e.g. `["*"]`, `["retrieve", "query_*"]`).
//!   An empty array `[]` denies all calls under that key.
//!
//! # Evaluation Precedence (Least Privilege)
//!
//! When calling tool `name` on optional server `server`, keys are checked in order:
//! 1. `{server}/{name}` (if `server` is present and non-empty)
//! 2. `{server}/*`     (if `server` is present and non-empty)
//! 3. `{name}`
//! 4. `*`
//!
//! The first matching key found in the policy is authoritative:
//! - If any glob rule in its list matches `name`, access is granted (`Ok(())`).
//! - If none of its rules match (or the list is empty), access is denied (`Err(...)`).
//!
//! Less specific rules cannot override a more specific match.
//! If none of the candidate keys exist in the policy, access is denied (`Err(...)`).
//!
//! # Fail-Closed Contract
//!
//! - If `NUDGE_TOOL_GRANTS` is unset: returns `Ok(())` (unrestricted, fail-open when no policy is configured).
//! - If `NUDGE_TOOL_GRANTS` is set: the policy is authoritative.
//!   - `{}` denies all tools.
//!   - Malformed JSON, non-object JSON, or non-glob-string-list rules return `Err(...)`.
//!   - Unapproved tool calls return:
//!     `tool '<name>' [on server '<server>'] is not granted by NUDGE_TOOL_GRANTS`.

use std::collections::HashMap;

/// Match a string against a glob pattern according to fnmatch rules.
///
/// Supports `*` (matches zero or more characters), `?` (matches any single character),
/// and `[...]` character classes (with `-` ranges and `!` or `^` negation).
pub fn fnmatch(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut match_ti = 0;

    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            match_ti = ti;
        } else if pi < p.len() && match_pattern_token(&p, &mut pi, t[ti]) {
            ti += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            match_ti += 1;
            ti = match_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

fn match_pattern_token(pattern: &[char], pi: &mut usize, target: char) -> bool {
    let start_pi = *pi;
    if start_pi >= pattern.len() {
        return false;
    }
    match pattern[start_pi] {
        '?' => {
            *pi += 1;
            true
        }
        '[' => {
            let rest = &pattern[start_pi + 1..];
            if let Some(close_idx) = rest.iter().position(|&c| c == ']') {
                let class_content = &rest[..close_idx];
                if match_char_class(class_content, target) {
                    *pi = start_pi + 1 + close_idx + 1;
                    true
                } else {
                    false
                }
            } else if pattern[start_pi] == target {
                *pi += 1;
                true
            } else {
                false
            }
        }
        ch => {
            if ch == target {
                *pi += 1;
                true
            } else {
                false
            }
        }
    }
}

fn match_char_class(content: &[char], target: char) -> bool {
    if content.is_empty() {
        return false;
    }
    let (negated, slice) = if content[0] == '!' || content[0] == '^' {
        (true, &content[1..])
    } else {
        (false, content)
    };

    let mut matched = false;
    let mut i = 0;
    while i < slice.len() {
        if i + 2 < slice.len() && slice[i + 1] == '-' {
            let start = slice[i];
            let end = slice[i + 2];
            if target >= start && target <= end {
                matched = true;
                break;
            }
            i += 3;
        } else {
            if slice[i] == target {
                matched = true;
                break;
            }
            i += 1;
        }
    }

    if negated {
        !matched
    } else {
        matched
    }
}

/// Formats the denial error message to match Python runtime parity.
fn deny_message(tool: &str, server: Option<&str>) -> String {
    match server {
        Some(s) if !s.is_empty() => {
            format!("tool '{tool}' on server '{s}' is not granted by NUDGE_TOOL_GRANTS")
        }
        _ => format!("tool '{tool}' is not granted by NUDGE_TOOL_GRANTS"),
    }
}

/// Evaluates a raw policy JSON string against a tool call and server.
///
/// Validates policy structure upfront and performs least-privilege lookup.
pub fn check_policy_str(raw: &str, tool: &str, server: Option<&str>) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return Err(format!("NUDGE_TOOL_GRANTS is not valid JSON: {e}")),
    };

    let obj = match value.as_object() {
        Some(map) => map,
        None => {
            return Err("NUDGE_TOOL_GRANTS must be a JSON object of {key: [rule, ...]}".to_string())
        }
    };

    let mut grants = HashMap::with_capacity(obj.len());
    for (key, rules_val) in obj {
        let arr = match rules_val.as_array() {
            Some(a) => a,
            None => {
                return Err(format!(
                    "NUDGE_TOOL_GRANTS rule for '{key}' must be a list of glob strings"
                ))
            }
        };
        let mut rules = Vec::with_capacity(arr.len());
        for item in arr {
            match item.as_str() {
                Some(s) => rules.push(s.to_string()),
                None => {
                    return Err(format!(
                        "NUDGE_TOOL_GRANTS rule for '{key}' must be a list of glob strings"
                    ))
                }
            }
        }
        grants.insert(key.as_str(), rules);
    }

    let mut keys = Vec::with_capacity(4);
    if let Some(s) = server {
        if !s.is_empty() {
            keys.push(format!("{s}/{tool}"));
            keys.push(format!("{s}/*"));
        }
    }
    keys.push(tool.to_string());
    keys.push("*".to_string());

    for key in &keys {
        if let Some(rules) = grants.get(key.as_str()) {
            if rules.iter().any(|r| fnmatch(r, tool)) {
                return Ok(());
            } else {
                return Err(deny_message(tool, server));
            }
        }
    }

    Err(deny_message(tool, server))
}

/// Check whether a tool call is allowed by the `NUDGE_TOOL_GRANTS` environment variable.
///
/// A fresh parse per call is performed to ensure thread-safe behavior without stale cache state.
///
/// Returns `Ok(())` if granted.
/// Returns `Err(String)` if denied or if the policy is invalid JSON / improperly typed.
pub fn check(tool: &str, server: Option<&str>) -> Result<(), String> {
    if crate::task::is_current_cancelled() {
        return Err("error[E-task-0001]: TaskCancelled".to_string());
    }
    let raw = match std::env::var("NUDGE_TOOL_GRANTS") {
        Ok(val) => val,
        Err(std::env::VarError::NotPresent) => return Ok(()),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("NUDGE_TOOL_GRANTS is not valid UTF-8".to_string())
        }
    };

    check_policy_str(&raw, tool, server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            unsafe {
                std::env::remove_var("NUDGE_TOOL_GRANTS");
            }
            Self { _lock: lock }
        }

        fn set(&self, val: &str) {
            unsafe {
                std::env::set_var("NUDGE_TOOL_GRANTS", val);
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("NUDGE_TOOL_GRANTS");
            }
        }
    }

    #[test]
    fn test_absent_env_is_unrestricted() {
        let _guard = EnvGuard::new();
        assert!(check("web_search", Some("untrusted")).is_ok());
        assert!(check("any_tool", None).is_ok());
    }

    #[test]
    fn test_empty_env_fails_closed() {
        let guard = EnvGuard::new();
        guard.set("");
        let err = check("web_search", None).unwrap_err();
        assert!(
            err.contains("not valid JSON"),
            "empty string env should fail JSON parsing, got: {err}"
        );
    }

    #[test]
    fn test_empty_object_denies_everything() {
        let guard = EnvGuard::new();
        guard.set("{}");
        let err = check("web_search", None).unwrap_err();
        assert_eq!(err, "tool 'web_search' is not granted by NUDGE_TOOL_GRANTS");
    }

    #[test]
    fn test_malformed_json_fails_closed() {
        let guard = EnvGuard::new();
        guard.set("not-json{");
        let err = check("web_search", None).unwrap_err();
        assert!(
            err.contains("NUDGE_TOOL_GRANTS is not valid JSON"),
            "expected JSON parse error, got: {err}"
        );
    }

    #[test]
    fn test_non_object_fails_closed() {
        let guard = EnvGuard::new();
        guard.set(r#"["web_search"]"#);
        let err = check("web_search", None).unwrap_err();
        assert_eq!(
            err,
            "NUDGE_TOOL_GRANTS must be a JSON object of {key: [rule, ...]}"
        );
    }

    #[test]
    fn test_invalid_rule_values_fail_closed() {
        let guard = EnvGuard::new();
        guard.set(r#"{"web_search": "*"}"#);
        let err = check("web_search", None).unwrap_err();
        assert_eq!(
            err,
            "NUDGE_TOOL_GRANTS rule for 'web_search' must be a list of glob strings"
        );

        guard.set(r#"{"web_search": [123]}"#);
        let err2 = check("web_search", None).unwrap_err();
        assert_eq!(
            err2,
            "NUDGE_TOOL_GRANTS rule for 'web_search' must be a list of glob strings"
        );
    }

    #[test]
    fn test_granted_tool() {
        let guard = EnvGuard::new();
        guard.set(r#"{"web_search": ["*"]}"#);
        assert!(check("web_search", None).is_ok());
    }

    #[test]
    fn test_tool_not_in_grants() {
        let guard = EnvGuard::new();
        guard.set(r#"{"web_search": ["*"]}"#);
        let err = check("other_tool", None).unwrap_err();
        assert_eq!(err, "tool 'other_tool' is not granted by NUDGE_TOOL_GRANTS");
    }

    #[test]
    fn test_server_restriction_beats_global_rule() {
        let guard = EnvGuard::new();
        guard.set(r#"{"web_search": ["*"], "untrusted/*": []}"#);
        let err = check("web_search", Some("untrusted")).unwrap_err();
        assert_eq!(
            err,
            "tool 'web_search' on server 'untrusted' is not granted by NUDGE_TOOL_GRANTS"
        );
        assert!(check("web_search", Some("trusted")).is_ok());
    }

    #[test]
    fn test_server_tool_beats_server_wide() {
        let guard = EnvGuard::new();
        guard.set(r#"{"kb/*": ["*"], "kb/secret": []}"#);
        let err = check("secret", Some("kb")).unwrap_err();
        assert_eq!(
            err,
            "tool 'secret' on server 'kb' is not granted by NUDGE_TOOL_GRANTS"
        );
        assert!(check("retrieve", Some("kb")).is_ok());
    }

    #[test]
    fn test_global_star_allows_unknown_tools() {
        let guard = EnvGuard::new();
        guard.set(r#"{"*": ["*"]}"#);
        assert!(check("anything", None).is_ok());
        assert!(check("anything", Some("any_server")).is_ok());
    }

    #[test]
    fn test_glob_rules_in_tool_names() {
        let guard = EnvGuard::new();
        guard.set(r#"{"kb/*": ["retrieve_*", "doc_?"]}"#);
        assert!(check("retrieve_article", Some("kb")).is_ok());
        assert!(check("doc_1", Some("kb")).is_ok());

        let err1 = check("doc_12", Some("kb")).unwrap_err();
        assert_eq!(
            err1,
            "tool 'doc_12' on server 'kb' is not granted by NUDGE_TOOL_GRANTS"
        );

        let err2 = check("delete_article", Some("kb")).unwrap_err();
        assert_eq!(
            err2,
            "tool 'delete_article' on server 'kb' is not granted by NUDGE_TOOL_GRANTS"
        );
    }

    #[test]
    fn test_fnmatch_patterns() {
        assert!(fnmatch("*", "anything"));
        assert!(fnmatch("*", ""));
        assert!(fnmatch("web_*", "web_search"));
        assert!(!fnmatch("web_*", "calc"));
        assert!(fnmatch("?at", "cat"));
        assert!(fnmatch("?at", "bat"));
        assert!(!fnmatch("?at", "at"));
        assert!(!fnmatch("?at", "chat"));
        assert!(fnmatch("[a-z]at", "cat"));
        assert!(!fnmatch("[a-z]at", "1at"));
        assert!(fnmatch("[!0-9]at", "cat"));
        assert!(!fnmatch("[!0-9]at", "1at"));
        assert!(fnmatch("tool[123]", "tool2"));
        assert!(!fnmatch("tool[123]", "tool4"));
        assert!(fnmatch("*search*", "google_search_v2"));
        assert!(fnmatch("exact_match", "exact_match"));
        assert!(!fnmatch("exact_match", "exact_match_2"));
    }

    #[test]
    fn test_cancel_beats_grants_for_task_and_descendants() {
        let _task_lock = crate::task::TASK_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::new();
        guard.set(r#"{"*": ["*"]}"#);
        crate::task::task_reset();

        // Without cancellation, tool calls are granted
        assert!(check("any_tool", Some("any_server")).is_ok());

        let root = crate::task::TaskToken::root();
        let child = root.child(0);
        let grandchild = child.child(0);
        crate::task::task_register(root.clone());
        crate::task::task_register(child.clone());
        crate::task::task_register(grandchild.clone());

        // Cancel root
        root.cancel();

        // Explicit test: cancelled task itself cannot call tool even with grants
        {
            let _g = crate::task::push_current_task(root);
            let err = check("any_tool", Some("any_server")).unwrap_err();
            assert_eq!(err, "error[E-task-0001]: TaskCancelled");
        }

        // Explicit test: child cannot call tool even with grants
        {
            let _g = crate::task::push_current_task(child);
            let err = check("any_tool", Some("any_server")).unwrap_err();
            assert_eq!(err, "error[E-task-0001]: TaskCancelled");
        }

        // Explicit test: grandchild cannot call tool even with grants
        {
            let _g = crate::task::push_current_task(grandchild);
            let err = check("any_tool", Some("any_server")).unwrap_err();
            assert_eq!(err, "error[E-task-0001]: TaskCancelled");
        }

        crate::task::task_reset();
    }
}
