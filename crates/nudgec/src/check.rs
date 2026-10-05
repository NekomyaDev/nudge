//! Nudge type checker core (design §3, §11) — roadmap day 4–6.
//!
//! Scope: alias resolution (records, lists, `@range`/`@format` refinements),
//! schema ↔ return-type agreement, interpolation existence, let/call/return
//! assignability. `Unknown` is the dynamic escape hatch: it is assignable to
//! and from everything, so MCP/Python interop never false-alarms.
//!
//! Diagnostics (design §11, v1.4): E0101 unknown identifier / type / effect
//! name, E0201 type mismatch, E0202 malformed refinement / alias cycle / bad
//! schema, E0301 effect used with no `uses` clause, E0302 `uses` clause too
//! narrow. Effects propagate transitively through user-fn calls (fixpoint);
//! `test` blocks are exempt (they exist to exercise effectful code).
//!
//! Deferred: optional/union types, flow analysis.

use crate::ast::*;
use std::collections::{BTreeSet, HashMap};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    pub code: &'static str,
    pub msg: String,
    /// Statement span the error was raised in (spanned AST, stage 1) —
    /// `None` for item-level diagnostics that have no single statement home.
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
enum Ty {
    Int,
    Float,
    Bool,
    Str,
    None_,
    Unknown, // dynamic escape hatch — permissive both ways
    List(Box<Ty>),
    Record(Vec<(String, Ty)>),
    Refine(Box<Ty>, String),
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::Bool => write!(f, "bool"),
            Ty::Str => write!(f, "string"),
            Ty::None_ => write!(f, "none"),
            Ty::Unknown => write!(f, "unknown"),
            Ty::List(t) => write!(f, "[{t}]"),
            Ty::Record(fs) => {
                let inner: Vec<String> = fs.iter().map(|(k, t)| format!("{k}: {t}")).collect();
                write!(f, "{{{}}}", inner.join(", "))
            }
            Ty::Refine(base, name) => write!(f, "{base} @{name}"),
        }
    }
}

#[derive(Default)]
struct Globals {
    aliases: HashMap<String, TypeExpr>,
    // name → ((param name, param type)…, return type) — kwargs checking
    // needs the names, not just the positional types
    fns: HashMap<String, (Vec<(String, TypeExpr)>, TypeExpr)>,
    tools: HashMap<String, (Vec<(String, TypeExpr)>, TypeExpr)>,
}

// ── type resolution ─────────────────────────────────────────────────

fn resolve(
    t: &TypeExpr,
    g: &Globals,
    visiting: &mut Vec<String>,
    errs: &mut Vec<CheckError>,
) -> Ty {
    match t {
        TypeExpr::Named(name) => match name.as_str() {
            "int" => Ty::Int,
            "float" => Ty::Float,
            "bool" => Ty::Bool,
            "string" => Ty::Str,
            "none" | "()" => Ty::None_,
            "bytes" | "timestamp" => Ty::Unknown, // post-MVP core types
            _ => {
                if let Some(builtin) = builtin_record_ty(name) {
                    return builtin;
                }
                if visiting.iter().any(|v| v == name) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0202",
                        msg: format!(
                            "cyclic type alias '{}'",
                            visiting.join(" → ") + " → " + name
                        ),
                    });
                    return Ty::Unknown;
                }
                match g.aliases.get(name) {
                    Some(body) => {
                        let body = body.clone();
                        visiting.push(name.clone());
                        let ty = resolve(&body, g, visiting, errs);
                        visiting.pop();
                        ty
                    }
                    None => {
                        let did = crate::hints::closest(name, g.aliases.keys())
                            .map(|c| format!(", did you mean '{c}'?"))
                            .unwrap_or_default();
                        errs.push(CheckError {
                            span: None,
                            code: "E0101",
                            msg: format!("unknown type '{name}'{did}"),
                        });
                        Ty::Unknown
                    }
                }
            }
        },
        TypeExpr::List(inner) => Ty::List(Box::new(resolve(inner, g, visiting, errs))),
        TypeExpr::Record(fields) => Ty::Record(
            fields
                .iter()
                .map(|(k, ft)| (k.clone(), resolve(ft, g, visiting, errs)))
                .collect(),
        ),
        TypeExpr::Refine(base, name, args) => {
            validate_refinement(name, args, errs);
            Ty::Refine(Box::new(resolve(base, g, visiting, errs)), name.clone())
        }
    }
}

fn validate_refinement(name: &str, args: &[Expr], errs: &mut Vec<CheckError>) {
    let numeric = |e: &Expr| match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        // `-1` parses as Unary{Sub, Int(1)} — negated literals are numeric
        ExprKind::Unary { x, .. } => {
            matches!(&x.kind, ExprKind::Int(_) | ExprKind::Float(_))
        }
        _ => false,
    };
    match name {
        "range" => {
            if !(args.len() == 2 && args.iter().all(numeric)) {
                errs.push(CheckError {
                    span: None,
                    code: "E0202",
                    msg: format!(
                        "@range expects 2 numeric bounds, e.g. @range(0, 1) — got {args:?}"
                    ),
                });
            }
        }
        "format" => {
            let ok = args.len() == 1
                && match &args[0].kind {
                    ExprKind::Ident(f) | ExprKind::Str(f) => matches!(f.as_str(), "url" | "email"),
                    _ => false,
                };
            if !ok {
                errs.push(CheckError {
                    span: None,
                    code: "E0202",
                    msg: format!("@format expects one of (url, email) — got {args:?}"),
                });
            }
        }
        _ => errs.push(CheckError {
            span: None,
            code: "E0202",
            msg: format!("unknown refinement '@{name}' (known: @range, @format)"),
        }),
    }
}

fn assignable(src: &Ty, dst: &Ty) -> bool {
    if matches!(src, Ty::Unknown) || matches!(dst, Ty::Unknown) {
        return true;
    }
    if src == dst {
        return true;
    }
    match (src, dst) {
        (Ty::Int, Ty::Float) => true, // numeric widening
        (Ty::List(a), Ty::List(b)) => assignable(a, b),
        (Ty::Record(a), Ty::Record(b)) => {
            a.len() == b.len()
                && b.iter().all(|(k, bt)| {
                    a.iter()
                        .find(|(ak, _)| ak == k)
                        .map(|(_, at)| assignable(at, bt))
                        .unwrap_or(false)
                })
        }
        (Ty::Refine(a, _), b) => assignable(a, b),
        (a, Ty::Refine(b, _)) => assignable(a, b),
        _ => false,
    }
}

fn elem_of(t: &Ty) -> Ty {
    match t {
        Ty::List(e) => (**e).clone(),
        _ => Ty::Unknown,
    }
}

// ── effect inference (design §3.2, v1.4) ──────────────────────────

const KNOWN_EFFECTS: [&str; 5] = ["LLM", "Tool", "IO", "Decision", "Computer"];

/// Builtin record types of the computer-use surface (v1.5): these names
/// resolve like user type aliases but need no declaration. Additive
/// runtime fields are typed as Unknown so consumers can read them without
/// false alarms.
fn builtin_record_ty(name: &str) -> Option<Ty> {
    let r = |fs: Vec<(&str, Ty)>| {
        Some(Ty::Record(
            fs.into_iter().map(|(k, t)| (k.to_string(), t)).collect(),
        ))
    };
    match name {
        "Observation" => r(vec![
            ("app", Ty::Str),
            ("state_id", Ty::Str),
            ("title", Ty::Str),
            (
                "elements",
                Ty::List(Box::new(builtin_record_ty("Element")?)),
            ),
            ("tree", Ty::Str),
            ("screenshot_hash", Ty::Str),
            ("screenshot", Ty::Str),
            ("drift", builtin_record_ty("Drift")?),
        ]),
        "Element" => r(vec![
            ("index", Ty::Int),
            ("role", Ty::Str),
            ("title", Ty::Str),
            ("value", Ty::Str),
            ("pressable", Ty::Bool),
            ("editable", Ty::Bool),
            ("focused", Ty::Bool),
            ("enabled", Ty::Bool),
            ("actions", Ty::List(Box::new(Ty::Str))),
            // diagnostic geometry (zcode rule): describes where the element
            // sits — never a click-coordinate source; Unknown because the
            // provider shape ([x, y, w, h]) is advisory
            ("bounds", Ty::Unknown),
        ]),
        "ActionResult" => r(vec![
            ("ok", Ty::Bool),
            ("outcome", Ty::Str),
            ("latency_ms", Ty::Int),
            ("error", Ty::Str),
            // dispatch receipt: False only when the bridge KNOWS the input
            // was NOT dispatched — the only safe-to-retry case
            ("action_sent", Ty::Bool),
        ]),
        "Drift" => r(vec![
            ("changed", Ty::Bool),
            ("screenshot_changed", Ty::Bool),
            ("added", Ty::Unknown),
            ("removed", Ty::Unknown),
            ("summary", Ty::Str),
        ]),
        _ => None,
    }
}

