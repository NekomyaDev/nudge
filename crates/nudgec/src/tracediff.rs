//! Trace diff (v1.2): `nudgec trace-diff <a.jsonl> <b.jsonl>` compares two
//! frozen-v1 traces — totals (calls, tokens, cost, repairs) and per-record
//! deltas (outcome changes, token/cost drift, changed outputs). This is the
//! "what changed when I edited the prompt?" command: run before and after,
//! diff the traces.

use crate::json::{dumps, Json};

fn parse_trace(text: &str) -> Vec<Json> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| crate::json::parse(l).ok())
        .filter(Json::is_obj)
        .collect()
}

fn num(rec: &Json, key: &str) -> f64 {
    rec.get(key).and_then(Json::as_num).unwrap_or(0.0)
}

fn s(rec: &Json, key: &str) -> String {
    rec.get(key)
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

fn tokens_in_out(rec: &Json) -> (f64, f64) {
    let t = rec.get("tokens");
    let get = |k: &str| {
        t.and_then(|t| t.get(k))
            .and_then(Json::as_num)
            .unwrap_or(0.0)
    };
    (get("in"), get("out"))
}

struct Totals {
    llm: usize,
    tools: usize,
    tin: f64,
    tout: f64,
    cost: f64,
    repairs: usize,
}

fn totals(recs: &[Json]) -> Totals {
    let mut t = Totals {
        llm: 0,
        tools: 0,
        tin: 0.0,
        tout: 0.0,
        cost: 0.0,
        repairs: 0,
    };
    for r in recs {
        match s(r, "kind").as_str() {
            "llm.call" => {
                t.llm += 1;
                let (i, o) = tokens_in_out(r);
                t.tin += i;
                t.tout += o;
                t.cost += num(r, "cost_usd");
                if num(r, "repair_round") > 0.0 {
                    t.repairs += 1;
                }
            }
            "tool.call" => t.tools += 1,
            _ => {}
        }
    }
    t
}

fn delta(a: f64, b: f64, suffix: &str) -> String {
    let d = b - a;
    if d.abs() < 1e-12 {
        String::new()
    } else if d > 0.0 {
        format!(" (+{d:.0}{suffix})")
    } else {
        format!(" ({d:.0}{suffix})")
    }
}

fn cost_delta(a: f64, b: f64) -> String {
    let d = b - a;
    if d.abs() < 1e-12 {
        String::new()
    } else if d > 0.0 {
        format!(" (+${d:.4})")
    } else {
        format!(" (-${:.4})", d.abs())
    }
}

fn preview(j: &Json, width: usize) -> String {
    let raw = match j {
        Json::Str(s) => s.clone(),
        other => dumps(other),
    };
    let one_line = raw.replace('\n', " ");
    if one_line.chars().count() > width {
        let cut: String = one_line.chars().take(width - 1).collect();
        format!("{cut}…")
    } else {
        one_line
    }
}

/// One human-readable report. Both traces are parsed leniently (invalid
/// lines are skipped) — schema enforcement is `trace-check`'s job.
pub fn diff(a_text: &str, b_text: &str) -> String {
    let a = parse_trace(a_text);
    let b = parse_trace(b_text);
    let mut out = String::new();

    let ta = totals(&a);
    let tb = totals(&b);
    out.push_str(&format!("records   {} -> {}\n", a.len(), b.len()));
    out.push_str(&format!(
        "llm calls {} -> {}   tool calls {} -> {}\n",
        ta.llm, tb.llm, ta.tools, tb.tools
    ));
    out.push_str(&format!(
        "tokens    {:.0} -> {:.0}{}   (in {:.0} -> {:.0}, out {:.0} -> {:.0})\n",
        ta.tin + ta.tout,
        tb.tin + tb.tout,
        delta(ta.tin + ta.tout, tb.tin + tb.tout, ""),
        ta.tin,
        tb.tin,
        ta.tout,
        tb.tout
    ));
    out.push_str(&format!(
        "cost      ${:.4} -> ${:.4}{}\n",
        ta.cost,
        tb.cost,
        cost_delta(ta.cost, tb.cost)
    ));
    out.push_str(&format!(
        "repairs   {} -> {}{}\n",
        ta.repairs,
        tb.repairs,
        delta(ta.repairs as f64, tb.repairs as f64, "")
    ));

    // per-record comparison, aligned by position (seq is 1..=n in a valid trace)
    let n = a.len().max(b.len());
    let mut changed = 0usize;
    for i in 0..n {
        match (a.get(i), b.get(i)) {
            (Some(ra), Some(rb)) => {
                let kind = s(ra, "kind");
                let label = match kind.as_str() {
                    "llm.call" => format!("llm.call {}", s(ra, "model")),
                    "tool.call" => format!("tool.call {}", s(ra, "tool")),
                    _ => format!("{} {}", kind, s(ra, "fn")),
                };
                let mut lines: Vec<String> = Vec::new();
                if s(ra, "kind") != s(rb, "kind") {
                    lines.push(format!(
                        "  kind      {} -> {}",
                        s(ra, "kind"),
                        s(rb, "kind")
                    ));
                }
                let oa = s(ra, "outcome");
                let ob = s(rb, "outcome");
                if oa != ob && (!oa.is_empty() || !ob.is_empty()) {
                    lines.push(format!("  outcome   {oa} -> {ob}"));
                }
                let (ia, oa_) = tokens_in_out(ra);
                let (ib, ob_) = tokens_in_out(rb);
                if (ia + oa_ - ib - ob_).abs() > 1e-12 {
                    lines.push(format!(
                        "  tokens    {:.0} -> {:.0}{}",
                        ia + oa_,
                        ib + ob_,
                        delta(ia + oa_, ib + ob_, "")
                    ));
                }
                let ca = num(ra, "cost_usd");
                let cb = num(rb, "cost_usd");
                if (ca - cb).abs() > 1e-12 {
                    lines.push(format!(
                        "  cost      ${ca:.4} -> ${cb:.4}{}",
                        cost_delta(ca, cb)
                    ));
                }
                let oa = ra.get("output");
                let ob = rb.get("output");
                if oa != ob {
                    lines.push("  output    CHANGED".to_string());
                    if let (Some(x), Some(y)) = (oa, ob) {
                        lines.push(format!("    - \"{}\"", preview(x, 72)));
                        lines.push(format!("    + \"{}\"", preview(y, 72)));
                    }
                }
                if !lines.is_empty() {
                    changed += 1;
                    out.push_str(&format!("\n#{} {}\n", i + 1, label));
                    for l in lines {
                        out.push_str(&l);
                        out.push('\n');
                    }
                }
            }
            (Some(ra), None) => {
                changed += 1;
                out.push_str(&format!("\n#{} {} — only in A\n", i + 1, s(ra, "kind")));
            }
            (None, Some(rb)) => {
                changed += 1;
                out.push_str(&format!("\n#{} {} — only in B\n", i + 1, s(rb, "kind")));
            }
            (None, None) => {}
        }
    }
    if changed == 0 {
        out.push_str("\n-- traces identical (per-record)\n");
    } else {
        out.push_str(&format!("\n-- {changed} record(s) differ\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llm(seq: u64, out: &str, tin: u64, tout: u64, cost: f64) -> String {
        format!(
            r#"{{"v": 1, "seq": {seq}, "kind": "llm.call", "model": "m", "params": {{}}, "input": "p", "output": {out}, "tokens": {{"in": {tin}, "out": {tout}}}, "cost_usd": {cost}, "repair_round": 0, "outcome": "ok", "provider": "fake"}}"#
        )
    }

    #[test]
    fn identical_traces_report_no_differences() {
        let t = format!("{}\n", llm(1, "\"x\"", 10, 5, 0.001));
        let r = diff(&t, &t);
        assert!(r.contains("traces identical"), "{r}");
    }

    #[test]
    fn changed_output_and_cost_are_reported() {
        let a = format!("{}\n", llm(1, "\"old\"", 10, 5, 0.001));
        let b = format!("{}\n", llm(1, "\"new\"", 12, 8, 0.002));
        let r = diff(&a, &b);
        assert!(r.contains("output    CHANGED"), "{r}");
        assert!(r.contains("tokens    15 -> 20 (+5)"), "{r}");
        assert!(r.contains("$0.0010 -> $0.0020 (+$0.0010)"), "{r}");
        assert!(r.contains("1 record(s) differ"), "{r}");
    }

    #[test]
    fn length_mismatch_is_reported() {
        let a = format!(
            "{}\n{}\n",
            llm(1, "\"x\"", 1, 1, 0.001),
            llm(2, "\"y\"", 1, 1, 0.001)
        );
        let b = format!("{}\n", llm(1, "\"x\"", 1, 1, 0.001));
        let r = diff(&a, &b);
        assert!(r.contains("only in A"), "{r}");
    }

    #[test]
    fn repair_counts_are_totalled() {
        let repaired = r#"{"v": 1, "seq": 1, "kind": "llm.call", "model": "m", "params": {}, "input": "p", "output": "o", "tokens": {"in": 1, "out": 1}, "cost_usd": 0.001, "repair_round": 2, "outcome": "ok", "provider": "fake"}"#;
        let clean = format!("{}\n", llm(1, "\"o\"", 1, 1, 0.001));
        let r = diff(&format!("{repaired}\n"), &clean);
        assert!(r.contains("repairs   1 -> 0 (-1)"), "{r}");
    }
}
