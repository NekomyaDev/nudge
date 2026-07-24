//! Prompt Clippy (strategy backlog, v1.2 candidate): the compiler lints
//! `llm"""` blocks for quality smells. Warnings (W-codes), never errors —
//! they print to stderr on `check`/`build`/`build-ts` and never fail a build.
//!
//! Rules:
//!   W0001 no-budget       — llm call without a `budget` option (uncapped cost)
//!   W0002 vague-prompt    — prompt body has fewer than 4 words
//!   W0003 schema-silence  — a record `schema: T` whose fields never appear
//!                           in the prompt text (the model can't guess the
//!                           output contract it was never told)

use crate::ast::{Expr, Item, Stmt, TypeExpr};

#[derive(Debug)]
pub struct Lint {
    pub code: &'static str,
    pub msg: String,
}

fn lint(code: &'static str, msg: impl Into<String>) -> Lint {
    Lint { code, msg: msg.into() }
}

/// Record type name → field names, collected from `type T = { ... }` items.
fn record_fields(items: &[Item]) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for it in items {
        if let Item::TypeAlias { name, ty: TypeExpr::Record(fields) } = it {
            out.push((name.clone(), fields.iter().map(|(f, _)| f.clone()).collect()));
        }
    }
    out
}

/// Words of the prompt body with `{interpolation}` holes removed.
fn prompt_words(body: &str) -> usize {
    let mut words = 0;
    let mut in_hole = false;
    let mut in_word = false;
    for c in body.chars() {
        match c {
            '{' => { in_hole = true; if in_word { words += 1; in_word = false; } }
            '}' => in_hole = false,
            c if c.is_whitespace() => {
                if in_word && !in_hole { words += 1; }
                in_word = false;
            }
            _ => { if !in_hole { in_word = true; } }
        }
    }
    if in_word && !in_hole { words += 1; }
    words
}

fn lint_llm_call(
    prompt_body: Option<&str>,
    options: &[(String, Expr)],
    records: &[(String, Vec<String>)],
    out: &mut Vec<Lint>,
) {
    // W0001: uncapped cost
    if !options.iter().any(|(k, _)| k == "budget") {
        out.push(lint("W0001", "llm call has no `budget` option — cost is uncapped; add `budget: N USD` to the with-block"));
    }
    let Some(body) = prompt_body else { return };
    // W0002: vague prompt
    if prompt_words(body) < 4 {
        out.push(lint("W0002", "prompt is fewer than 4 words — vague instructions produce vague output; state the task and the expected shape"));
    }
    // W0003: schema fields never mentioned in the prompt
    if let Some((_, Expr::Ident(schema))) = options.iter().find(|(k, _)| k == "schema") {
        if let Some((_, fields)) = records.iter().find(|(n, _)| n == schema) {
            let lower = body.to_lowercase();
            let mentioned = fields.iter().filter(|f| lower.contains(&f.to_lowercase())).count();
            if !fields.is_empty() && mentioned == 0 {
                out.push(lint(
                    "W0003",
                    format!(
                        "schema `{schema}` fields ({}) never appear in the prompt — tell the model the output contract, e.g. \"return JSON with fields: {}\"",
                        fields.join(", "),
                        fields.join(", ")
                    ),
                ));
            }
        }
    }
}

