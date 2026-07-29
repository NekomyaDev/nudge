//! Static cost report (design §13, v0.4): walks each fn's AST, counts llm
//! call sites under flat fake pricing ($0.001/call), applies the
//! `retry: N with repair` multiplier for the worst case, and marks calls
//! inside `par map` bodies as runtime-dependent (× collection size).
//!
//! Per-fn lines report DIRECT sites; when a fn reaches more llm sites
//! through the fns it calls, an `(incl. callees: …)` annotation adds the
//! transitive worst case (v1.5 — a fn calling an llm-using fn used to
//! show a misleading $0.000). The `total` line sums direct counts only,
//! so nothing is double-counted.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

pub const FAKE_CALL_COST: f64 = 0.001;

#[derive(Default)]
struct Count {
    sites: usize,
    max_calls: usize,
    dynamic: bool,
}

impl Count {
    fn add(&mut self, other: &Count) {
        self.sites += other.sites;
        self.max_calls += other.max_calls;
        self.dynamic |= other.dynamic;
    }
}

/// `nudgec cost <file.ndg>` output: one line per fn plus a total.
pub fn report(items: &[Item]) -> String {
    // collect every fn: (display name, plain name, body)
    let mut fns: Vec<(String, String, &[Stmt])> = Vec::new();
    for item in items {
        match item {
            Item::Fn { name, body, .. } => fns.push((name.clone(), name.clone(), body)),
            Item::Agent {
                name,
                fns: agent_fns,
                ..
            } => {
                for f in agent_fns {
                    if let Item::Fn {
                        name: fn_name,
                        body,
                        ..
                    } = f
                    {
                        fns.push((format!("{name}.{fn_name}"), fn_name.clone(), body));
                    }
                }
            }
            Item::Test { .. } | Item::TypeAlias { .. } | Item::Tool { .. } => {}
        }
    }

    // direct counts + call graph (plain name → (callee, reached inside par map))
    let mut direct: HashMap<&str, Count> = HashMap::new();
    let mut graph: HashMap<&str, Vec<(String, bool)>> = HashMap::new();
    for (_, plain, body) in &fns {
        let mut c = Count::default();
        for st in body.iter() {
            match &st.kind {
                StmtKind::Let { value, .. } => count_expr(value, false, &mut c),
                StmtKind::StateWrite { value, .. } => count_expr(value, false, &mut c),
                StmtKind::Assert(e) | StmtKind::ExprStmt(e) => count_expr(e, false, &mut c),
            }
        }
        direct.insert(plain.as_str(), c);
        let mut edges = Vec::new();
        for st in body.iter() {
            match &st.kind {
                StmtKind::Let { value, .. } => collect_calls(value, false, &mut edges),
                StmtKind::StateWrite { value, .. } => collect_calls(value, false, &mut edges),
                StmtKind::Assert(e) | StmtKind::ExprStmt(e) => collect_calls(e, false, &mut edges),
            }
        }
        graph.insert(plain.as_str(), edges);
    }

    let mut out = String::from("cost report (fake pricing, $0.001 per call)\n");
    let mut total = Count::default();
    for (display, plain, _) in &fns {
        let c = &direct[plain.as_str()];
        let t = transitive(plain, &direct, &graph);
        line(&mut out, display, c, &t);
        total.add(c);
    }
    // the total line sums direct counts only — no transitive annotation
    // (nothing to double-count, nothing to re-attribute)
    line(&mut out, "total", &total, &total);
    out
}

/// Direct count + every llm site reachable through the call graph
/// (cycle-safe; each fn counted once). An edge crossed inside a
/// `par map` body makes the transitive count collection-size-dependent.
fn transitive(
    start: &str,
    direct: &HashMap<&str, Count>,
    graph: &HashMap<&str, Vec<(String, bool)>>,
) -> Count {
    let mut t = Count::default();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, bool)> = vec![(start, false)];
    while let Some((name, via_par)) = stack.pop() {
        if !seen.insert(name) {
            continue;
        }
        if let Some(c) = direct.get(name) {
            t.add(c);
            t.dynamic |= via_par;
        }
        if let Some(edges) = graph.get(name) {
            for (callee, par) in edges {
                stack.push((callee.as_str(), via_par || *par));
            }
        }
    }
    t
}