/// Collect the direct (non-transitive) effects of an expression, plus the
/// names of user fns it calls (call-graph edges for the fixpoint).
fn direct_effects(
    e: &Expr,
    g: &Globals,
    effects: &mut BTreeSet<String>,
    calls: &mut BTreeSet<String>,
) {
    match &e.kind {
        ExprKind::LlmCall { options, .. } => {
            effects.insert("LLM".into());
            for (_, v) in options {
                direct_effects(v, g, effects, calls);
            }
        }
        ExprKind::DecideCall { state, options, .. } => {
            effects.insert("Decision".into());
            direct_effects(state, g, effects, calls);
            for (_, v) in options {
                direct_effects(v, g, effects, calls);
            }
        }
        ExprKind::ComputerCall { args, kwargs, .. } => {
            effects.insert("Computer".into());
            for a in args {
                direct_effects(a, g, effects, calls);
            }
            for (_, v) in kwargs {
                direct_effects(v, g, effects, calls);
            }
        }
        ExprKind::Call { func, args, kwargs } => {
            if let ExprKind::Ident(name) = &func.as_ref().kind {
                match name.as_str() {
                    "replay" | "python" => {
                        effects.insert("IO".into());
                    }
                    "mcp" => {
                        effects.insert("Tool".into());
                    }
                    "len" | "zip" => {}
                    _ => {
                        if g.tools.contains_key(name) {
                            effects.insert("Tool".into());
                        } else if g.fns.contains_key(name) {
                            calls.insert(name.clone());
                        }
                    }
                }
            } else {
                direct_effects(func, g, effects, calls);
            }
            for a in args {
                direct_effects(a, g, effects, calls);
            }
            for (_, v) in kwargs {
                direct_effects(v, g, effects, calls);
            }
        }
        ExprKind::ListLit(xs) | ExprKind::ParAll(xs) | ExprKind::ParRace(xs) => {
            for x in xs {
                direct_effects(x, g, effects, calls);
            }
        }
        ExprKind::Field { obj, .. } => direct_effects(obj, g, effects, calls),
        ExprKind::Binary { l, r, .. } | ExprKind::Merge { l, r } => {
            direct_effects(l, g, effects, calls);
            direct_effects(r, g, effects, calls);
        }
        ExprKind::Route { arms } => {
            for (_, value, cond) in arms {
                direct_effects(value, g, effects, calls);
                if let Some(c) = cond {
                    direct_effects(c, g, effects, calls);
                }
            }
        }
        ExprKind::Unary { x, .. } => direct_effects(x, g, effects, calls),
        ExprKind::ParMap {
            coll, kwargs, body, ..
        } => {
            direct_effects(coll, g, effects, calls);
            for (_, v) in kwargs {
                direct_effects(v, g, effects, calls);
            }
            direct_effects(body, g, effects, calls);
        }
        _ => {}
    }
}

fn body_effects(
    body: &[Stmt],
    g: &Globals,
    effects: &mut BTreeSet<String>,
    calls: &mut BTreeSet<String>,
) {
    for st in body {
        match &st.kind {
            StmtKind::Let { value, .. } => direct_effects(value, g, effects, calls),
            StmtKind::StateWrite { value, .. } => direct_effects(value, g, effects, calls),
            StmtKind::Assert(e) | StmtKind::ExprStmt(e) => direct_effects(e, g, effects, calls),
            // for_all only survives checking inside test blocks, which
            // carry no effect signature — nothing to contribute here
            StmtKind::ForAll { .. } => {}
        }
    }
}

/// All fn items in a program, including fns nested inside `agent` blocks
/// (design §7) — they share the top-level namespace at MVP.
fn fn_items(items: &[Item]) -> Vec<&Item> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Fn { .. } => out.push(item),
            Item::Agent { fns, .. } => out.extend(fns.iter()),
            _ => {}
        }
    }
    out
}

/// Agent fn context: the agent name plus its declared state fields.
type AgentCtx<'a> = Option<(&'a str, &'a [(String, TypeExpr, Expr)])>;

/// Check one fn body. `agent_ctx` is `Some((agent_name, state_fields))` when
/// the fn lives inside an `agent` block: `state` becomes a record local and
/// state writes are validated against the declared fields. Outside an agent,
/// a state write is E0701 (design §7).
/// Generators available to `for_all` (design §6.4). Returns the Ty the
/// loop variable binds to, or None for an unknown generator/arity (E0802).
fn generator_ty(gen: &str, argc: usize) -> Option<Ty> {
    match (gen, argc) {
        ("int", 2) => Some(Ty::Int),
        ("str", 1) => Some(Ty::Str),
        ("injection", 0) => Some(Ty::Str),
        ("bool", 0) => Some(Ty::Bool),
        _ => None,
    }
}

/// A `for_all` body must be pure: no direct `llm"""` and no direct tool
/// call — properties run for dozens of cases and must stay deterministic
/// and token-free (E0804). Calls to plain fns are allowed.
fn expr_has_effect_call(
    e: &Expr,
    g: &Globals,
    inferred: &HashMap<String, BTreeSet<String>>,
) -> bool {
    match &e.kind {
        ExprKind::LlmCall { .. } | ExprKind::DecideCall { .. } | ExprKind::ComputerCall { .. } => {
            true
        }
        ExprKind::Call { func, args, kwargs } => {
            let callee = match &func.kind {
                ExprKind::Ident(n) => Some(n.clone()),
                _ => None,
            };
            let tool = callee
                .as_deref()
                .map(|n| g.tools.contains_key(n))
                .unwrap_or(false);
            // a call to a fn whose INFERRED effects are non-empty (directly
            // or transitively) is an effect call even if it declares none
            let effectful_fn = callee
                .and_then(|n| inferred.get(&n))
                .map(|eff| !eff.is_empty())
                .unwrap_or(false);
            tool || effectful_fn
                || args.iter().any(|a| expr_has_effect_call(a, g, inferred))
                || kwargs
                    .iter()
                    .any(|(_, v)| expr_has_effect_call(v, g, inferred))
        }
        ExprKind::ListLit(xs) => xs.iter().any(|a| expr_has_effect_call(a, g, inferred)),
        ExprKind::Field { obj, .. } => expr_has_effect_call(obj, g, inferred),
        ExprKind::Binary { l, r, .. } | ExprKind::Merge { l, r } => {
            expr_has_effect_call(l, g, inferred) || expr_has_effect_call(r, g, inferred)
        }
        ExprKind::Unary { x, .. } => expr_has_effect_call(x, g, inferred),
        ExprKind::ParMap {
            coll, kwargs, body, ..
        } => {
            expr_has_effect_call(coll, g, inferred)
                || kwargs
                    .iter()
                    .any(|(_, v)| expr_has_effect_call(v, g, inferred))
                || expr_has_effect_call(body, g, inferred)
        }
        ExprKind::ParAll(xs) | ExprKind::ParRace(xs) => {
            xs.iter().any(|a| expr_has_effect_call(a, g, inferred))
        }
        ExprKind::Route { arms } => arms.iter().any(|(_, value, cond)| {
            expr_has_effect_call(value, g, inferred)
                || cond
                    .as_ref()
                    .is_some_and(|c| expr_has_effect_call(c, g, inferred))
        }),
        _ => false,
    }
}

fn stmts_have_effect_call(
    body: &[Stmt],
    g: &Globals,
    inferred: &HashMap<String, BTreeSet<String>>,
) -> bool {
    body.iter().any(|st| match &st.kind {
        StmtKind::Let { value, .. } | StmtKind::StateWrite { value, .. } => {
            expr_has_effect_call(value, g, inferred)
        }
        StmtKind::Assert(e) | StmtKind::ExprStmt(e) => expr_has_effect_call(e, g, inferred),
        StmtKind::ForAll { args, body, .. } => {
            args.iter().any(|a| expr_has_effect_call(a, g, inferred))
                || stmts_have_effect_call(body, g, inferred)
        }
    })
}

/// Shared `for_all` checking (design §6.4): valid generator (E0802),
/// test-block confinement (E0801), purity (E0804), then the body with the
/// loop variable bound. `in_test` is false when one appears in fn/agent
/// bodies.
#[allow(clippy::too_many_arguments)]
fn check_for_all(
    var: &str,
    gen: &str,
    args: &[Expr],
    body: &[Stmt],
    locals: &HashMap<String, Ty>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
    in_test: bool,
    inferred: &HashMap<String, BTreeSet<String>>,
) {
    if !in_test {
        errs.push(CheckError {
            span: None,
            code: "E0801",
            msg: "for_all is only allowed inside test blocks — properties are test artifacts, not runtime control flow".into(),
        });
        return;
    }
    for a in args {
        let at = check_expr(a, locals, g, errs);
        if !assignable(&at, &Ty::Int) {
            errs.push(CheckError {
                span: None,
                code: "E0802",
                msg: format!("gen.{gen} arguments must be int bounds, got {at}"),
            });
        }
    }
    let Some(vt) = generator_ty(gen, args.len()) else {
        errs.push(CheckError {
            span: None,
            code: "E0802",
            msg: format!(
                "unknown generator 'gen.{gen}' — available: gen.int(lo, hi), gen.str(max_len), gen.injection(), gen.bool()"
            ),
        });
        return;
    };
    if stmts_have_effect_call(body, g, inferred) {
        errs.push(CheckError {
            span: None,
            code: "E0804",
            msg: "for_all bodies must be pure — llm calls and tool calls are not allowed inside a property (deterministic + token-free)".into(),
        });
    }
    let mut inner = locals.clone();
    inner.insert(var.to_string(), vt);
    check_test_body(body, &inner, g, errs, inferred);
}

