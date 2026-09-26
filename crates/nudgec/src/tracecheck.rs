//! Frozen trace format validator (design §6, v1.0): the v1 record schema is
//! **frozen** — `nudgec trace-check <trace.jsonl>` validates every line
//! against it. Unknown versions report E0601; additive fields are allowed,
//! removed/renamed fields are not.

use crate::json::Json;

/// Required fields per record kind (additive optional fields like
/// `streamed`/`chunks`/`early_abort`/`route`/`server` are not listed).
fn required(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "llm.call" => Some(&[
            "model",
            "params",
            "input",
            "output",
            "tokens",
            "cost_usd",
            "repair_round",
            "outcome",
            "provider",
        ]),
        "tool.call" => Some(&["tool", "input", "output"]),
        "fn.return" => Some(&["fn", "output"]),
        _ => None,
    }
}

/// Expected type per frozen-v1 field: presence is not enough — a
/// `tokens.in` holding a string (or a hostile `<img onerror=...>`) must
/// be rejected by trace-check, not just rendered escaped downstream.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Ty {
    Str,
    Num,
    Obj,
}

fn field_type(kind: &str, field: &str) -> Option<Ty> {
    match (kind, field) {
        ("llm.call", "model")
        | ("llm.call", "input")
        | ("llm.call", "outcome")
        | ("llm.call", "provider") => Some(Ty::Str),
        ("llm.call", "cost_usd") | ("llm.call", "repair_round") => Some(Ty::Num),
        ("llm.call", "params") | ("llm.call", "tokens") => Some(Ty::Obj),
        // `output` is a string OR a structured object (typed answers)
        ("tool.call", "tool") | ("fn.return", "fn") => Some(Ty::Str),
        // dotted paths check nested fields — tokens.in must be a NUMBER,
        // so a hostile `<img onerror=...>` string is rejected at check time
        ("llm.call", "tokens.in") | ("llm.call", "tokens.out") => Some(Ty::Num),
        _ => None,
    }
}