fn walk_expr(e: &Expr, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
    match e {
        Expr::LlmCall { prompt, options, .. } => {
            let body = match prompt.as_ref() {
                Expr::Prompt { body, .. } => Some(body.as_str()),
                Expr::Str(s) => Some(s.as_str()),
                _ => None,
            };
            lint_llm_call(body, options, records, out);
            walk_expr(prompt, records, out);
            for (_, v) in options {
                walk_expr(v, records, out);
            }
        }
        Expr::ListLit(xs) | Expr::ParAll(xs) | Expr::ParRace(xs) => {
            for x in xs { walk_expr(x, records, out); }
        }
        Expr::Call { func, args, kwargs } => {
            walk_expr(func, records, out);
            for a in args { walk_expr(a, records, out); }
            for (_, v) in kwargs { walk_expr(v, records, out); }
        }
        Expr::Field { obj, .. } => walk_expr(obj, records, out),
        Expr::Binary { l, r, .. } | Expr::Merge { l, r, .. } => {
            walk_expr(l, records, out);
            walk_expr(r, records, out);
        }
        Expr::Unary { x, .. } => walk_expr(x, records, out),
        Expr::ParMap { coll, kwargs, body, .. } => {
            walk_expr(coll, records, out);
            for (_, v) in kwargs { walk_expr(v, records, out); }
            walk_expr(body, records, out);
        }
        Expr::Route { arms } => {
            for (_, _, cond) in arms {
                if let Some(c) = cond { walk_expr(c, records, out); }
            }
        }
        _ => {}
    }
}

fn walk_stmt(s: &Stmt, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
    match s {
        Stmt::Let { value, .. } | Stmt::StateWrite { value, .. } => walk_expr(value, records, out),
        Stmt::Assert(e) | Stmt::ExprStmt(e) => walk_expr(e, records, out),
    }
}

/// Run all prompt-quality rules over the parsed items.
pub fn lint_items(items: &[Item]) -> Vec<Lint> {
    let records = record_fields(items);
    let mut out = Vec::new();
    fn walk_item(it: &Item, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
        match it {
            Item::Fn { body, .. } | Item::Test { body, .. } => {
                for s in body { walk_stmt(s, records, out); }
            }
            Item::Agent { fns, .. } => {
                for f in fns { walk_item(f, records, out); }
            }
            _ => {}
        }
    }
    for it in items {
        walk_item(it, &records, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lints(src: &str) -> Vec<Lint> {
        let toks = crate::lexer::lex(src).unwrap();
        let items = crate::parser::parse(toks).unwrap();
        lint_items(&items)
    }

    #[test]
    fn no_budget_warns_w0001() {
        let ls = lints("fn f() -> string uses LLM {\n    llm\"\"\"summarize this text please\"\"\" with { model: \"fake\" }\n}");
        assert!(ls.iter().any(|l| l.code == "W0001"), "{ls:?}");
        assert!(!ls.iter().any(|l| l.code == "W0002"), "{ls:?}");
    }

    #[test]
    fn vague_prompt_warns_w0002() {
        let ls = lints("fn f() -> string uses LLM {\n    llm\"\"\"do it\"\"\" with { model: \"fake\", budget: 0.01 USD }\n}");
        assert!(ls.iter().any(|l| l.code == "W0002"), "{ls:?}");
        assert!(!ls.iter().any(|l| l.code == "W0001"), "{ls:?}");
    }

    #[test]
    fn schema_silence_warns_w0003_and_mentioning_fields_clears_it() {
        let ty = "type Smoke = { title: string, confidence: float }\n";
        let silent = lints(&format!("{ty}fn f() -> string uses LLM {{\n    llm\"\"\"analyze this thing carefully\"\"\" with {{ model: \"fake\", schema: Smoke, budget: 0.01 USD }}\n}}"));
        assert!(silent.iter().any(|l| l.code == "W0003"), "{silent:?}");
        let loud = lints(&format!("{ty}fn f() -> string uses LLM {{\n    llm\"\"\"return JSON with fields: title and confidence\"\"\" with {{ model: \"fake\", schema: Smoke, budget: 0.01 USD }}\n}}"));
        assert!(!loud.iter().any(|l| l.code == "W0003"), "{loud:?}");
        assert!(loud.is_empty(), "{loud:?}");
    }

    #[test]
    fn interpolations_do_not_count_as_words() {
        assert_eq!(prompt_words("summarize {text} briefly please"), 3);
        assert_eq!(prompt_words(""), 0);
    }
}
