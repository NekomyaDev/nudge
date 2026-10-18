//! Policy sweep (v1.4, design §11.5): `nudgec policy-sweep <trace.jsonl>
//! --question dept --thresholds 0.5,0.8 [--json]` re-cuts decision thresholds over
//! the distributions ALREADY RECORDED in a trace -- zero model calls. The
//! "change a threshold, see the effect before deploy" loop from the
//! product plan: replay recorded confidence/p values, report auto-decide
//! coverage per threshold.

use crate::json::Json;

struct Row {
    value: f64,
    latency_ms: f64,
    outcome: String,
    deadline_set: bool,
}

enum SweepParseResult {
    Rows(Vec<Row>),
    MissingMetric,
    NoRecords,
}

fn collect_rows(text: &str, question: &str, metric: &str) -> SweepParseResult {
    let mut rows: Vec<Row> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(rec) = crate::json::parse(line) else {
            continue;
        };
        if !rec.is_obj() || rec.get("kind").and_then(Json::as_str) != Some("decision.call") {
            continue;
        }
        let Some(ans) = rec.get("answers").and_then(|a| a.get(question)) else {
            continue;
        };
        let Some(value) = ans.get(metric).and_then(Json::as_num) else {
            return SweepParseResult::MissingMetric;
        };
        rows.push(Row {
            value,
            latency_ms: rec.get("latency_ms").and_then(Json::as_num).unwrap_or(0.0),
            outcome: rec
                .get("outcome")
                .and_then(Json::as_str)
                .unwrap_or("ok")
                .to_string(),
            deadline_set: rec.get("deadline_ms").is_some(),
        });
    }
    if rows.is_empty() {
        SweepParseResult::NoRecords
    } else {
        rows.sort_by(|a, b| b.value.total_cmp(&a.value));
        SweepParseResult::Rows(rows)
    }
}

/// Sweep one question's decision values across thresholds.
pub fn sweep(text: &str, question: &str, metric: &str, thresholds: &[f64]) -> String {
    let rows = match collect_rows(text, question, metric) {
        SweepParseResult::MissingMetric => {
            return format!(
                "-- question '{question}' has no `{metric}` value (a noul answer only carries `p`; choice carries `p` and `confidence`)"
            );
        }
        SweepParseResult::NoRecords => {
            return format!("-- no decision.call records for question '{question}' in this trace");
        }
        SweepParseResult::Rows(rows) => rows,
    };

    let n = rows.len();
    let avg: f64 = rows.iter().map(|r| r.value).sum::<f64>() / n as f64;
    let total_latency: f64 = rows.iter().map(|r| r.latency_ms).sum();
    let missed = rows.iter().filter(|r| r.outcome != "ok").count();
    let mut out = format!(
        "question '{question}' ({metric}): {n} recorded decision(s), avg {avg:.3}, \
total latency {total_latency:.0} ms, {missed} non-ok outcome(s)\n"
    );
    if rows.iter().any(|r| r.deadline_set) {
        out.push_str("deadline_ms present on at least one record -- latency is policy-relevant\n");
    }
    out.push_str(&format!(
        "{:>10}  {:>8}  {:>8}\n",
        "threshold", "auto", "review"
    ));
    for t in thresholds {
        let auto = rows.iter().filter(|r| r.value >= *t).count();
        out.push_str(&format!(
            "{t:>10.2}  {:>7.1}%  {:>7.1}%\n",
            100.0 * auto as f64 / n as f64,
            100.0 * (n - auto) as f64 / n as f64
        ));
    }
    // the minimal failing case is the interesting one: the first row below
    // the lowest threshold is what a human would review first
    if !thresholds.is_empty() {
        let lowest = thresholds.iter().cloned().fold(f64::INFINITY, f64::min);
        if let Some(r) = rows.iter().find(|r| r.value < lowest) {
            out.push_str(&format!(
                "first human-review case: {metric}={:.3} (just below the lowest threshold {lowest:.2})\n",
                r.value
            ));
        }
    }
    out
}

