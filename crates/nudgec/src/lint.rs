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
//!   W0005 option-count — a `decide` choice with >20 options: family
//!   decision heads degrade past ~20 options (Laya rejects >126 with 422)
//!   — narrow the candidate set first
//!   W0004 schema-without-repair — a `schema` with no `retry: N with repair`:
//!                           a violation raises at runtime instead of repairing

use crate::ast::{Expr, ExprKind, Item, Stmt, StmtKind, TypeExpr};

#[derive(Debug)]
pub struct Lint {
    pub code: &'static str,
    pub msg: String,
    /// B5: an insertion-based auto-fix. `at` is a byte offset in the
    /// source; fixes are applied bottom-up so offsets never shift.
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub at: usize,
    pub insert: String,
}

fn lint(code: &'static str, msg: impl Into<String>) -> Lint {
    Lint {
        code,
        msg: msg.into(),
        fix: None,
    }
}

fn lint_fixable(code: &'static str, msg: impl Into<String>, fix: Fix) -> Lint {
    Lint {
        code,
        msg: msg.into(),
        fix: Some(fix),
    }
}

/// Record type name → field names, collected from `type T = { ... }` items.
fn record_fields(items: &[Item]) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for it in items {
        if let Item::TypeAlias {
            name,
            ty: TypeExpr::Record(fields),
        } = it
        {
            out.push((
                name.clone(),
                fields.iter().map(|(f, _)| f.clone()).collect(),
            ));
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
            '{' => {
                in_hole = true;
                if in_word {
                    words += 1;
                    in_word = false;
                }
            }
            '}' => in_hole = false,
            c if c.is_whitespace() => {
                if in_word && !in_hole {
                    words += 1;
                }
                in_word = false;
            }
            _ => {
                if !in_hole {
                    in_word = true;
                }
            }
        }
    }
    if in_word && !in_hole {
        words += 1;
    }
    words
}

fn lint_llm_call(
    ctx: &str,
    prompt: &Expr,
    options: &[(String, Expr)],
    repair: bool,
    records: &[(String, Vec<String>)],
    out: &mut Vec<Lint>,
) {
    // insertion point for with-block options: right after the last
    // option expression (a leading comma keeps it always valid)
    let with_fix = |insert: String| -> Option<Fix> {
        options.last().map(|(_, v)| Fix {
            at: v.span.end,
            insert,
        })
    };
    // W0001: uncapped cost
    if !options.iter().any(|(k, _)| k == "budget") {
        let msg = format!("in {ctx}: llm call has no `budget` option — cost is uncapped; add `budget: N USD` to the with-block");
        match with_fix(", budget: 0.02 USD".to_string()) {
            Some(f) => out.push(lint_fixable("W0001", msg, f)),
            None => out.push(lint("W0001", msg)),
        }
    }
    // W0004: schema without repair — a validation failure raises instead
    // of entering the repair loop. Applies to streaming too: `stream let`
    // shares the §4.2 repair loop (an early-abort counts as a violation).
    if options.iter().any(|(k, _)| k == "schema") && !repair {
        let msg = format!("in {ctx}: `schema` without `retry: N with repair` — a schema violation raises at runtime instead of being repaired; add a repair loop or accept the crash");
        match with_fix(", retry: 2 with repair".to_string()) {
            Some(f) => out.push(lint_fixable("W0004", msg, f)),
            None => out.push(lint("W0004", msg)),
        }
    }
    let body = match &prompt.kind {
        ExprKind::Prompt { body, .. } => body.as_str(),
        ExprKind::Str(s) => s.as_str(),
        _ => return,
    };
    // W0002: vague prompt
    if prompt_words(body) < 4 {
        out.push(lint("W0002", format!("in {ctx}: prompt is fewer than 4 words — vague instructions produce vague output; state the task and the expected shape")));
    }
    // W0003: schema fields never mentioned in the prompt
    // the schema option is either a bare alias (`schema: Finding`) or a
    // one-element list of one (`schema: [Finding]`) — handle both
    let schema_ident = options
        .iter()
        .find(|(k, _)| k == "schema")
        .and_then(|(_, v)| match &v.kind {
            ExprKind::Ident(schema) => Some(schema.clone()),
            ExprKind::ListLit(xs) => match xs.first().map(|x| &x.kind) {
                Some(ExprKind::Ident(schema)) => Some(schema.clone()),
                _ => None,
            },
            _ => None,
        });
    if let Some(schema) = schema_ident {
        if let Some((_, fields)) = records.iter().find(|(n, _)| n == &schema) {
            // word-boundary match: a field name counts as "mentioned" only
            // as a standalone word — otherwise `in` matches "instruction"
            let words: Vec<String> = body
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .map(|w| w.to_lowercase())
                .filter(|w| !w.is_empty())
                .collect();
            let mentioned = fields
                .iter()
                .filter(|f| words.iter().any(|w| w == &f.to_lowercase()))
                .count();
            if !fields.is_empty() && mentioned == 0 {
                let fix = Fix {
                    // before the closing """ of the prompt
                    at: prompt.span.end - 3,
                    insert: format!("\n    return JSON with fields: {}", fields.join(", ")),
                };
                out.push(lint_fixable(
                    "W0003",
                    format!(
                        "in {ctx}: schema `{schema}` fields ({}) never appear in the prompt — tell the model the output contract, e.g. \"return JSON with fields: {}\"",
                        fields.join(", "),
                        fields.join(", ")
                    ),
                    fix,
                ));
            }
        }
    }
}