/// Statement checking for test blocks and for_all bodies: lets with the
/// annotation rule, asserts must be bool, state writes rejected (E0701),
/// nested for_all recursion.
fn check_test_body(
    body: &[Stmt],
    locals: &HashMap<String, Ty>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
    inferred: &HashMap<String, BTreeSet<String>>,
) {
    let mut locals = locals.clone();
    for st in body {
        let before = errs.len();
        match &st.kind {
            StmtKind::Let {
                name, ty, value, ..
            } => {
                let vt = check_expr(value, &locals, g, errs);
                check_reserved_binding(name, errs);
                if let Some(ann) = ty {
                    let at = resolve(ann, g, &mut Vec::new(), errs);
                    if !assignable(&vt, &at) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("let '{name}' is annotated {at} but the value is {vt}"),
                        });
                    }
                }
                locals.insert(name.clone(), vt);
            }
            StmtKind::StateWrite { field, .. } => {
                errs.push(CheckError {
                    span: None,
                    code: "E0701",
                    msg: format!("state write 'state.{field}' outside an agent block — state exists only inside `agent` (design §7)"),
                });
            }
            StmtKind::Assert(e) => {
                let at = check_expr(e, &locals, g, errs);
                if !assignable(&at, &Ty::Bool) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("assert expects a bool condition, got {at}"),
                    });
                }
            }
            StmtKind::ExprStmt(e) => {
                check_expr(e, &locals, g, errs);
            }
            StmtKind::ForAll {
                var,
                gen,
                args,
                body,
            } => {
                check_for_all(var, gen, args, body, &locals, g, errs, true, inferred);
            }
        }
        for e in &mut errs[before..] {
            if e.span.is_none() {
                e.span = Some(st.span);
            }
        }
    }
}