fn type_check(kind: &str, rec: &Json) -> Vec<String> {
    let mut errs = Vec::new();
    let mut paths: Vec<(&str, Ty)> = Vec::new();
    for f in required(kind).unwrap_or(&[]) {
        if let Some(ty) = field_type(kind, f) {
            paths.push((f, ty));
        }
    }
    // nested dotted paths beyond the required list
    for (f, ty) in [("tokens.in", Ty::Num), ("tokens.out", Ty::Num)] {
        if field_type(kind, f).is_some() && !paths.iter().any(|(p, _)| *p == f) {
            paths.push((f, ty));
        }
    }
    for (f, ty) in paths {
        // walk dotted path ("tokens.in" → rec["tokens"]["in"])
        let mut cur = rec;
        let mut found = true;
        for part in f.split('.') {
            match cur.get(part) {
                Some(v) => cur = v,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if !found {
            continue;
        }
        let v = cur;
        let ok = match ty {
            Ty::Str => v.as_str().is_some(),
            Ty::Num => v.as_num().is_some(),
            Ty::Obj => v.is_obj(),
        };
        if !ok {
            let got = match v {
                Json::Str(_) => "string",
                Json::Num(_) => "number",
                Json::Bool(_) => "boolean",
                Json::Null => "null",
                j if j.is_obj() => "object",
                _ => "array",
            };
            errs.push(format!("`{f}` must be {ty:?}, got {got}"));
        }
    }
    errs
}

/// One human-readable problem per violation; empty means the trace conforms.
pub fn validate(text: &str) -> Vec<String> {
    let mut errs = Vec::new();
    let mut expect_seq = 1.0;
    // a line with a missing seq breaks the counter — re-baseline on the
    // next valid seq instead of cascading a second false "out of order"
    let mut seq_unknown = false;
    for (idx, line) in text.lines().enumerate() {
        let n = idx + 1;
        if line.trim().is_empty() {
            continue;
        }
        let rec = match crate::json::parse(line) {
            Ok(r) if r.is_obj() => r,
            Ok(_) => {
                errs.push(format!("line {n}: record is not a JSON object"));
                continue;
            }
            Err(e) => {
                errs.push(format!("line {n}: invalid JSON: {e}"));
                continue;
            }
        };
        match rec.get("v").and_then(Json::as_num) {
            Some(1.0) => {}
            Some(v) => errs.push(format!("line {n}: unsupported record version {v} (E0601)")),
            None => errs.push(format!("line {n}: missing or non-numeric `v`")),
        }
        match rec.get("seq").and_then(Json::as_num) {
            Some(s) if seq_unknown => {
                // resume numbering from here; the rest of the trace must
                // stay contiguous from s+1, so a jumpy tail still reports
                seq_unknown = false;
                expect_seq = s + 1.0;
            }
            Some(s) if s == expect_seq => expect_seq += 1.0,
            Some(s) => {
                errs.push(format!(
                    "line {n}: seq {s} out of order (expected {expect_seq})"
                ));
                expect_seq = s + 1.0;
            }
            None => {
                errs.push(format!("line {n}: missing or non-numeric `seq`"));
                seq_unknown = true;
            }
        }
        match rec.get("kind").and_then(Json::as_str) {
            None => errs.push(format!("line {n}: missing `kind`")),
            Some(kind) => match required(kind) {
                None => errs.push(format!("line {n}: unknown record kind '{kind}'")),
                Some(fields) => {
                    for f in fields {
                        if rec.get(f).is_none() {
                            errs.push(format!("line {n}: {kind} record missing `{f}`"));
                        }
                    }
                    for e in type_check(kind, &rec) {
                        errs.push(format!("line {n}: {kind} record {e}"));
                    }
                }
            },
        }
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llm_line(seq: u64) -> String {
        format!(
            r#"{{"v": 1, "seq": {seq}, "kind": "llm.call", "model": "m", "params": {{}}, "input": "p", "output": "o", "tokens": {{"in": 1, "out": 1}}, "cost_usd": 0.001, "repair_round": 0, "outcome": "ok", "provider": "fake"}}"#
        )
    }

    #[test]
    fn a_frozen_v1_trace_validates() {
        let text = format!(
            "{}\n{}\n{}\n",
            llm_line(1),
            r#"{"v": 1, "seq": 2, "kind": "tool.call", "tool": "web_search", "input": ["q"], "output": [], "server": "search"}"#,
            r#"{"v": 1, "seq": 3, "kind": "fn.return", "fn": "main", "output": {"x": 1}}"#,
        );
        assert_eq!(validate(&text), Vec::<String>::new());
    }

    #[test]
    fn wrong_typed_frozen_v1_fields_are_reported() {
        let text = format!(
            "{}\n{}\n",
            // tokens.in is a string (a hostile XSS payload must be caught
            // here, not just escaped downstream) and cost_usd is a string
            r#"{"v": 1, "seq": 1, "kind": "llm.call", "model": "m", "params": {}, "input": "p", "output": "o", "tokens": {"in": "<img src=x onerror=alert(1)>", "out": 1}, "cost_usd": "0.001", "repair_round": 0, "outcome": "ok", "provider": "fake"}"#,
            r#"{"v": 1, "seq": 2, "kind": "tool.call", "tool": 7, "input": [], "output": []}"#,
        );
        let errs = validate(&text);
        assert!(
            errs.iter().any(|e| e.contains("`tokens.in` must be Num")),
            "{errs:?}"
        );
        assert!(
            errs.iter().any(|e| e.contains("`cost_usd` must be Num")),
            "{errs:?}"
        );
        assert!(
            errs.iter().any(|e| e.contains("`tool` must be Str")),
            "{errs:?}"
        );
    }

    #[test]
    fn bad_version_kind_field_and_seq_are_reported() {
        let text = format!(
            "{}\n{}\n{}\n{}\n",
            r#"{"v": 2, "seq": 1, "kind": "llm.call"}"#,
            r#"{"v": 1, "seq": 2, "kind": "mystery"}"#,
            r#"{"v": 1, "seq": 3, "kind": "tool.call", "tool": "t"}"#,
            llm_line(7),
        );
        let errs = validate(&text);
        assert!(
            errs.iter()
                .any(|e| e.contains("line 1") && e.contains("E0601")),
            "{errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("line 1") && e.contains("missing `model`")),
            "{errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("line 2") && e.contains("unknown record kind")),
            "{errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("line 3") && e.contains("missing `input`")),
            "{errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("line 4") && e.contains("out of order")),
            "{errs:?}"
        );
    }

    #[test]
    fn a_missing_seq_rebaselines_instead_of_cascading() {
        // line 2 has no seq; line 3 must NOT inherit a stale expectation
        // and report a second, false "out of order"
        let text = format!(
            "{}\n{}\n{}\n",
            llm_line(1),
            r#"{"v": 1, "kind": "fn.return", "fn": "main", "output": 1}"#,
            llm_line(2),
        );
        let errs = validate(&text);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0].contains("line 2") && errs[0].contains("`seq`"),
            "{errs:?}"
        );
    }
}