fn walk_expr(ctx: &str, e: &Expr, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
    match &e.kind {
        ExprKind::LlmCall {
            prompt,
            options,
            repair,
        } => {
            lint_llm_call(ctx, prompt, options, *repair, records, out);
            walk_expr(ctx, prompt, records, out);
            for (_, v) in options {
                walk_expr(ctx, v, records, out);
            }
        }
        ExprKind::DecideCall {
            questions,
            state,
            options,
        } => {
            for (name, q) in questions {
                if let crate::ast::DecisionQ::Choice { options: opts, .. } = q {
                    if opts.len() > 20 {
                        out.push(lint("W0005", format!("in {ctx}: question '{name}' has {} options — family decision heads degrade past ~20 (Laya rejects >126 with a 422); pre-filter the candidate set", opts.len())));
                    }
                }
            }
            walk_expr(ctx, state, records, out);
            for (_, v) in options {
                walk_expr(ctx, v, records, out);
            }
        }
        ExprKind::ListLit(xs) | ExprKind::ParAll(xs) | ExprKind::ParRace(xs) => {
            for x in xs {
                walk_expr(ctx, x, records, out);
            }
        }
        ExprKind::Call { func, args, kwargs } => {
            walk_expr(ctx, func, records, out);
            for a in args {
                walk_expr(ctx, a, records, out);
            }
            for (_, v) in kwargs {
                walk_expr(ctx, v, records, out);
            }
        }
        ExprKind::Field { obj, .. } => walk_expr(ctx, obj, records, out),
        ExprKind::Binary { l, r, .. } | ExprKind::Merge { l, r, .. } => {
            walk_expr(ctx, l, records, out);
            walk_expr(ctx, r, records, out);
        }
        ExprKind::Unary { x, .. } => walk_expr(ctx, x, records, out),
        ExprKind::ParMap {
            coll, kwargs, body, ..
        } => {
            walk_expr(ctx, coll, records, out);
            for (_, v) in kwargs {
                walk_expr(ctx, v, records, out);
            }
            walk_expr(ctx, body, records, out);
        }
        ExprKind::Route { arms } => {
            for (_, _, cond) in arms {
                if let Some(c) = cond {
                    walk_expr(ctx, c, records, out);
                }
            }
        }
        _ => {}
    }
}

fn walk_stmt(ctx: &str, s: &Stmt, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
    match &s.kind {
        StmtKind::Let { value, .. } | StmtKind::StateWrite { value, .. } => {
            walk_expr(ctx, value, records, out)
        }
        StmtKind::Assert(e) | StmtKind::ExprStmt(e) => walk_expr(ctx, e, records, out),
        // properties are pure (E0804) — prompt lints don't apply inside
        StmtKind::ForAll { .. } => {}
    }
}

