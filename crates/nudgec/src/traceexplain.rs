//! `nudgec explain <trace.jsonl>` — the human report over a recorded
//! run (B1): totals in one glance, the records a human should look at
//! (low-confidence decisions, failures, deadline misses), and an "all
//! clear" line when there is nothing to see. Read-only; schema
//! enforcement stays with `trace-check`.

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

/// The weakest typed answer of a decision record: (question, metric-name,
/// value). Choice questions rank by confidence, then p; score questions by
/// their spread of interest, so only choice/noul feed the review list.
fn weakest_answer(rec: &Json) -> Option<(String, String, f64)> {
    let answers = match rec.get("answers") {
        Some(Json::Obj(m)) => m,
        _ => return None,
    };
    let mut best: Option<(String, String, f64)> = None;
    for (name, a) in answers {
        for key in ["confidence", "p"] {
            if let Some(v) = a.get(key).and_then(Json::as_num) {
                if best.as_ref().is_none_or(|(_, _, bv)| v < *bv) {
                    best = Some((name.clone(), key.to_string(), v));
                }
                break;
            }
        }
    }
    best
}

/// One human-readable report.
pub fn explain(text: &str) -> String {
    let recs = parse_trace(text);
    let mut out = String::new();
    let mut llm = 0usize;
    let mut tools = 0usize;
    let mut decisions = 0usize;
    let mut tin = 0.0;
    let mut tout = 0.0;
    let mut cost = 0.0;
    let mut repairs = 0usize;
    let mut cache_hits = 0usize;
    let mut lat_total = 0.0;
    let mut slowest: Option<(String, f64)> = None;
    let mut review: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for (idx, r) in recs.iter().enumerate() {
        let line_no = idx + 1;
        match s(r, "kind").as_str() {
            "llm.call" => {
                llm += 1;
                let (i, o) = tokens_in_out(r);
                tin += i;
                tout += o;
                cost += num(r, "cost_usd");
                if num(r, "repair_round") > 0.0 {
                    repairs += 1;
                }
                let outcome = s(r, "outcome");
                if outcome == "error" {
                    failures.push(format!(
                        "  llm.call #{line_no}  model={}  {}",
                        s(r, "model"),
                        preview(r.get("error").unwrap_or(r), 90),
                    ));
                }
                if let Some(conf) = r.get("confidence").and_then(Json::as_num) {
                    if conf < 0.5 {
                        review.push(format!(
                            "  llm.call #{line_no}  model={}  confidence={conf:.2}  {}",
                            s(r, "model"),
                            preview(r.get("output").unwrap_or(r), 60),
                        ));
                    }
                }
            }
            "tool.call" => {
                tools += 1;
                if s(r, "outcome") == "error" {
                    failures.push(format!(
                        "  tool.call #{line_no}  server={}  fn={}  {}",
                        s(r, "server"),
                        s(r, "fn"),
                        preview(r.get("error").unwrap_or(r), 90),
                    ));
                }
            }
            "decision.call" => {
                decisions += 1;
                let lat = num(r, "latency_ms");
                lat_total += lat;
                if s(r, "cache") == "hit" {
                    cache_hits += 1;
                }
                match &slowest {
                    Some((_, l)) if *l >= lat => {}
                    _ => slowest = Some((s(r, "model"), lat)),
                }
                let outcome = s(r, "outcome");
                if outcome != "ok" {
                    failures.push(format!(
                        "  decision.call #{line_no}  model={}  outcome={outcome}",
                        s(r, "model"),
                    ));
                }
                if let Some((q, metric, v)) = weakest_answer(r) {
                    if v < 0.5 {
                        review.push(format!(
                            "  decision.call #{line_no}  model={}  question '{q}'  {metric}={v:.2}",
                            s(r, "model"),
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    out.push_str(&format!(
        "trace: {} record(s) — {llm} llm, {tools} tool, {decisions} decision\n",
        recs.len()
    ));
    if llm > 0 {
        out.push_str(&format!(
            "  tokens in {tin:.0}  out {tout:.0}   cost ${cost:.4} (pricing as recorded)\n"
        ));
    }
    if decisions > 0 {
        let avg = lat_total / decisions as f64;
        out.push_str(&format!(
            "  decision latency {lat_total:.0} ms total, {avg:.1} ms avg, cache hits {cache_hits}\n"
        ));
        if let Some((model, lat)) = slowest {
            out.push_str(&format!("  slowest decision: {model} at {lat:.0} ms\n"));
        }
    }
    if repairs > 0 {
        out.push_str(&format!("  repairs: {repairs}\n"));
    }

    if !failures.is_empty() {
        out.push_str("\nfailures (fix these first):\n");
        out.push_str(&failures.join("\n"));
        out.push('\n');
    }
    if !review.is_empty() {
        out.push_str("\nlow confidence (review these):\n");
        out.push_str(&review.join("\n"));
        out.push('\n');
    }
    if failures.is_empty() && review.is_empty() {
        out.push_str(
            "\nnothing to review: no failures, no deadline misses, no low-confidence answers.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::explain;

    fn llm(conf: Option<f64>) -> String {
        let c = conf
            .map(|c| format!(r#""confidence": {c},"#))
            .unwrap_or_default();
        format!(
            r#"{{"kind": "llm.call", "model": "m1", "outcome": "ok", {c} "tokens": {{"in": 100, "out": 20}}, "cost_usd": 0.002, "output": "some answer"}}"#
        )
    }

    fn decision(conf: f64, winner: &str, outcome: &str) -> String {
        format!(
            r#"{{"kind": "decision.call", "model": "laya:multi", "provider": "laya", "outcome": "{outcome}", "latency_ms": 51, "questions": {{"dept": {{"name": "dept", "kind": "choice"}}}}, "answers": {{"dept": {{"winner": "{winner}", "p": {conf:.2}, "confidence": {conf:.2}}}}}}}"#
        )
    }

    #[test]
    fn explain_totals_and_reports_the_weakest_decision() {
        let trace = [
            llm(None),
            decision(0.92, "billing", "ok"),
            decision(0.31, "security", "ok"),
        ]
        .join("\n");
        let rep = explain(&trace);
        assert!(
            rep.contains("3 record(s) — 1 llm, 0 tool, 2 decision"),
            "{rep}"
        );
        assert!(rep.contains("tokens in 100"), "{rep}");
        assert!(rep.contains("$0.0020"), "{rep}");
        assert!(rep.contains("102 ms total"), "{rep}");
        assert!(rep.contains("low confidence"), "{rep}");
        assert!(rep.contains("question 'dept'  confidence=0.31"), "{rep}");
        assert!(!rep.contains("nothing to review"), "{rep}");
    }

    #[test]
    fn explain_surfaces_failures_and_stays_calm_on_a_clean_trace() {
        let bad = r#"{"kind": "tool.call", "server": "kb", "fn": "retrieve", "outcome": "error", "error": "timeout"}"#.to_string();
        let rep = explain(&bad);
        assert!(rep.contains("failures (fix these first):"), "{rep}");
        assert!(rep.contains("tool.call #1"), "{rep}");

        let clean = explain(&decision(0.9, "billing", "ok"));
        assert!(clean.contains("nothing to review: no failures"), "{clean}");
        assert!(explain("").contains("0 record(s)"), "{clean}");
    }

    #[test]
    fn explain_counts_cache_hits_and_skips_other_kinds() {
        let mut d = decision(0.9, "billing", "ok");
        d = d.replace(
            r#""provider": "laya""#,
            r#""provider": "laya", "cache": "hit""#,
        );
        let trace = [
            d,
            r#"{"kind": "fn.return", "fn": "f", "output": "x"}"#.to_string(),
        ]
        .join("\n");
        let rep = explain(&trace);
        assert!(rep.contains("cache hits 1"), "{rep}");
        assert!(rep.contains("2 record(s)"), "{rep}");
    }
}