fn line(out: &mut String, name: &str, c: &Count, t: &Count) {
    let note = if c.dynamic {
        "  (× collection size inside par map — runtime-dependent)"
    } else {
        ""
    };
    out.push_str(&format!(
        "  {name}: {} llm call site(s), min ${:.3}, max ${:.3}{note}",
        c.sites,
        c.sites as f64 * FAKE_CALL_COST,
        c.max_calls as f64 * FAKE_CALL_COST,
    ));
    if t.sites != c.sites || t.max_calls != c.max_calls {
        let pdyn = if t.dynamic && !c.dynamic {
            ", × collection size via par map"
        } else {
            ""
        };
        out.push_str(&format!(
            "  (incl. callees: min ${:.3}, max ${:.3}{pdyn})",
            t.sites as f64 * FAKE_CALL_COST,
            t.max_calls as f64 * FAKE_CALL_COST,
        ));
    }
    out.push('\n');
}

/// Collect fn-call edges: `Expr::Call` on a plain identifier, excluding
/// builtins — the same walk as `count_expr`, tracking `par map` bodies.
fn collect_calls(e: &Expr, in_par: bool, out: &mut Vec<(String, bool)>) {
    match e {
        Expr::LlmCall {
            prompt, options, ..
        } => {
            collect_calls(prompt, in_par, out);
            for (_, v) in options {
                collect_calls(v, in_par, out);
            }
        }
        Expr::ListLit(xs) | Expr::ParAll(xs) | Expr::ParRace(xs) => {
            for x in xs {
                collect_calls(x, in_par, out);
            }
        }
        Expr::Prompt { .. }
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Money(..)
        | Expr::Bool(_)
        | Expr::None
        | Expr::Ident(_) => {}
        Expr::Call { func, args, kwargs } => {
            if let Expr::Ident(n) = func.as_ref() {
                if !matches!(n.as_str(), "len" | "zip" | "replay" | "mcp" | "python") {
                    out.push((n.clone(), in_par));
                }
            }
            collect_calls(func, in_par, out);
            for a in args {
                collect_calls(a, in_par, out);
            }
            for (_, v) in kwargs {
                collect_calls(v, in_par, out);
            }
        }
        Expr::Field { obj, .. } => collect_calls(obj, in_par, out),
        Expr::Binary { l, r, .. } | Expr::Merge { l, r } => {
            collect_calls(l, in_par, out);
            collect_calls(r, in_par, out);
        }
        Expr::Unary { x, .. } => collect_calls(x, in_par, out),
        Expr::ParMap {
            coll, kwargs, body, ..
        } => {
            collect_calls(coll, in_par, out);
            for (_, v) in kwargs {
                collect_calls(v, in_par, out);
            }
            // calls in the lambda run once per element — dynamic edge
            collect_calls(body, true, out);
        }
        Expr::Route { arms } => {
            for (_, _, cond) in arms {
                if let Some(x) = cond {
                    collect_calls(x, in_par, out);
                }
            }
        }
    }
}