/// Run all prompt-quality rules over the parsed items.
pub fn lint_items(items: &[Item]) -> Vec<Lint> {
    let records = record_fields(items);
    let mut out = Vec::new();
    fn walk_item(it: &Item, records: &[(String, Vec<String>)], out: &mut Vec<Lint>) {
        match it {
            Item::Fn { name, body, .. } => {
                let ctx = format!("fn {name}");
                for s in body {
                    walk_stmt(&ctx, s, records, out);
                }
            }
            Item::Test { name, body, .. } => {
                let ctx = format!("test {}", name);
                for s in body {
                    walk_stmt(&ctx, s, records, out);
                }
            }
            Item::Agent { name, fns, .. } => {
                for f in fns {
                    // prefix agent context onto fn context
                    match f {
                        Item::Fn {
                            name: fname, body, ..
                        } => {
                            let ctx = format!("agent {name} / fn {fname}");
                            for s in body {
                                walk_stmt(&ctx, s, records, out);
                            }
                        }
                        _ => walk_item(f, records, out),
                    }
                }
            }
            _ => {}
        }
    }
    for it in items {
        walk_item(it, &records, &mut out);
    }
    // identical warnings (same code + message — e.g. three uncapped calls in
    // one fn) collapse into one line with a repetition count, keeping the
    // output readable until spans give each warning its own position
    let mut deduped: Vec<Lint> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for l in out {
        if let Some(i) = deduped
            .iter()
            .position(|d| d.code == l.code && d.msg == l.msg)
        {
            counts[i] += 1;
            continue;
        }
        deduped.push(l);
        counts.push(1);
    }
    for (l, n) in deduped.iter_mut().zip(counts) {
        if n > 1 {
            l.msg = format!("{} (×{n})", l.msg);
        }
    }
    deduped
}

