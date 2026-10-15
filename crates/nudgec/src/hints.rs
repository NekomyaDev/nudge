// Plain-language hints under compiler errors (A3: "the compiler helps,
// it doesn't just scold"). One short line per E-code, printed under the
// error; plus `closest()` for did-you-mean suggestions at E0101 sites.

/// Static per-code hint. Always one line, plain language, action-first.
pub fn hint(code: &str) -> Option<&'static str> {
    Some(match code {
        "E0001" => "the file isn't valid Nudge text — look for a stray character just before this position",
        "E0002" => "that line isn't valid Nudge syntax — compare with a working example (`nudgec learn hello`) and check brackets and commas around the position",
        "E0101" => "a name is not defined or is misspelled — check the spelling, or declare the type/fn above its first use",
        "E0102" => "the same type name is declared twice — rename one of them",
        "E0103" => "duplicate declaration — every fn/tool/effect name must be unique",
        "E0201" => "two types don't fit together — trace the expression back to its source; record fields are read with dot syntax and each has exactly one type",
        "E0202" => "a constraint or schema is malformed — check @range bounds order (lo, hi) and that a `schema:` value is a record type",
        "E0203" => "duplicate field in record literal - each field name in a record must be unique",
        "E0301" => "this call has an effect the function doesn't declare: add the effect (LLM, Tool, IO, Decision, Computer, Workspace, WorkspaceDelete, Store, or UI) to the `uses` clause",
        "E0302" => "unknown effect in `uses`: valid effects are LLM, Tool, IO, Decision, Computer, Workspace, WorkspaceDelete, Store, UI",
        "E0501" => "money literals keep their unit (100 USD + 100 EUR won't add) — convert explicitly before comparing or combining",
        "E0601" => "trace records must be JSONL objects with `v` (1), `seq` and a known `kind` — run `nudgec trace-check` to see the first bad record",
        "E0701" => "`state.x` writes are only valid inside an `agent` block, and only for fields declared in its `state` section",
        "E0702" => "a route block needs an `otherwise` arm so the program has a value on every path",
        "E0703" => "loop iteration limit exceeded - execution halted after 10000 iterations to prevent infinite loop",
        "E0704" => "an if expression used as a value must include an else branch so every path yields a value",
        "E0705" => "index out of bounds - check that the index is within the valid range",
        "E0706" => "cannot assign to immutable local - declare it with `let mut` to allow reassignment",
        "E0801" => "`for_all` properties only live inside `test` blocks — wrap it in `test \"...\" { ... }`",
        "E0802" => "unknown generator or wrong arity — valid: gen.int(lo, hi), gen.str(max), gen.injection(), gen.bool()",
        "E0804" => "properties must be pure: no llm/tool/decide/computer calls inside `for_all` — decide on recorded traces or fake providers instead",
        "E0806" => "a decide question references an unknown or mistyped option — option labels must match the `choose [...]` list exactly",
        "E0807" => "too many options — choice questions take 1–255 options (20+ degrades family accuracy, W0005), rubrics take 2–10 levels",
        "E0901" => "a computer call references an unknown method or option — known methods: observe/click/type/key/scroll/set_value/drag/perform/paste/screen_size/window_info/mouse_move/mouse_down/mouse_up/cursor_pos; options: allow, deadline, screenshot",
        "E0902" => "a computer call gets the wrong number of arguments — observe takes the app name; each action takes its target/payload (docs/computer-use.md)",
        "E0910" => "workspace path escape denied — the path escapes the workspace root via `../` traversal, an absolute path, or a symlink pointing outside; use a relative path inside the workspace",
        "E0912" => "workspace.delete requires the WorkspaceDelete effect: add `WorkspaceDelete` to the `uses` clause alongside `Workspace` to allow deletion",
        "E0913" => "store schema mismatch: data loaded from store does not match the requested type",
        "E1001" => "missing UI effect: operations that open windows, mount UI, or handle UI events require `uses UI`",
        "E1002" => "invalid or unknown UI component: check the element combinator name and its children or properties",
        "E1003" => "event handler signature mismatch: expected compatible handler function",
        "E1004" => "duplicate window or view declaration: every window and view name must be unique",
        "E-auth-0401" | "E0401" => "credential missing or uninitialized for the provider - set the required environment variable or authenticate",
        "E-auth-0402" | "E0402" => "device code flow expired, timed out, or was rejected by user",
        "E-auth-0403" | "E0403" => "account does not have the required scope or permissions",
        "E-auth-0404" | "E0404" => "provider adapter is disabled because no official public OAuth contract exists",
        "E-auth-0405" | "E0405" => "credential store is corrupted, insecure, or unreadable",
        "E-task-0001" => "task was cancelled — cooperative checkpoints prevent further execution",
        "E-task-0002" => "task failed during execution — inspect the inner error for failure details",
        "E-task-0003" => "invalid or corrupt task handle — verify task handle origin and lifecycle",
        _ => return None,
    })
}

/// Levenshtein distance, early-exit banded (names are short; a full DP
/// would also be fine, but bands keep it O(min(n,m)) per row).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The candidate nearest to `name` within a small edit distance — used to
/// append ", did you mean 'x'?" to unknown-name errors. Exact prefix
/// matches win over raw distance (typos usually keep the head).
pub fn closest<'a>(name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let mut best: Option<(usize, &String)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let cl = c.to_ascii_lowercase();
        let d = if cl.starts_with(&lower) || lower.starts_with(&cl) {
            1 // a prefix typo is almost always the intended name
        } else {
            edit_distance(&lower, &cl)
        };
        if d <= 2 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checker_code_has_a_hint() {
        for code in [
            "E0001", "E0002", "E0101", "E0102", "E0103", "E0201", "E0202", "E0203", "E0301",
            "E0302", "E0501", "E0601", "E0701", "E0702", "E0703", "E0704", "E0705", "E0706",
            "E0801", "E0802", "E0804", "E0806", "E0807", "E0901", "E0902", "E0910", "E0912",
            "E0913", "E1001", "E1002", "E1003", "E1004",
        ] {
            assert!(hint(code).is_some(), "{code} has no hint");
            assert!(
                !hint(code).unwrap().contains('\n'),
                "{code} hint is multi-line"
            );
        }
        assert!(hint("E9999").is_none());
    }

    #[test]
    fn auth_codes_have_hints() {
        for code in [
            "E-auth-0401",
            "E-auth-0402",
            "E-auth-0403",
            "E-auth-0404",
            "E-auth-0405",
            "E0401",
            "E0402",
            "E0403",
            "E0404",
            "E0405",
        ] {
            assert!(hint(code).is_some(), "{code} has no hint");
            assert!(
                !hint(code).unwrap().contains('\n'),
                "{code} hint is multi-line"
            );
            assert!(hint(code).unwrap().len() <= 180, "{code} hint is too long");
        }
    }

    #[test]
    fn closest_suggests_typos_and_prefixes_but_not_far_names() {
        let cands = [
            "triage".to_string(),
            "translate".to_string(),
            "handle".to_string(),
        ];
        assert_eq!(closest("triagee", cands.iter()).as_deref(), Some("triage"));
        assert_eq!(closest("tran", cands.iter()).as_deref(), Some("translate"));
        assert_eq!(closest("zzzzzz", cands.iter()), None);
    }
}