fn check_fn_body(
    name: &str,
    params: &[Param],
    ret: &TypeExpr,
    body: &[Stmt],
    agent_ctx: AgentCtx<'_>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
) {
    let mut locals: HashMap<String, Ty> = HashMap::new();
    for p in params {
        locals.insert(p.name.clone(), resolve(&p.ty, g, &mut Vec::new(), errs));
    }
    if let Some((_, fields)) = agent_ctx {
        let rec = Ty::Record(
            fields
                .iter()
                .map(|(f, ty, _)| (f.clone(), resolve(ty, g, &mut Vec::new(), errs)))
                .collect(),
        );
        locals.insert("state".into(), rec);
    }
    let mut last_ty = Ty::None_;
    for st in body {
        let before = errs.len();
        match &st.kind {
            StmtKind::Let {
                name: n, ty, value, ..
            } => {
                let vt = check_expr(value, &locals, g, errs);
                check_reserved_binding(n, errs);
                let bound = match ty {
                    Some(ann) => {
                        let at = resolve(ann, g, &mut Vec::new(), errs);
                        if !assignable(&vt, &at) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("let '{n}' is annotated {at} but the value is {vt}"),
                            });
                        }
                        at
                    }
                    None => vt,
                };
                locals.insert(n.clone(), bound);
            }
            StmtKind::StateWrite { field, op, value } => {
                let vt = check_expr(value, &locals, g, errs);
                match agent_ctx {
                    None => errs.push(CheckError {
                        span: None,
                        code: "E0701",
                        msg: format!("state write 'state.{field}' outside an agent block — state exists only inside `agent` (design §7)"),
                    }),
                    Some((_, fields)) => {
                        match fields.iter().find(|(f, _, _)| f == field) {
                            None => errs.push(CheckError {
                                span: None,
                                code: "E0701",
                                msg: format!("agent state has no field '{field}' — declare it in the state block"),
                            }),
                            Some((_, fty, _)) => {
                                // `=`: the value must fit the declared type.
                                // `+=`/`-=`: list-concat / numeric add-sub —
                                // the runtime checkpoint stores whatever the
                                // write yields.
                                if matches!(op, StateOp::Set) {
                                    let want = resolve(fty, g, &mut Vec::new(), errs);
                                    if !assignable(&vt, &want) {
                                        errs.push(CheckError {
                                            span: None,
                                            code: "E0201",
                                            msg: format!("state field '{field}' is {want} but the value is {vt}"),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            StmtKind::Assert(e) => {
                let at = check_expr(e, &locals, g, errs);
                if !assignable(&at, &Ty::Bool) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("assert expects a bool condition, got {at}"),
                    });
                }
            }
            StmtKind::ExprStmt(e) => {
                last_ty = check_expr(e, &locals, g, errs);
            }
            StmtKind::ForAll {
                var,
                gen,
                args,
                body,
            } => {
                let _ = (var, gen, args, body);
                errs.push(CheckError {
                    span: None,
                    code: "E0801",
                    msg: "for_all is only allowed inside test blocks — properties are test artifacts, not runtime control flow".into(),
                });
            }
        }
        // spanned AST (stage 1): statement-level diagnostics point at their
        // statement unless check_expr attached something more specific later
        for e in &mut errs[before..] {
            if e.span.is_none() {
                e.span = Some(st.span);
            }
        }
    }
    let want = resolve(ret, g, &mut Vec::new(), errs);
    if !assignable(&last_ty, &want) {
        errs.push(CheckError {
            span: None,
            code: "E0201",
            msg: format!("fn '{name}' declares -> {want} but its body yields {last_ty}"),
        });
    }
}

// ── expression checking + inference ─────────────────────────────────

/// Bindings users must never take: `rt`/`nudge_runtime` collide with the
/// generated Python/TS, `state` is the agent-state receiver, `computer`
/// would hijack the native computer-use surface (`computer.observe(...)`).
const RESERVED_BINDINGS: [&str; 4] = ["rt", "nudge_runtime", "state", "computer"];

fn check_reserved_binding(name: &str, errs: &mut Vec<CheckError>) {
    if RESERVED_BINDINGS.contains(&name) {
        errs.push(CheckError {
            span: None,
            code: "E0103",
            msg: format!("'{name}' collides with a generated/runtime identifier — rename it"),
        });
    }
}

fn schema_expr_ty(e: &Expr, g: &Globals, errs: &mut Vec<CheckError>) -> Ty {
    let as_type = match &e.kind {
        ExprKind::Ident(n) => Some(TypeExpr::Named(n.clone())),
        ExprKind::ListLit(xs) if xs.len() == 1 => match &xs[0].kind {
            ExprKind::Ident(n) => Some(TypeExpr::List(Box::new(TypeExpr::Named(n.clone())))),
            _ => None,
        },
        _ => None,
    };
    match as_type {
        Some(t) => resolve(&t, g, &mut Vec::new(), errs),
        None => {
            errs.push(CheckError {
                span: None,
                code: "E0202",
                msg: format!(
                    "schema must be a type (e.g. schema: Report or schema: [Finding]) — got {e:?}"
                ),
            });
            Ty::Unknown
        }
    }
}

fn check_expr(
    e: &Expr,
    locals: &HashMap<String, Ty>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
) -> Ty {
    match &e.kind {
        ExprKind::Int(_) => Ty::Int,
        ExprKind::Float(_) => Ty::Float,
        ExprKind::Money(_, unit) => {
            // v0.1 speaks USD only (design §4.3)
            if unit != "USD" {
                errs.push(CheckError {
                    span: None,
                    code: "E0501",
                    msg: format!("unknown budget unit '{unit}' (v0.1 supports USD only)"),
                });
            }
            Ty::Float
        }
        ExprKind::Str(_) => Ty::Str,
        ExprKind::Prompt { interpolations, .. } => {
            // bare `llm"""..."""` without a with-block: run the same
            // interpolation-existence check the LlmCall arm does
            for name in interpolations {
                let root = name.split('.').next().unwrap_or(name);
                if !locals.contains_key(root) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown identifier '{name}' in prompt interpolation"),
                    });
                }
            }
            Ty::Str
        }
        ExprKind::Bool(_) => Ty::Bool,
        ExprKind::None => Ty::None_,
        ExprKind::Ident(n) => match locals.get(n) {
            Some(t) => t.clone(),
            None => {
                // fn/tool/alias names may be referenced as values (dynamic
                // interop stays permissive); anything else is E0101
                let known =
                    g.fns.contains_key(n) || g.tools.contains_key(n) || g.aliases.contains_key(n);
                if !known {
                    let registry = g.fns.keys().chain(g.tools.keys()).chain(g.aliases.keys());
                    let did = crate::hints::closest(n, registry)
                        .map(|c| format!(", did you mean '{c}'?"))
                        .unwrap_or_default();
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown identifier '{n}'{did}"),
                    });
                }
                Ty::Unknown
            }
        },
        ExprKind::ListLit(xs) => {
            // unify the element type: later elements must fit (or widen, so
            // [1, 2.5] is [float]); incompatible mixes are E0201 — they used
            // to be typed by the first element alone
            let mut elem = Ty::Unknown;
            for x in xs {
                let t = check_expr(x, locals, g, errs);
                if matches!(elem, Ty::Unknown) {
                    elem = t;
                } else if assignable(&t, &elem) {
                    // fits the running element type
                } else if assignable(&elem, &t) {
                    elem = t; // widen (int → float)
                } else {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!(
                            "list element {t} does not fit the list's element type {elem}"
                        ),
                    });
                }
            }
            Ty::List(Box::new(elem))
        }
        ExprKind::LlmCall {
            prompt, options, ..
        } => {
            if let ExprKind::Prompt { interpolations, .. } = &prompt.as_ref().kind {
                for name in interpolations {
                    let root = name.split('.').next().unwrap_or(name);
                    if !locals.contains_key(root) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0101",
                            msg: format!("unknown identifier '{name}' in prompt interpolation"),
                        });
                    }
                }
            }
            let mut schema_ty = None;
            for (k, v) in options {
                if k == "schema" {
                    schema_ty = Some(schema_expr_ty(v, g, errs));
                } else {
                    check_expr(v, locals, g, errs);
                }
            }
            schema_ty.unwrap_or(Ty::Str)
        }
        ExprKind::Call { func, args, kwargs } => {
            let arg_tys: Vec<Ty> = args
                .iter()
                .map(|a| check_expr(a, locals, g, errs))
                .collect();
            let kwarg_tys: Vec<(&String, Ty)> = kwargs
                .iter()
                .map(|(k, v)| (k, check_expr(v, locals, g, errs)))
                .collect();
            if let ExprKind::Ident(name) = &func.as_ref().kind {
                match name.as_str() {
                    "len" => {
                        if args.len() != 1 {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("'len' takes 1 argument(s), got {}", args.len()),
                            });
                        }
                        return Ty::Int;
                    }
                    "zip" => {
                        if args.len() != 2 {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("'zip' takes 2 argument(s), got {}", args.len()),
                            });
                        }
                        let a = elem_of(arg_tys.first().unwrap_or(&Ty::Unknown));
                        let b = elem_of(arg_tys.get(1).unwrap_or(&Ty::Unknown));
                        return Ty::List(Box::new(Ty::Record(vec![
                            ("first".into(), a),
                            ("second".into(), b),
                        ])));
                    }
                    "replay" | "mcp" | "python" => return Ty::Unknown,
                    _ => {}
                }
                let sig = g.fns.get(name).or_else(|| g.tools.get(name));
                if let Some((params, ret)) = sig {
                    if args.len() > params.len() {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "'{name}' takes {} argument(s), got {}",
                                params.len(),
                                args.len()
                            ),
                        });
                    } else {
                        // positional args fill the first params in order
                        for (at, (_, pt)) in arg_tys.iter().zip(params.iter()) {
                            let want = resolve(pt, g, &mut Vec::new(), errs);
                            if !assignable(at, &want) {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E0201",
                                    msg: format!("argument to '{name}' must be {want}, got {at}"),
                                });
                            }
                        }
                        // kwargs fill the rest by name — unknown, duplicate,
                        // mistyped and missing parameters are all diagnosed
                        let mut missing: Vec<&str> = params[args.len()..]
                            .iter()
                            .map(|(n, _)| n.as_str())
                            .collect();
                        for (k, vt) in &kwarg_tys {
                            match params.iter().position(|(pn, _)| pn == *k) {
                                None => errs.push(CheckError {
                                    span: None,
                                    code: "E0201",
                                    msg: format!("'{name}' has no parameter '{k}'"),
                                }),
                                Some(pos) if pos < args.len() => errs.push(CheckError {
                                    span: None,
                                    code: "E0201",
                                    msg: format!("parameter '{k}' of '{name}' got a value twice"),
                                }),
                                Some(pos) => {
                                    let want = resolve(&params[pos].1, g, &mut Vec::new(), errs);
                                    if !assignable(vt, &want) {
                                        errs.push(CheckError {
                                            span: None,
                                            code: "E0201",
                                            msg: format!("argument '{k}' to '{name}' must be {want}, got {vt}"),
                                        });
                                    }
                                    missing.retain(|m| *m != k.as_str());
                                }
                            }
                        }
                        if !missing.is_empty() {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "'{name}' takes {} argument(s), got {} (missing: {})",
                                    params.len(),
                                    args.len(),
                                    missing.join(", ")
                                ),
                            });
                        }
                    }
                    return resolve(ret, g, &mut Vec::new(), errs);
                }
                // a call to a name that is neither builtin nor declared
                let registry = g.fns.keys().chain(g.tools.keys());
                let did = crate::hints::closest(name, registry)
                    .map(|c| format!(", did you mean '{c}'?"))
                    .unwrap_or_default();
                errs.push(CheckError {
                    span: None,
                    code: "E0101",
                    msg: format!("unknown identifier '{name}' (called but never declared){did}"),
                });
            }
            Ty::Unknown
        }
        ExprKind::Field { obj, name } => match check_expr(obj, locals, g, errs) {
            Ty::Record(fs) => {
                // map-shaped record (v1.2.1): `{string: T}` is a string-keyed
                // map — any field name resolves to the value type, matching
                // the schema lowering in codegen
                if fs.len() == 1 && is_builtin_type_name(&fs[0].0) {
                    return fs[0].1.clone();
                }
                match fs.iter().find(|(k, _)| k == name) {
                    Some((_, t)) => t.clone(),
                    None => {
                        let did = crate::hints::closest(name, fs.iter().map(|(k, _)| k))
                            .map(|c| format!(", did you mean '{c}'?"))
                            .unwrap_or_default();
                        errs.push(CheckError {
                            span: None,
                            code: "E0101",
                            msg: format!("record has no field '{name}'{did}"),
                        });
                        Ty::Unknown
                    }
                }
            }
            _ => Ty::Unknown,
        },
        ExprKind::Binary { op, l, r } => {
            let lt = check_expr(l, locals, g, errs);
            let rt_ = check_expr(r, locals, g, errs);
            match op {
                // boolean connectives need bool operands — `1 and "x"` used
                // to type as bool while operating on nonsense
                BinOp::And | BinOp::Or => {
                    let word = if matches!(op, BinOp::And) {
                        "and"
                    } else {
                        "or"
                    };
                    for (side, t) in [("left", &lt), ("right", &rt_)] {
                        if !assignable(t, &Ty::Bool) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("'{word}' expects bool operands, {side} side is {t}"),
                            });
                        }
                    }
                    Ty::Bool
                }
                // order comparisons: numeric×numeric or string×string only
                // (`1 < "a"` used to type as bool while comparing nonsense)
                BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                    fn numeric(t: &Ty) -> bool {
                        match t {
                            // money literals lower to float (design §4.3)
                            Ty::Int | Ty::Float => true,
                            Ty::Refine(b, ..) => matches!(b.as_ref(), Ty::Int | Ty::Float),
                            _ => false,
                        }
                    }
                    let comparable = (numeric(&lt) && numeric(&rt_))
                        || (matches!(lt, Ty::Str) && matches!(rt_, Ty::Str))
                        || matches!(lt, Ty::Unknown)
                        || matches!(rt_, Ty::Unknown);
                    if !comparable {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("cannot order-compare {lt} and {rt_}"),
                        });
                    }
                    Ty::Bool
                }
                // equality: operands must be assignable to each other either
                // way (int/float widen — money literals are floats, so
                // 0.001 USD == 0.001 compares two floats)
                BinOp::Eq | BinOp::NotEq => {
                    if !assignable(&lt, &rt_) && !assignable(&rt_, &lt) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("cannot compare {lt} with {rt_} for equality"),
                        });
                    }
                    Ty::Bool
                }
                _ => {
                    // arithmetic is numeric-only; previously a non-numeric
                    // operand silently fell through to Ty::Unknown and
                    // `true + 1` / `"a" * 3` compiled with no diagnostic
                    let numeric_ty = |t: &Ty| match t {
                        Ty::Int | Ty::Float => true,
                        Ty::Refine(b, ..) => matches!(b.as_ref(), Ty::Int | Ty::Float),
                        _ => false,
                    };
                    if !numeric_ty(&lt) || !numeric_ty(&rt_) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "arithmetic needs numeric operands — got {lt} {op:?} {rt_}"
                            ),
                        });
                    }
                    match (&lt, &rt_) {
                        (Ty::Float, _) | (_, Ty::Float) => Ty::Float,
                        // `/` always yields float in the Python/TS runtimes
                        // (7 / 2 == 3.5) — typing it int would be a lie
                        (Ty::Int, Ty::Int) if matches!(op, BinOp::Div) => Ty::Float,
                        (Ty::Int, Ty::Int) => Ty::Int,
                        _ => Ty::Unknown,
                    }
                }
            }
        }
        ExprKind::Unary { op, x } => {
            let t = check_expr(x, locals, g, errs);
            match op {
                BinOp::Not => {
                    if !matches!(t, Ty::Bool | Ty::Unknown) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("'!' expects a bool operand — got {t}"),
                        });
                    }
                    Ty::Bool
                }
                _ => t,
            }
        }
        ExprKind::ParMap {
            coll,
            kwargs,
            params,
            body,
        } => {
            let ct = check_expr(coll, locals, g, errs);
            for (_, v) in kwargs {
                check_expr(v, locals, g, errs);
            }
            let mut inner = locals.clone();
            if params.len() == 2 {
                if let Ty::List(pair) = &ct {
                    if let Ty::Record(fs) = pair.as_ref() {
                        for (i, p) in params.iter().enumerate() {
                            let key = if i == 0 { "first" } else { "second" };
                            let t = fs
                                .iter()
                                .find(|(k, _)| k == key)
                                .map(|(_, t)| t.clone())
                                .unwrap_or(Ty::Unknown);
                            inner.insert(p.clone(), t);
                        }
                    }
                }
                for p in params {
                    inner.entry(p.clone()).or_insert(Ty::Unknown);
                }
            } else {
                for p in params {
                    inner.insert(p.clone(), elem_of(&ct));
                }
            }
            Ty::List(Box::new(check_expr(body, &inner, g, errs)))
        }
        ExprKind::ParAll(xs) => {
            let t = xs
                .first()
                .map(|x| check_expr(x, locals, g, errs))
                .unwrap_or(Ty::Unknown);
            for x in &xs[1.min(xs.len())..] {
                check_expr(x, locals, g, errs);
            }
            Ty::List(Box::new(t))
        }
        ExprKind::ParRace(xs) => {
            let t = xs
                .first()
                .map(|x| check_expr(x, locals, g, errs))
                .unwrap_or(Ty::Unknown);
            for x in &xs[1.min(xs.len())..] {
                check_expr(x, locals, g, errs);
            }
            t
        }
        // design §7 reducer join: two records (dict union) or two lists
        // (append-dedup); anything else is a type error
        ExprKind::Merge { l, r } => {
            let lt = check_expr(l, locals, g, errs);
            let rt_ = check_expr(r, locals, g, errs);
            match (&lt, &rt_) {
                (Ty::Record(_), Ty::Record(_)) => lt,
                (Ty::List(a), Ty::List(b)) => {
                    if !assignable(b, a) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("merge list element mismatch: {lt} vs {rt_}"),
                        });
                    }
                    lt
                }
                (Ty::Unknown, t) | (t, Ty::Unknown) => t.clone(),
                _ => {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!(
                            "merge expects two records or two lists, got {lt} | merge {rt_}"
                        ),
                    });
                    Ty::Unknown
                }
            }
        }
        // design §4.4: route arms — every `when` condition must be bool and
        // the block needs an `otherwise` fallback (E0702)
        ExprKind::Route { arms } => {
            let mut has_otherwise = false;
            let mut unified: Option<Ty> = None;
            for (label, value, cond) in arms {
                // v1.4: arm VALUES are typed — every arm must agree on the
                // result type (string arms keep model-routing semantics)
                let vt = check_expr(value, locals, g, errs);
                match &unified {
                    None => unified = Some(vt),
                    Some(first) => {
                        if !assignable(&vt, first) && !assignable(first, &vt) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "route arm '{label}' yields {vt} but the block yields {first}"
                                ),
                            });
                        }
                    }
                }
                match cond {
                    Some(c) => {
                        let ct = check_expr(c, locals, g, errs);
                        if !assignable(&ct, &Ty::Bool) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "route arm '{label}' has a non-bool when condition ({ct})"
                                ),
                            });
                        }
                    }
                    None => has_otherwise = true,
                }
            }
            if !has_otherwise {
                errs.push(CheckError {
                    span: None,
                    code: "E0702",
                    msg: "route block needs an `otherwise` arm — no model is chosen when every `when` is false (design §4.4)".into(),
                });
            }
            unified.unwrap_or(Ty::Str)
        }
        ExprKind::DecideCall {
            questions,
            state,
            options,
        } => {
            check_expr(state, locals, g, errs);
            for (key, val) in options {
                match key.as_str() {
                    "model" | "null_option" => {
                        if !matches!(val.kind, ExprKind::Str(_)) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0806",
                                msg: format!("decide option '{key}' must be a string"),
                            });
                        }
                    }
                    "deadline" => {
                        let t = check_expr(val, locals, g, errs);
                        if !matches!(t, Ty::Int | Ty::Float) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0806",
                                msg: "decide option 'deadline' must be milliseconds (int), e.g. deadline: 50".into(),
                            });
                        }
                    }
                    other => errs.push(CheckError {
                        span: None,
                        code: "E0806",
                        msg: format!(
                            "unknown decide option '{other}' (known: model, deadline, null_option)"
                        ),
                    }),
                }
            }
            let mut fields = Vec::new();
            for (name, q) in questions {
                let qf = match q {
                    DecisionQ::Choice { options, .. } => {
                        if options.is_empty() {
                            errs.push(CheckError {
                                span: None,
                                code: "E0807",
                                msg: format!("question '{name}' has no options"),
                            });
                        }
                        // family capability surface: Laya rejects >~126
                        // short-label options with a 422 and degrades past
                        // ~20 (the W0005 lint fires below); hard-fail here
                        if options.len() > 255 {
                            errs.push(CheckError {
                                span: None,
                                code: "E0807",
                                msg: format!(
                                    "question '{name}' has {} options — the family hard limit is 255; narrow the candidate set",
                                    options.len()
                                ),
                            });
                        }
                        vec![
                            ("winner".to_string(), Ty::Str),
                            ("p".to_string(), Ty::Float),
                            ("distribution".to_string(), Ty::Unknown),
                            ("confidence".to_string(), Ty::Float),
                        ]
                    }
                    DecisionQ::Noul { .. } => vec![("p".to_string(), Ty::Float)],
                    DecisionQ::Score { levels, .. } => {
                        // ordinal rubric: 2..=10 levels is the family norm
                        if levels.len() < 2 || levels.len() > 10 {
                            errs.push(CheckError {
                                span: None,
                                code: "E0807",
                                msg: format!(
                                    "question '{name}' has {} rubric levels — a score rubric needs 2..=10",
                                    levels.len()
                                ),
                            });
                        }
                        vec![
                            ("score".to_string(), Ty::Float),
                            ("distribution".to_string(), Ty::Unknown),
                        ]
                    }
                };
                fields.push((name.clone(), Ty::Record(qf)));
            }
            Ty::Record(fields)
        }
        ExprKind::ComputerCall {
            method,
            args,
            kwargs,
        } => {
            let known_options = ["allow", "deadline", "screenshot"];
            let result_ty = match method.as_str() {
                "observe" => {
                    if args.len() != 1 {
                        errs.push(CheckError {
                            span: None,
                            code: "E0902",
                            msg: format!(
                                "computer.observe takes 1 argument (the app name), got {}",
                                args.len()
                            ),
                        });
                    }
                    builtin_record_ty("Observation").unwrap()
                }
                "click" | "drag" | "set_value" | "type" | "key" | "scroll" | "perform"
                | "paste" => {
                    let min_args = match method.as_str() {
                        "click" | "type" | "key" | "paste" => 1,
                        "set_value" | "drag" | "perform" => 2,
                        _ => 2, // scroll(target, direction[, pages])
                    };
                    if args.len() < min_args {
                        errs.push(CheckError {
                            span: None,
                            code: "E0902",
                            msg: format!(
                                "computer.{method} takes at least {min_args} argument(s), got {}",
                                args.len()
                            ),
                        });
                    }
                    builtin_record_ty("ActionResult").unwrap()
                }
                other => {
                    // unreachable via the parser (COMPUTER_METHODS gates it);
                    // kept for ASTs built by other tools
                    errs.push(CheckError {
                        span: None,
                        code: "E0901",
                        msg: format!(
                            "unknown computer method '{other}' (known: observe, click, type, key, scroll, set_value, drag, perform, paste)"
                        ),
                    });
                    Ty::Unknown
                }
            };
            // targets are int element indices or {x, y} records, and text
            // payloads are strings — typed loosely (Unknown) so provider-side
            // shapes keep flowing through
            for a in args {
                check_expr(a, locals, g, errs);
            }
            for (k, v) in kwargs {
                if !known_options.contains(&k.as_str()) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0901",
                        msg: format!(
                            "unknown computer option '{k}' (known: allow, deadline, screenshot)"
                        ),
                    });
                    check_expr(v, locals, g, errs);
                    continue;
                }
                let t = check_expr(v, locals, g, errs);
                match k.as_str() {
                    "allow" if !matches!(t, Ty::List(_) | Ty::Unknown) => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0901",
                            msg: format!(
                                "computer option 'allow' must be a list of app names, got {t}"
                            ),
                        });
                    }
                    "deadline" if !matches!(t, Ty::Int | Ty::Float | Ty::Unknown) => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0901",
                            msg: "computer option 'deadline' must be milliseconds (int), e.g. deadline: 30000".into(),
                        });
                    }
                    "screenshot" if !matches!(t, Ty::Bool | Ty::Unknown) => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0901",
                            msg: format!("computer option 'screenshot' must be a bool, got {t}"),
                        });
                    }
                    _ => {}
                }
            }
            result_ty
        }
    }
}