/// Apply every fix bottom-up (insertions only — existing offsets stay
/// valid). Deduplicates fixes at identical offsets.
pub fn apply_fixes(src: &str, lints: &[Lint]) -> String {
    let mut fixes: Vec<&Fix> = lints.iter().filter_map(|l| l.fix.as_ref()).collect();
    fixes.sort_by_key(|f| std::cmp::Reverse(f.at));
    let mut out = src.to_string();
    let mut i = 0usize;
    while i < fixes.len() {
        // fixes sharing an offset (e.g. W0001 + W0004 on the same with-block)
        // merge into one insertion
        let at = fixes[i].at;
        let mut insert = String::new();
        while i < fixes.len() && fixes[i].at == at {
            let part = &fixes[i].insert;
            // inserts carry their own leading comma — don't double it
            if !insert.is_empty() && !part.starts_with(',') {
                insert.push_str(", ");
            }
            insert.push_str(part);
            i += 1;
        }
        let at = at.min(out.len());
        out.insert_str(at, &insert);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::lexer::lex;
    use crate::parser::parse;

    const FIXABLE: &str = concat!(
        "type Answer = { summary: string, confidence: float }\n",
        "\n",
        "fn run(text: string) -> Answer uses LLM {\n",
        "    llm\"\"\"Summarize the text.\"\"\"\n",
        "    with { schema: Answer, model: \"openai:gpt-4o-mini\" }\n",
        "}\n",
    );

    #[test]
    fn apply_fixes_makes_the_sample_lint_clean_and_compilable() {
        let items = parse(lex(FIXABLE).unwrap()).unwrap();
        let lints = lint_items(&items);
        assert_eq!(lints.len(), 4, "W0001/W0002/W0003/W0004 all fire");
        let fixed = apply_fixes(FIXABLE, &lints);
        // the fixed program parses and lints clean
        let items2 = parse(lex(&fixed).unwrap()).unwrap();
        assert!(
            lint_items(&items2).is_empty(),
            "remaining: {:?}",
            lint_items(&items2)
                .iter()
                .map(|l| &l.code)
                .collect::<Vec<_>>()
        );
        // W0002's fix is nil (can't write your prompt for you) but W0002
        // itself is gone because the W0003 insertion added words
    }

    #[test]
    fn fixes_sharing_an_offset_merge_without_double_commas() {
        let items = parse(lex(FIXABLE).unwrap()).unwrap();
        let lints = lint_items(&items);
        let fixed = apply_fixes(FIXABLE, &lints);
        assert!(!fixed.contains(",, "), "{fixed}");
        assert!(!fixed.contains(", ,"), "{fixed}");
    }

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
        let loud = lints(&format!("{ty}fn f() -> string uses LLM {{\n    llm\"\"\"return JSON with fields: title and confidence\"\"\" with {{ model: \"fake\", schema: Smoke, budget: 0.01 USD, retry: 2 with repair }}\n}}"));
        assert!(!loud.iter().any(|l| l.code == "W0003"), "{loud:?}");
        assert!(loud.is_empty(), "{loud:?}");
    }

    #[test]
    fn schema_without_repair_warns_w0004() {
        let ty = "type S = { title: string }\n";
        let no_repair = lints(&format!("{ty}fn f() -> S uses LLM {{\n    llm\"\"\"return JSON with the title field\"\"\" with {{ model: \"fake\", schema: S, budget: 0.01 USD }}\n}}"));
        assert!(no_repair.iter().any(|l| l.code == "W0004"), "{no_repair:?}");
        let repaired = lints(&format!("{ty}fn f() -> S uses LLM {{\n    llm\"\"\"return JSON with the title field\"\"\" with {{ model: \"fake\", schema: S, budget: 0.01 USD, retry: 2 with repair }}\n}}"));
        assert!(repaired.is_empty(), "{repaired:?}");
    }

    #[test]
    fn w0003_uses_word_boundaries_not_substrings() {
        // field named `in` must not match the word "instruction"
        let ty = "type T = { inp: string }\n";
        let ls = lints(&format!("{ty}fn f() -> string uses LLM {{\n    llm\"\"\"follow the instructions carefully and answer well\"\"\" with {{ model: \"fake\", schema: T, budget: 0.01 USD }}\n}}"));
        assert!(
            ls.iter().any(|l| l.code == "W0003"),
            "substring false-negative guard: {ls:?}"
        );
        let ok = lints(&format!("{ty}fn f() -> string uses LLM {{\n    llm\"\"\"return JSON with the inp field please\"\"\" with {{ model: \"fake\", schema: T, budget: 0.01 USD }}\n}}"));
        assert!(
            !ok.iter().any(|l| l.code == "W0003"),
            "substring false positive: {ok:?}"
        );
    }

    #[test]
    fn warnings_carry_fn_context() {
        let ls = lints("fn analyze() -> string uses LLM {\n    llm\"\"\"do it\"\"\" with { model: \"fake\" }\n}");
        assert!(ls.iter().all(|l| l.msg.contains("in fn analyze")), "{ls:?}");
    }

    #[test]
    fn streaming_calls_get_w0004_too() {
        // stream let shares the §4.2 repair loop — schema without repair is
        // the same smell there
        let ty = "type S = { title: string }\n";
        let streamed = lints(&format!("{ty}fn f() -> string uses LLM {{\n    stream let a = llm\"\"\"return JSON with the title field\"\"\" with {{ model: \"fake\", schema: S, budget: 0.01 USD }}\n}}"));
        assert!(streamed.iter().any(|l| l.code == "W0004"), "{streamed:?}");
        let repaired = lints(&format!("{ty}fn f() -> string uses LLM {{\n    stream let a = llm\"\"\"return JSON with the title field\"\"\" with {{ model: \"fake\", schema: S, budget: 0.01 USD, retry: 2 with repair }}\n}}"));
        assert!(!repaired.iter().any(|l| l.code == "W0004"), "{repaired:?}");
    }

    #[test]
    fn identical_warnings_collapse_with_a_count() {
        let src = "fn f() -> string uses LLM {\n    let a = llm\"\"\"summarize this text please\"\"\" with { model: \"fake\" }\n    let b = llm\"\"\"summarize this text please\"\"\" with { model: \"fake\" }\n}";
        let ls = lints(src);
        let w1: Vec<_> = ls.iter().filter(|l| l.code == "W0001").collect();
        assert_eq!(w1.len(), 1, "{ls:?}");
        assert!(w1[0].msg.contains("×2"), "{}", w1[0].msg);
    }

    #[test]
    fn interpolations_do_not_count_as_words() {
        assert_eq!(prompt_words("summarize {text} briefly please"), 3);
        assert_eq!(prompt_words(""), 0);
    }
}
