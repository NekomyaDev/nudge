//! Static cost report (design §13, v0.4): walks each fn's AST, counts llm
//! call sites under flat fake pricing ($0.001/call), applies the
//! `retry: N with repair` multiplier for the worst case, and marks calls
//! inside `par map` bodies as runtime-dependent (× collection size).

use crate::ast::*;

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
    let mut out = String::from("cost report (fake pricing, $0.001 per call)\n");
    let mut total = Count::default();
    for item in items {
        match item {
            Item::Fn { name, body, .. } => {
                let c = count_body(body);
                line(&mut out, name, &c);
                total.add(&c);
            }
            Item::Agent { name, fns, .. } => {
                for f in fns {
                    if let Item::Fn { name: fn_name, body, .. } = f {
                        let c = count_body(body);
                        line(&mut out, &format!("{name}.{fn_name}"), &c);
                        total.add(&c);
                    }
                }
            }
            // test blocks replay recorded traces — zero live token cost
            Item::Test { .. } | Item::TypeAlias { .. } | Item::Tool { .. } => {}
        }
    }
    line(&mut out, "total", &total);
    out
}

fn line(out: &mut String, name: &str, c: &Count) {
    let note = if c.dynamic { "  (× collection size inside par map — runtime-dependent)" } else { "" };
    out.push_str(&format!(
        "  {name}: {} llm call site(s), min ${:.3}, max ${:.3}{note}\n",
        c.sites,
        c.sites as f64 * FAKE_CALL_COST,
        c.max_calls as f64 * FAKE_CALL_COST,
    ));
}

fn count_body(body: &[Stmt]) -> Count {
    let mut c = Count::default();
    for st in body {
        match st {
            Stmt::Let { value, .. } => count_expr(value, false, &mut c),
            Stmt::StateWrite { value, .. } => count_expr(value, false, &mut c),
            Stmt::Assert(e) | Stmt::ExprStmt(e) => count_expr(e, false, &mut c),
        }
    }
    c
}

fn count_expr(e: &Expr, in_par: bool, c: &mut Count) {
    match e {
        Expr::LlmCall { prompt, options, repair } => {
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
        Expr::Prompt { .. } | Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Money(..)
        | Expr::Bool(_) | Expr::None | Expr::Ident(_) => {}
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
        Expr::ParMap { coll, kwargs, body, .. } => {
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
        let items = parse(lex(include_str!("../../../examples/research_agent.ndg")).unwrap()).unwrap();
        let r = report(&items);
        assert!(r.contains("cost report (fake pricing, $0.001 per call)"), "got:\n{r}");
        // analyze: 1 site, retry: 2 with repair → worst case 3 calls
        assert!(r.contains("analyze: 1 llm call site(s), min $0.001, max $0.003"), "got:\n{r}");
        // run: plan site (flat) + merge site (retry: 3 with repair → 4 calls)
        assert!(r.contains("run: 2 llm call site(s), min $0.002, max $0.005"), "got:\n{r}");
        assert!(r.contains("total: 3 llm call site(s), min $0.003, max $0.008"), "got:\n{r}");
        // the par map bodies call fns/tools, not llm directly — nothing dynamic
        assert!(!r.contains("runtime-dependent"), "got:\n{r}");
    }

    #[test]
    fn llm_sites_inside_par_map_are_marked_dynamic() {
        let src = "fn f(xs: [string]) -> [string] uses LLM {\n    par map xs |x| -> llm\"\"\"go {x}\"\"\" with { model: \"m\" }\n}";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(r.contains("f: 1 llm call site(s), min $0.001, max $0.001"), "got:\n{r}");
        assert!(r.contains("runtime-dependent"), "par map calls must be marked dynamic, got:\n{r}");
    }

    #[test]
    fn retry_with_repairs_multiplies_the_worst_case() {
        let src = "fn f() -> string uses LLM {\n    llm\"\"\"x\"\"\" with { model: \"m\", retry: 2 with repair }\n}";
        let r = report(&parse(lex(src).unwrap()).unwrap());
        assert!(r.contains("f: 1 llm call site(s), min $0.001, max $0.003"), "got:\n{r}");
        // retry without repair does not multiply
        let src2 = "fn f() -> string uses LLM {\n    llm\"\"\"x\"\"\" with { model: \"m\", retry: 2 }\n}";
        let r2 = report(&parse(lex(src2).unwrap()).unwrap());
        assert!(r2.contains("max $0.001"), "got:\n{r2}");
    }
}