// ── entry point ─────────────────────────────────────────────────────

/// Check a whole program. Returns every diagnostic found (deterministic order).
pub fn check(items: &[Item]) -> Vec<CheckError> {
    let mut g = Globals::default();
    let mut errs = Vec::new();

    // generated identifiers must stay owner-only: user code binding these
    // names silently breaks the emitted Python/TS (`rt.llm_call` resolving
    // to a user fn, `_state_A` shadowing a checkpoint object). `computer`
    // is reserved the same way — `computer.observe(...)` is the native
    // computer-use surface (v1.5), and a user binding would hijack it.
    const RESERVED: [&str; 4] = ["rt", "nudge_runtime", "state", "computer"];
    for item in items {
        let fn_name = match item {
            Item::Fn { name, .. } => Some(name),
            Item::Agent { .. } => None, // agent fns checked below via Item::Fn pass
            _ => None,
        };
        if let Some(name) = fn_name {
            if RESERVED.contains(&name.as_str())
                || name.starts_with("_state_")
                || name.starts_with("nudge_test_")
            {
                errs.push(CheckError {
                    span: None,
                    code: "E0103",
                    msg: format!(
                        "'{name}' collides with a generated/runtime identifier — rename it"
                    ),
                });
            }
        }
    }
    for item in items {
        match item {
            Item::TypeAlias { name, ty } => {
                if g.aliases.insert(name.clone(), ty.clone()).is_some() {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate type alias '{name}'"),
                    });
                }
            }
            Item::Fn {
                name, params, ret, ..
            } => {
                if g.fns
                    .insert(
                        name.clone(),
                        (
                            params
                                .iter()
                                .map(|p| (p.name.clone(), p.ty.clone()))
                                .collect(),
                            ret.clone(),
                        ),
                    )
                    .is_some()
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate fn '{name}'"),
                    });
                }
            }
            Item::Tool {
                name, params, ret, ..
            } => {
                if g.tools
                    .insert(
                        name.clone(),
                        (
                            params
                                .iter()
                                .map(|p| (p.name.clone(), p.ty.clone()))
                                .collect(),
                            ret.clone(),
                        ),
                    )
                    .is_some()
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate tool '{name}'"),
                    });
                }
            }
            Item::Agent { fns, .. } => {
                for f in fns {
                    if let Item::Fn {
                        name, params, ret, ..
                    } = f
                    {
                        if g.fns
                            .insert(
                                name.clone(),
                                (
                                    params
                                        .iter()
                                        .map(|p| (p.name.clone(), p.ty.clone()))
                                        .collect(),
                                    ret.clone(),
                                ),
                            )
                            .is_some()
                        {
                            errs.push(CheckError {
                                span: None,
                                code: "E0102",
                                msg: format!("duplicate fn '{name}' (agent fns share the top-level namespace)"),
                            });
                        }
                    }
                }
            }
            Item::Test { .. } => {}
        }
    }

    // resolve every alias body once, so unused-but-broken types still error
    let names: Vec<String> = g.aliases.keys().cloned().collect();
    for name in names {
        let body = g.aliases[&name].clone();
        resolve(&body, &g, &mut vec![name], &mut errs);
    }

    // ── effect inference (design §3.2) — computed after registration so
    // g.tools/g.fns are populated, BEFORE bodies are checked: the for_all
    // purity check needs it to reject calls to effectful fns (E0804);
    // signature verification over the same map happens after item checking
    let mut direct: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    for item in fn_items(items) {
        if let Item::Fn { name, body, .. } = item {
            let mut eff = BTreeSet::new();
            let mut calls = BTreeSet::new();
            body_effects(body, &g, &mut eff, &mut calls);
            direct.insert(name.clone(), eff);
            edges.insert(name.clone(), calls);
        }
    }
    // propagate effects along the call graph to a fixpoint (cycles converge:
    // sets are bounded by the 3 known effects)
    let mut inferred = direct;
    loop {
        let mut changed = false;
        for (name, callees) in &edges {
            let mut add = BTreeSet::new();
            for c in callees {
                if let Some(eff) = inferred.get(c) {
                    add.extend(eff.iter().cloned());
                }
            }
            let entry = inferred.get_mut(name).unwrap();
            for e in add {
                if entry.insert(e) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    for item in items {
        match item {
            Item::Fn {
                name,
                params,
                ret,
                body,
                ..
            } => {
                check_fn_body(name, params, ret, body, None, &g, &mut errs);
            }
            Item::Agent {
                name: agent,
                state,
                fns,
            } => {
                // resolve state field types once; unknown types already error
                for (fname, ty, default) in state.iter() {
                    let rt = resolve(ty, &g, &mut Vec::new(), &mut errs);
                    let vt = check_expr(default, &HashMap::new(), &g, &mut errs);
                    if !assignable(&vt, &rt) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "agent state field '{fname}' is {rt} but its default is {vt}"
                            ),
                        });
                    }
                }
                for f in fns {
                    if let Item::Fn {
                        name,
                        params,
                        ret,
                        body,
                        ..
                    } = f
                    {
                        check_fn_body(name, params, ret, body, Some((agent, state)), &g, &mut errs);
                    }
                }
            }
            Item::Tool {
                params,
                ret,
                fields,
                ..
            } => {
                let mut locals: HashMap<String, Ty> = HashMap::new();
                for p in params {
                    locals.insert(
                        p.name.clone(),
                        resolve(&p.ty, &g, &mut Vec::new(), &mut errs),
                    );
                }
                resolve(ret, &g, &mut Vec::new(), &mut errs);
                for (_, v) in fields {
                    check_expr(v, &locals, &g, &mut errs);
                }
            }
            Item::Test { body, .. } => {
                let locals: HashMap<String, Ty> = HashMap::new();
                check_test_body(body, &locals, &g, &mut errs, &inferred);
            }
            Item::TypeAlias { .. } => {}
        }
    }

    // ── signature verification over the inferred effects (design §3.2) ────
    for item in fn_items(items) {
        if let Item::Fn {
            name,
            effects: declared,
            ..
        } = item
        {
            for d in declared {
                if !KNOWN_EFFECTS.contains(&d.as_str()) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown effect '{d}' in fn '{name}' (known: LLM, Tool, IO, Decision, Computer)"),
                    });
                }
            }
            let want = &inferred[name];
            let missing: Vec<&String> = want
                .iter()
                .filter(|e| !declared.iter().any(|d| d == *e))
                .collect();
            if missing.is_empty() {
                continue;
            }
            let list = missing
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if declared.is_empty() {
                errs.push(CheckError {
                    span: None,
                    code: "E0301",
                    msg: format!(
                        "fn '{name}' uses {list} but has no `uses` clause — add `uses {list}` to its signature"
                    ),
                });
            } else {
                errs.push(CheckError {
                    span: None,
                    code: "E0302",
                    msg: format!(
                        "fn '{name}' declares `uses {}` but its body also uses {list} — annotation too narrow",
                        declared.join(", ")
                    ),
                });
            }
        }
    }
    errs
}