/// Sweep one question's decision values across thresholds and emit structured JSON metrics.
pub fn sweep_json(text: &str, question: &str, metric: &str, thresholds: &[f64]) -> String {
    let rows = match collect_rows(text, question, metric) {
        SweepParseResult::MissingMetric => {
            let payload = serde_json::json!({
                "question": question,
                "metric": metric,
                "total_records": 0,
                "records": 0,
                "distribution": [],
                "error": format!("question '{question}' has no `{metric}` value (a noul answer only carries `p`; choice carries `p` and `confidence`)"),
            });
            return serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
        }
        SweepParseResult::NoRecords => {
            let payload = serde_json::json!({
                "question": question,
                "metric": metric,
                "total_records": 0,
                "records": 0,
                "distribution": [],
                "error": format!("no decision.call records for question '{question}' in this trace"),
            });
            return serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
        }
        SweepParseResult::Rows(rows) => rows,
    };

    let n = rows.len();
    let avg: f64 = rows.iter().map(|r| r.value).sum::<f64>() / n as f64;
    let total_latency: f64 = rows.iter().map(|r| r.latency_ms).sum();
    let missed = rows.iter().filter(|r| r.outcome != "ok").count();
    let deadline_relevant = rows.iter().any(|r| r.deadline_set);

    let mut distribution = Vec::new();
    for t in thresholds {
        let auto = rows.iter().filter(|r| r.value >= *t).count();
        let coverage = if n > 0 { auto as f64 / n as f64 } else { 0.0 };
        distribution.push(serde_json::json!({
            "threshold": *t,
            "coverage": coverage,
            "records": auto,
            "auto_records": auto,
            "review_records": n - auto,
            "review_coverage": if n > 0 { (n - auto) as f64 / n as f64 } else { 0.0 },
        }));
    }

    let first_human_review = if !thresholds.is_empty() {
        let lowest = thresholds.iter().cloned().fold(f64::INFINITY, f64::min);
        rows.iter().find(|r| r.value < lowest).map(|r| r.value)
    } else {
        None
    };

    let payload = serde_json::json!({
        "question": question,
        "metric": metric,
        "total_records": n,
        "records": n,
        "average": avg,
        "avg": avg,
        "total_latency_ms": total_latency,
        "avg_latency_ms": if n > 0 { total_latency / n as f64 } else { 0.0 },
        "non_ok_outcomes": missed,
        "deadline_relevant": deadline_relevant,
        "first_human_review": first_human_review,
        "distribution": distribution,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::{sweep, sweep_json};

    fn trace() -> String {
        let conf = |p: f64, lat: f64, outcome: &str| {
            format!(
                r#"{{"v":1,"seq":1,"kind":"decision.call","model":"m","provider":"laya","questions":{{}},"answers":{{"dept":{{"winner":"a","p":{p:.3},"distribution":{{}},"confidence":{p:.3}}}}},"latency_ms":{lat},"outcome":"{outcome}"}}"#
            )
        };
        format!(
            "{}\n{}\n{}\n{}\n",
            conf(0.9, 30.0, "ok"),
            conf(0.75, 35.0, "ok"),
            conf(0.55, 40.0, "ok"),
            conf(0.2, 90.0, "deadline_missed")
        )
    }

    #[test]
    fn sweep_reports_coverage_per_threshold() {
        let out = sweep(&trace(), "dept", "confidence", &[0.5, 0.8]);
        assert!(out.contains("4 recorded decision(s)"), "{out}");
        assert!(out.contains("total latency 195 ms"), "{out}");
        assert!(out.contains("1 non-ok outcome(s)"), "{out}");
        // 0.8: values 0.9 -> 1 auto (25%); 0.5: 0.9/0.75/0.55 -> 75%
        let row = |t: &str, auto: &str| {
            out.lines()
                .find(|l| l.trim_start().starts_with(t))
                .unwrap_or("")
                .contains(auto)
        };
        assert!(row("0.80", "25.0%") && row("0.50", "75.0%"), "{out}");
        assert!(
            out.contains("first human-review case: confidence=0.200"),
            "{out}"
        );
    }

    #[test]
    fn sweep_on_missing_metric_or_question_is_helpful() {
        assert!(sweep(&trace(), "dept", "nope", &[0.5]).contains("has no `nope`"));
        assert!(sweep("{}", "dept", "confidence", &[0.5]).contains("no decision.call records"));
    }

    #[test]
    fn sweep_with_empty_thresholds_does_not_panic() {
        let out = sweep(&trace(), "dept", "confidence", &[]);
        assert!(out.contains("4 recorded decision(s)"));
        assert!(!out.contains("first human-review case"));
    }

    #[test]
    fn sweep_picks_first_borderline_human_review_case() {
        let trace = format!("{}\n{}", trace(), "{\"v\":1,\"seq\":5,\"kind\":\"decision.call\",\"answers\":{\"dept\":{\"confidence\":0.45}},\"latency_ms\":10,\"outcome\":\"ok\"}");
        let out = sweep(&trace, "dept", "confidence", &[0.5]);
        assert!(
            out.contains(
                "first human-review case: confidence=0.450 (just below the lowest threshold 0.50)"
            ),
            "{out}"
        );
    }

    #[test]
    fn sweep_json_structured_metrics_and_distribution() {
        let json_str = sweep_json(&trace(), "dept", "confidence", &[0.5, 0.8]);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("sweep_json produces valid json");
        assert_eq!(parsed["question"], "dept");
        assert_eq!(parsed["metric"], "confidence");
        assert_eq!(parsed["total_records"], 4);
        assert_eq!(parsed["records"], 4);
        assert_eq!(parsed["total_latency_ms"], 195.0);
        assert_eq!(parsed["non_ok_outcomes"], 1);
        assert_eq!(parsed["first_human_review"], 0.2);

        let dist = parsed["distribution"]
            .as_array()
            .expect("distribution array");
        assert_eq!(dist.len(), 2);
        // 0.5: values 0.9, 0.75, 0.55 -> 3 records, 75% coverage
        assert_eq!(dist[0]["threshold"], 0.5);
        assert_eq!(dist[0]["coverage"], 0.75);
        assert_eq!(dist[0]["records"], 3);
        // 0.8: value 0.9 -> 1 record, 25% coverage
        assert_eq!(dist[1]["threshold"], 0.8);
        assert_eq!(dist[1]["coverage"], 0.25);
        assert_eq!(dist[1]["records"], 1);
    }

    #[test]
    fn sweep_json_handles_missing_metric_and_empty_records() {
        let missing_metric_json = sweep_json(&trace(), "dept", "nope", &[0.5]);
        let parsed1: serde_json::Value =
            serde_json::from_str(&missing_metric_json).expect("valid json");
        assert_eq!(parsed1["question"], "dept");
        assert_eq!(parsed1["metric"], "nope");
        assert_eq!(parsed1["total_records"], 0);
        assert!(parsed1["error"].as_str().unwrap().contains("has no `nope`"));

        let empty_trace_json = sweep_json("{}", "dept", "confidence", &[0.5]);
        let parsed2: serde_json::Value =
            serde_json::from_str(&empty_trace_json).expect("valid json");
        assert_eq!(parsed2["total_records"], 0);
        assert!(parsed2["error"]
            .as_str()
            .unwrap()
            .contains("no decision.call records"));
    }
}