fn count_expr(e: &Expr, in_par: bool, c: &mut Count) {
    match e {
        Expr::LlmCall {
            prompt,
            options,
            repair,
        } => {
            c.sites += 1;
            // the §4.2 repair loop re-calls up to `retry` times when set
            let retry = if *repair {
                options
                    .iter()
                    .find(|(k, _)| k == "retry")
                    .and_then(|(_, v)| match v {
                        Expr::Int(n) => Some(*n as usize),
                        _ => None,
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            c.max_calls += 1 + retry;
            c.dynamic |= in_par;
            count_expr(prompt, in_par, c);
            for (_, v) in options {
                count_expr(v, in_par, c);
            }
        }
        Expr::ListLit(xs) | Expr::ParAll(xs) | Expr::ParRace(xs) => {
            for x in xs {
                count_expr(x, in_par, c);
            }
        }
        Expr::Prompt { .. }
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Money(..)
        | Expr::Bool(_)
        | Expr::None
        | Expr::Ident(_) => {}
        Expr::Call { func, args, kwargs } => {
            count_expr(func, in_par, c);
            for a in args {
                count_expr(a, in_par, c);
            }
            for (_, v) in kwargs {
                count_expr(v, in_par, c);
            }
        }
        Expr::Field { obj, .. } => count_expr(obj, in_par, c),
        Expr::Binary { l, r, .. } | Expr::Merge { l, r } => {
            count_expr(l, in_par, c);
            count_expr(r, in_par, c);
        }
        Expr::Unary { x, .. } => count_expr(x, in_par, c),
        Expr::ParMap {
            coll, kwargs, body, ..
        } => {
            count_expr(coll, in_par, c);
            for (_, v) in kwargs {
                count_expr(v, in_par, c);
            }
            // the lambda body runs once per element — calls in it are dynamic
            count_expr(body, true, c);
        }
        Expr::Route { arms } => {
            for (_, _, cond) in arms {
                if let Some(x) = cond {
                    count_expr(x, in_par, c);
                }
            }
        }
    }
}

// ── tests ────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    #[test]
    fn research_agent_cost_report() {
        let items =
            parse(lex(include_str!("../../../examples/research_agent.ndg")).unwrap()).unwrap();
        let r = report(&items);
        assert!(
            r.contains("cost report (fake pricing, $0.001 per call)"),
            "got:\n{r}"
        );
        // analyze: 1 site, retry: 2 with repair → worst case 3 calls
        assert!(
            r.contains("analyze: 1 llm call site(s), min $0.001, max $0.003"),
            "got:\n{r}"
        );
        // run: plan site (retry: 1 with repair → 2 calls) + merge site
        // (retry: 3 with repair → 4 calls)
        assert!(
            r.contains("run: 2 llm call site(s), min $0.002, max $0.006"),
            "got:\n{r}"
        );
        assert!(
            r.contains("total: 3 llm call site(s), min $0.003, max $0.009"),
            "got:\n{r}"
        );
        // the par map bodies call fns/tools, not llm directly — nothing dynamic
        assert!(!r.contains("runtime-dependent"), "got:\n{r}");
    }

    #[test]
    fn llm_sites_inside_par_map_are_marked_dynamic() {
        let src = "fn f(xs: [string]) -> [string] uses LLM {\n    par map xs |x| -> llm\"\"\"go {x}\"\"\" with { model: \"m\" }\n}";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(
            r.contains("f: 1 llm call site(s), min $0.001, max $0.001"),
            "got:\n{r}"
        );
        assert!(
            r.contains("runtime-dependent"),
            "par map calls must be marked dynamic, got:\n{r}"
        );
    }

    #[test]
    fn retry_with_repairs_multiplies_the_worst_case() {
        let src = "fn f() -> string uses LLM {\n    llm\"\"\"x\"\"\" with { model: \"m\", retry: 2 with repair }\n}";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(
            r.contains("f: 1 llm call site(s), min $0.001, max $0.003"),
            "got:\n{r}"
        );
        // retry without repair does not multiply
        let src2 =
            "fn f() -> string uses LLM {\n    llm\"\"\"x\"\"\" with { model: \"m\", retry: 2 }\n}";
        let r2 = report(&parse(lex(src2).unwrap()).unwrap());
        assert!(r2.contains("max $0.001"), "got:\n{r2}");
    }

    #[test]
    fn callee_costs_are_annotated_transitively() {
        // f calls g (an llm fn): f's line used to claim min $0.000
        let src = "fn g() -> string uses LLM {\n    llm\"\"\"go\"\"\" with { model: \"m\" }\n}\nfn f() -> string { g() }";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(
            r.contains("f: 0 llm call site(s), min $0.000, max $0.000"),
            "got:\n{r}"
        );
        assert!(
            r.contains("(incl. callees: min $0.001, max $0.001)"),
            "got:\n{r}"
        );
        // total stays a direct sum — no double counting
        assert!(r.contains("total: 1 llm call site(s)"), "got:\n{r}");
        // cycles don't hang
        let src = "fn a() -> string uses LLM { b() }\nfn b() -> string uses LLM {\n    llm\"\"\"go\"\"\" with { model: \"m\" }\n    a()\n}";
        let _ = report(&parse(lex(src).unwrap()).unwrap());
        // reaching an llm fn through par map marks the transitive line dynamic
        let src = "fn g(x: string) -> string uses LLM {\n    llm\"\"\"go {x}\"\"\" with { model: \"m\" }\n}\nfn f(xs: [string]) -> [string] uses LLM {\n    par map xs |x| -> g(x)\n}";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(r.contains("× collection size via par map"), "got:\n{r}");
    }
}