/// builtin primitive type names usable as a map key in `{string: T}`-style
/// map types (codegen lowers these to `additionalProperties`)
fn is_builtin_type_name(n: &str) -> bool {
    matches!(n, "string" | "int" | "float" | "bool" | "none")
}

// ── tests ────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    #[test]
    fn unknown_names_get_did_you_mean_suggestions() {
        let errs = check_src(
            "fn handler(s: string) -> int { len(s) }\nfn main() -> int { handlr(\"x\") }",
        );
        assert!(
            errs.iter()
                .any(|e| e.msg.contains("did you mean 'handler'")),
            "got {errs:?}"
        );
        let errs =
            check_src("type Greet = { message: string }\nfn f(g: Greet) -> string { g.mesage }");
        assert!(
            errs.iter()
                .any(|e| e.msg.contains("did you mean 'message'")),
            "got {errs:?}"
        );
        // far-off names get no suggestion (no noise)
        let errs = check_src("fn main() -> int { zzzzqqq() }");
        assert!(
            errs.iter()
                .any(|e| e.msg.contains("never declared") && !e.msg.contains("did you mean")),
            "got {errs:?}"
        );
    }

    fn check_src(src: &str) -> Vec<CheckError> {
        check(&parse(lex(src).unwrap()).unwrap())
    }

    #[test]
    fn for_all_in_test_block_checks_clean() {
        let errs = check_src(
            r#"
fn helper(s: string) -> int { len(s) }

test "props" {
    for_all n in gen.int(0, 10) {
        assert n <= 10
    }
    for_all s in gen.str(4) {
        assert helper(s) >= 0
    }
    for_all p in gen.injection() {
        assert len(p) >= 0
    }
}"#,
        );
        assert_eq!(errs, vec![], "{errs:?}");
    }

    #[test]
    fn for_all_outside_test_block_is_e0801() {
        let errs = check_src(
            "fn f() -> int { for_all n in gen.int(0, 3) { assert n >= 0 }
    1 }",
        );
        assert!(errs.iter().any(|e| e.code == "E0801"), "{errs:?}");
    }

    #[test]
    fn unknown_generator_is_e0802() {
        let errs = check_src(r#"test "x" { for_all n in gen.range(0, 3) { assert n >= 0 } }"#);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0802" && e.msg.contains("gen.range")),
            "{errs:?}"
        );
    }

    #[test]
    fn llm_and_tool_calls_in_properties_are_e0804() {
        let llm_src = r#"
fn ask(q: string) -> string uses LLM {
    llm"""answer {q}""" with { schema: string, model: "m", budget: 0.01 USD }
}

test "x" { for_all p in gen.injection() { let r = ask(p)
    assert len(r) >= 0 } }"#;
        let errs = check_src(llm_src);
        assert!(errs.iter().any(|e| e.code == "E0804"), "{errs:?}");

        let route_src = r#"
test "r" { for_all p in gen.injection() {
    let r = route {
        a: llm"""go""" with { model: "m" } when true,
        b: "ok" otherwise,
    }
    assert len(r) >= 0
} }"#;
        let errs = check_src(route_src);
        assert!(errs.iter().any(|e| e.code == "E0804"), "{errs:?}");
    }

    #[test]
    fn gen_type_mismatch_is_e0802() {
        let errs = check_src(r#"test "x" { for_all s in gen.str(4) { assert s == 1 } }"#);
        // s is a string; `s == 1` must surface an assignability error
        assert!(errs.iter().any(|e| e.code == "E0201"), "{errs:?}");
    }

    #[test]
    fn decide_checks_clean_and_types_the_answer() {
        let errs = check_src(
            r#"
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept: "which team?" choose [billing, technical],
        risk: "churn?" yes/no,
        urgency: "how urgent?" score [low, soon, critical]
    } on t with { model: "laya:multilingual", deadline: 50 }
    d.dept.winner
}"#,
        );
        assert_eq!(errs, vec![], "{errs:?}");
    }

    #[test]
    fn decide_infers_the_decision_effect() {
        let errs = check_src(
            "fn f(t: string) -> string { let d = decide { x: \"pick?\" choose [a, b] } on t\n    d.x.winner }",
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Decision")),
            "{errs:?}"
        );
    }

    // ── v1.5 computer-use ──────────────────────────────────────────

    #[test]
    fn computer_calls_check_clean_and_are_typed() {
        let src = r#"
fn run(app: string) -> bool uses Computer {
    let obs = computer.observe(app, allow = ["Notes"], screenshot = true)
    let r = computer.click(3, allow = ["Notes"])
    len(obs.elements) >= 0 and r.ok
}"#;
        assert_eq!(check_src(src), vec![], "{errs:?}", errs = check_src(src));
    }

    #[test]
    fn computer_infers_the_computer_effect() {
        let errs = check_src("fn f() -> bool { let r = computer.click(1)\n    r.ok }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Computer")),
            "{errs:?}"
        );
    }

    #[test]
    fn computer_option_and_arity_errors() {
        // unknown option
        let errs = check_src(
            "fn f() -> bool uses Computer { let r = computer.click(1, volume = 3)\n    r.ok }",
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0901" && e.msg.contains("volume")),
            "{errs:?}"
        );
        // mistyped option
        let errs = check_src(
            "fn f() -> bool uses Computer { let r = computer.click(1, allow = 3)\n    r.ok }",
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0901" && e.msg.contains("'allow' must be")),
            "{errs:?}"
        );
        // arity
        let errs = check_src(
            "fn f() -> bool uses Computer { let o = computer.observe()\n    len(o.drift.added) >= 0 }",
        );
        assert!(errs.iter().any(|e| e.code == "E0902"), "{errs:?}");
    }

    #[test]
    fn computer_observation_fields_are_typed() {
        // `.changed` is bool on the Observation's drift record — using it in
        // arithmetic must be E0201, proving Observation resolves as a record
        let errs = check_src("fn f() -> int uses Computer { let o = computer.observe(\"Notes\")\n    o.drift.changed + 1 }");
        assert!(errs.iter().any(|e| e.code == "E0201"), "{errs:?}");
        // valid field reads stay clean
        let ok = check_src("fn f() -> bool uses Computer { let o = computer.observe(\"Notes\")\n    o.drift.changed or o.title == \"\" }");
        assert_eq!(ok, vec![]);
    }

    #[test]
    fn computer_in_properties_is_e0804() {
        let errs = check_src(
            r#"test "x" { for_all p in gen.injection() {
    let o = computer.observe("Notes")
    assert o.elements.len() >= 0 } }"#,
        );
        assert!(errs.iter().any(|e| e.code == "E0804"), "{errs:?}");
    }

    #[test]
    fn computer_perform_and_paste_check_clean() {
        let src = r#"
fn run(app: string) -> bool uses Computer {
    let obs = computer.observe(app, allow = [app])
    let p = computer.perform(2, "AXPress", allow = [app])
    let w = computer.paste("hello", allow = [app])
    obs.elements.len() >= 0 and p.action_sent and w.ok
}"#;
        assert_eq!(check_src(src), vec![], "{errs:?}", errs = check_src(src));
    }

    #[test]
    fn computer_dispatch_receipt_is_typed() {
        // action_sent is a bool — comparing it to an int must be E0201
        let errs = check_src(
            "fn f() -> bool uses Computer { let r = computer.click(1)
    r.action_sent == 3 }",
        );
        assert!(errs.iter().any(|e| e.code == "E0201"), "{errs:?}");
    }

    #[test]
    fn computer_is_a_reserved_name() {
        let errs = check_src("fn f() -> int { let computer = 3\n    computer }");
        assert!(errs.iter().any(|e| e.code == "E0103"), "{errs:?}");
    }

    #[test]
    fn decide_answer_field_types_are_enforced() {
        let errs = check_src(
            r#"
fn f(t: string) -> int uses Decision {
    let d = decide { dept: "team?" choose [a, b] } on t
    let n: int = d.dept.winner
    n
}"#,
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("annotated int")),
            "{errs:?}"
        );
    }

    #[test]
    fn decide_capability_limits() {
        // rubric out of range
        let errs = check_src(
            r#"fn f(t: string) -> float uses Decision { let d = decide { s: "rate?" score [a] } on t
    d.s.score }"#,
        );
        assert!(errs.iter().any(|e| e.code == "E0807"), "{errs:?}");
        // unknown option key
        let errs2 = check_src(
            r#"fn f(t: string) -> string uses Decision { let d = decide { x: "pick?" choose [a, b] } on t with { temperature: 0 }
    d.x.winner }"#,
        );
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0806" && e.msg.contains("temperature")),
            "{errs2:?}"
        );
    }

    #[test]
    fn route_arms_must_agree_on_type() {
        let errs = check_src(
            r#"
fn resolve(t: string) -> string uses IO { t }
fn f(c: bool) -> string uses IO {
    route { a: resolve("x") when c, b: 42 otherwise }
}"#,
        );
        assert!(
            errs.iter().any(|e| e.msg.contains("route arm 'b' yields")),
            "{errs:?}"
        );
    }
    #[test]
    fn research_agent_checks_clean() {
        let src = include_str!("../../../examples/research_agent.ndg");
        assert_eq!(check_src(src), vec![], "expected zero diagnostics");
    }

    #[test]
    fn statement_errors_carry_spans() {
        // the let on line 1 is annotated string but bound to an int — the
        // diagnostic must point at that statement's span (spanned AST §1)
        let errs = check_src("fn f() -> int { let x: string = 1\n    x }");
        let e = errs
            .iter()
            .find(|e| e.code == "E0201" && e.msg.contains("let 'x'"))
            .expect("got no let diagnostic");
        let sp = e.span.expect("diagnostic carries no span");
        assert_eq!(sp.start, 16, "span should start at the let keyword");
        assert!(sp.end > sp.start);
    }

    #[test]
    fn schema_must_match_return_type() {
        let errs = check_src(
            "type Finding = { claim: string }\ntype Report = { title: string }\nfn f() -> [Finding] uses LLM { llm\"\"\"x\"\"\" with { schema: Report } }",
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("declares ->")),
            "got {errs:?}"
        );
        assert_eq!(errs.len(), 1, "got {errs:?}");
    }

    #[test]
    fn unknown_interpolation_identifier() {
        let errs = check_src("fn f(q: string) -> string uses LLM { llm\"\"\"hi {qusetion}\"\"\" with { model: \"m\" } }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("qusetion")),
            "got {errs:?}"
        );
    }

    #[test]
    fn unknown_type_name() {
        let errs = check_src("fn f(x: Strnig) -> int { 1 }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("Strnig")),
            "got {errs:?}"
        );
    }

    #[test]
    fn malformed_range_refinement() {
        let errs = check_src("type S = float @range(1)");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0202" && e.msg.contains("@range")),
            "got {errs:?}"
        );
    }

    #[test]
    fn unknown_refinement() {
        let errs = check_src("type S = float @between(0, 1)");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0202" && e.msg.contains("@between")),
            "got {errs:?}"
        );
    }

    #[test]
    fn let_annotation_mismatch() {
        let errs = check_src("fn f() -> int { let x: int = \"nope\" }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("let 'x'")),
            "got {errs:?}"
        );
    }

    #[test]
    fn int_widens_to_float() {
        let errs = check_src("fn f() -> float { let x: float = 1\n x }");
        assert_eq!(errs, vec![]);
    }

    #[test]
    fn call_argument_type_and_arity() {
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f(n: int) -> [R] uses Tool { web(n) }";
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("argument to 'web'")),
            "got {errs:?}"
        );
        let errs2 = check_src(
            "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f() -> [R] uses Tool { web() }",
        );
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0201" && e.msg.contains("takes 1 argument")),
            "got {errs2:?}"
        );
    }

    #[test]
    fn cyclic_alias_is_an_error_not_a_hang() {
        let errs = check_src("type A = { b: B }\ntype B = { a: A }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0202" && e.msg.contains("cyclic")),
            "got {errs:?}"
        );
    }

    #[test]
    fn par_map_typing_flows_through_zip() {
        // mirrors the research agent's fan-out shape
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn a(q: string, h: [R]) -> string uses LLM { llm\"\"\"{q} {h}\"\"\" with { model: \"m\" } }\nfn run(qs: [string]) -> [string] uses LLM, Tool {\n    let hits = par map qs |q| -> web(q)\n    par map(qs zip hits) |(q, h)| -> a(q, h)\n}";
        assert_eq!(check_src(src), vec![]);
    }

    // ── effect inference (design §3.2, v1.4) ───────────────────────

    #[test]
    fn llm_call_without_uses_is_e0301() {
        let errs = check_src("fn f() -> string { llm\"\"\"x\"\"\" with { model: \"m\" } }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("LLM") && e.msg.contains("f")),
            "got {errs:?}"
        );
    }

    #[test]
    fn narrow_annotation_is_e0302() {
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f(q: string) -> [R] uses LLM { web(q) }";
        let errs = check_src(src);
        assert!(
            errs.iter().any(|e| e.code == "E0302"
                && e.msg.contains("Tool")
                && e.msg.contains("too narrow")),
            "got {errs:?}"
        );
    }

    #[test]
    fn effects_propagate_through_user_fn_calls() {
        let src = "fn a() -> string uses LLM { llm\"\"\"x\"\"\" with { model: \"m\" } }\nfn b() -> string { a() }";
        let errs = check_src(src);
        assert_eq!(errs.len(), 1, "got {errs:?}");
        assert!(
            errs[0].code == "E0301" && errs[0].msg.contains("'b'"),
            "got {errs:?}"
        );
    }

    #[test]
    fn replay_and_python_are_io() {
        let errs = check_src("fn f(p: string) -> string { replay(p) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("IO")),
            "got {errs:?}"
        );
        let errs2 = check_src("fn f(p: string) -> string uses IO { replay(p) }");
        assert_eq!(errs2, vec![]);
    }

    #[test]
    fn tool_call_declared_is_clean() {
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f(q: string) -> [R] uses Tool { web(q) }";
        assert_eq!(check_src(src), vec![]);
    }

    #[test]
    fn unknown_effect_name_is_e0101() {
        let errs = check_src("fn f() -> int uses Magic { 1 }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("Magic")),
            "got {errs:?}"
        );
    }

    #[test]
    fn test_blocks_are_exempt_from_effect_rules() {
        let errs = check_src("test \"t\" { let x = replay(\"t.jsonl\")\nassert true }");
        assert_eq!(errs, vec![]);
    }

    #[test]
    fn non_usd_budget_is_e0501() {
        let errs =
            check_src("fn f() -> string uses LLM { llm\"\"\"x\"\"\" with { budget: 5 EUR } }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0501" && e.msg.contains("EUR")),
            "got {errs:?}"
        );
    }

    #[test]
    fn usd_budget_is_clean() {
        let errs =
            check_src("fn f() -> string uses LLM { llm\"\"\"x\"\"\" with { budget: 0.02 USD } }");
        assert_eq!(errs, vec![]);
    }

    #[test]
    fn agent_state_checks_and_e0701_outside_agent() {
        // clean: writes hit declared fields, `=` type matches
        let src = "agent A {\n    state {\n        notes: [string] = [],\n        round: int = 0,\n    }\n    fn step(q: string) -> int uses LLM {\n        let r = llm\"\"\"next: {q}\"\"\" with { model: \"m\" }\n        state.notes += [r]\n        state.round = state.round + 1\n        state.round\n    }\n}";
        assert_eq!(check_src(src), vec![], "expected zero diagnostics");
        // E0701: state write in a plain fn
        let errs = check_src("fn f() -> int { state.round = 1\n    0 }");
        assert!(errs.iter().any(|e| e.code == "E0701"), "got {errs:?}");
        // E0701: undeclared state field
        let errs = check_src("agent B {\n    state {\n        round: int = 0,\n    }\n    fn step() -> int { state.missing = 1\n        0\n    }\n}");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0701" && e.msg.contains("no field 'missing'")),
            "got {errs:?}"
        );
        // E0201: `=` write with a mismatched type
        let errs = check_src("agent C {\n    state {\n        round: int = 0,\n    }\n    fn step() -> int { state.round = \"oops\"\n        0\n    }\n}");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }

    #[test]
    fn merge_reducer_checks_operands() {
        // clean: list append-dedup and record union
        assert_eq!(check_src("fn f(x: [int]) -> [int] { x | merge x }"), vec![]);
        assert_eq!(
            check_src("type R = { a: int }\nfn f(x: R) -> R { x | merge x }"),
            vec![]
        );
        // E0201: mismatched / scalar operands
        let errs = check_src("fn f(x: int) -> int { x | merge x }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("merge expects")),
            "got {errs:?}"
        );
        let errs = check_src("fn f(x: [int], s: string) -> [int] { x | merge s }");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }

    #[test]
    fn route_block_requires_otherwise_and_bool_conditions() {
        // clean
        assert_eq!(check_src("fn f(b: bool) -> string uses LLM { llm\"\"\"x\"\"\" with { model: route{ cheap: \"m1\" when b, strong: \"m2\" otherwise } } }"), vec![]);
        // E0702: no otherwise arm
        let errs = check_src("fn f(b: bool) -> string uses LLM { llm\"\"\"x\"\"\" with { model: route{ cheap: \"m1\" when b } } }");
        assert!(errs.iter().any(|e| e.code == "E0702"), "got {errs:?}");
        // E0201: non-bool condition
        let errs = check_src("fn f(n: int) -> string uses LLM { llm\"\"\"x\"\"\" with { model: route{ cheap: \"m1\" when n, strong: \"m2\" otherwise } } }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("non-bool when")),
            "got {errs:?}"
        );
    }

    // ── v1.3 regression: unknown identifiers, builtin arity, div typing ──

    #[test]
    fn unknown_variable_identifier_is_e0101() {
        let errs = check_src("fn f() -> int { qusetion }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("qusetion")),
            "got {errs:?}"
        );
        // declared names stay clean
        assert_eq!(check_src("fn f(q: int) -> int { q }"), vec![]);
    }

    #[test]
    fn call_to_undeclared_name_is_e0101() {
        let errs = check_src("fn f() -> int { nosuchfn(1) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("nosuchfn")),
            "got {errs:?}"
        );
    }

    #[test]
    fn unknown_record_field_is_e0101() {
        let errs = check_src("type R = { t: string }\nfn f(r: R) -> string { r.ttle }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0101" && e.msg.contains("ttle")),
            "got {errs:?}"
        );
        assert_eq!(
            check_src("type R = { t: string }\nfn f(r: R) -> string { r.t }"),
            vec![]
        );
    }

    #[test]
    fn builtin_arity_is_checked() {
        let errs = check_src("fn f() -> int { len() }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("len")),
            "got {errs:?}"
        );
        let errs = check_src("fn f(a: [int]) -> int { zip(a) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("zip")),
            "got {errs:?}"
        );
        assert_eq!(check_src("fn f(a: [int]) -> int { len(a) }"), vec![]);
    }

    #[test]
    fn int_division_types_as_float() {
        let errs = check_src("fn f() -> float { 7 / 2 }");
        assert_eq!(errs, vec![], "got {errs:?}");
        // and an int-annotated binding of a division now mismatches
        let errs = check_src("fn f() -> int { let x: int = 7 / 2\n    x }");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
        // the other int operators stay int
        assert_eq!(check_src("fn f() -> int { 7 % 2 }"), vec![]);
    }

    #[test]
    fn duplicate_definitions_are_e0102() {
        let errs = check_src("type A = int\ntype A = string");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0102" && e.msg.contains("duplicate type alias 'A'")),
            "got {errs:?}"
        );
        let errs = check_src("fn f() -> int { 1 }\nfn f() -> int { 2 }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0102" && e.msg.contains("duplicate fn 'f'")),
            "got {errs:?}"
        );
        // agent fns share the namespace: two agents, same fn name → error
        let errs =
            check_src("agent A { fn step() -> int { 1 } }\nagent B { fn step() -> int { 2 } }");
        assert!(errs.iter().any(|e| e.code == "E0102"), "got {errs:?}");
    }

    // ── v1.4 regression: kwargs, bool rules, list unification ──────

    #[test]
    fn kwargs_fill_params_by_name() {
        // kwargs alone satisfy the signature (used to be a false "takes 1" error)
        let src = "fn g(a: int, b: string) -> int { a }\nfn f() -> int { g(1, b = \"x\") }";
        assert_eq!(check_src(src), vec![]);
        let src = "fn g(a: int) -> int { a }\nfn f() -> int { g(a = 3) }";
        assert_eq!(check_src(src), vec![]);
        // unknown kwarg name
        let errs = check_src("fn g(a: int) -> int { a }\nfn f() -> int { g(b = 3) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("no parameter 'b'")),
            "got {errs:?}"
        );
        // positional + kwarg for the same param
        let errs = check_src("fn g(a: int) -> int { a }\nfn f() -> int { g(1, a = 3) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("twice")),
            "got {errs:?}"
        );
        // mistyped kwarg
        let errs = check_src("fn g(a: int) -> int { a }\nfn f() -> int { g(a = \"x\") }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("argument 'a'")),
            "got {errs:?}"
        );
        // still missing after kwargs
        let errs = check_src("fn g(a: int, b: int) -> int { a }\nfn f() -> int { g(1) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("missing: b")),
            "got {errs:?}"
        );
    }

    #[test]
    fn assert_and_boolean_connectives_need_bools() {
        let errs = check_src("test \"t\" { assert \"nope\" }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("assert expects a bool")),
            "got {errs:?}"
        );
        let errs = check_src("fn f(x: int) -> bool { x and true }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("'and' expects bool")),
            "got {errs:?}"
        );
        assert_eq!(
            check_src("fn f(x: bool) -> bool { x and true or false }"),
            vec![]
        );
        assert_eq!(check_src("test \"t\" { assert 1 < 2 }"), vec![]);
    }

    #[test]
    fn list_literals_unify_element_types() {
        // numeric widening is fine
        assert_eq!(check_src("fn f() -> [float] { [1, 2.5] }"), vec![]);
        // incompatible mixes are E0201 (used to be typed by the first element)
        let errs = check_src("fn f() -> [int] { [1, \"a\"] }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("list element")),
            "got {errs:?}"
        );
    }

    #[test]
    fn test_block_let_annotations_are_checked() {
        let errs = check_src("test \"t\" { let x: int = \"nope\"\nassert true }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("let 'x'")),
            "got {errs:?}"
        );
    }

    // ── v1.5 regression: comparison operand rules ──────────────────

    #[test]
    fn order_comparisons_need_comparable_operands() {
        // numeric × numeric (incl. money and refined numerics) is fine
        assert_eq!(
            check_src("fn f(a: int, b: float) -> bool { a < b }"),
            vec![]
        );
        assert_eq!(
            check_src("fn f() -> bool { 0.001 USD <= 0.05 USD }"),
            vec![]
        );
        assert_eq!(
            check_src("fn f(a: int @range(1, 9)) -> bool { a < 5 }"),
            vec![]
        );
        // string × string is fine
        assert_eq!(check_src("fn f(a: string) -> bool { a < \"z\" }"), vec![]);
        // mixed nonsense is E0201 (used to type as bool)
        let errs = check_src("fn f() -> bool { 1 < \"a\" }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("order-compare")),
            "got {errs:?}"
        );
        let errs = check_src("fn f(a: bool) -> bool { a < true }");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }

    #[test]
    fn equality_needs_compatible_operands() {
        assert_eq!(check_src("fn f(a: int) -> bool { a == 2.5 }"), vec![]); // int/float widen
        assert_eq!(check_src("fn f(a: string) -> bool { a != \"x\" }"), vec![]);
        let errs = check_src("fn f() -> bool { 1 == \"a\" }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("for equality")),
            "got {errs:?}"
        );
        // records only equal matching records
        let errs = check_src("type R = { t: string }\nfn f(r: R) -> bool { r == 1 }");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }
}
