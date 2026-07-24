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
    fns: HashMap<String, (Vec<TypeExpr>, TypeExpr)>,
    tools: HashMap<String, (Vec<TypeExpr>, TypeExpr)>,
}

// ── type resolution ─────────────────────────────────────────────────

fn resolve(t: &TypeExpr, g: &Globals, visiting: &mut Vec<String>, errs: &mut Vec<CheckError>) -> Ty {
    match t {
        TypeExpr::Named(name) => match name.as_str() {
            "int" => Ty::Int,
            "float" => Ty::Float,
            "bool" => Ty::Bool,
            "string" => Ty::Str,
            "none" | "()" => Ty::None_,
            "bytes" | "timestamp" => Ty::Unknown, // post-MVP core types
            _ => {
                if visiting.iter().any(|v| v == name) {
                    errs.push(CheckError {
                        code: "E0202",
                        msg: format!("cyclic type alias '{}'", visiting.join(" → ") + " → " + name),
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
                        errs.push(CheckError {
                            code: "E0101",
                            msg: format!("unknown type '{name}'"),
                        });
                        Ty::Unknown
                    }
                }
            }
        },
        TypeExpr::List(inner) => Ty::List(Box::new(resolve(inner, g, visiting, errs))),
        TypeExpr::Record(fields) => Ty::Record(
            fields.iter().map(|(k, ft)| (k.clone(), resolve(ft, g, visiting, errs))).collect(),
        ),
        TypeExpr::Refine(base, name, args) => {
            validate_refinement(name, args, errs);
            Ty::Refine(Box::new(resolve(base, g, visiting, errs)), name.clone())
        }
    }
}

fn validate_refinement(name: &str, args: &[Expr], errs: &mut Vec<CheckError>) {
    let numeric = |e: &Expr| matches!(e, Expr::Int(_) | Expr::Float(_));
    match name {
        "range" => {
            if !(args.len() == 2 && args.iter().all(numeric)) {
                errs.push(CheckError {
                    code: "E0202",
                    msg: format!("@range expects 2 numeric bounds, e.g. @range(0, 1) — got {args:?}"),
                });
            }
        }
        "format" => {
            let ok = args.len() == 1
                && match &args[0] {
                    Expr::Ident(f) | Expr::Str(f) => matches!(f.as_str(), "url" | "email"),
                    _ => false,
                };
            if !ok {
                errs.push(CheckError {
                    code: "E0202",
                    msg: format!("@format expects one of (url, email) — got {args:?}"),
                });
            }
        }
        _ => errs.push(CheckError {
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
                    a.iter().find(|(ak, _)| ak == k).map(|(_, at)| assignable(at, bt)).unwrap_or(false)
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

const KNOWN_EFFECTS: [&str; 3] = ["LLM", "Tool", "IO"];

/// Collect the direct (non-transitive) effects of an expression, plus the
/// names of user fns it calls (call-graph edges for the fixpoint).
fn direct_effects(
    e: &Expr,
    g: &Globals,
    effects: &mut BTreeSet<String>,
    calls: &mut BTreeSet<String>,
) {
    match e {
        Expr::LlmCall { options, .. } => {
            effects.insert("LLM".into());
            for (_, v) in options {
                direct_effects(v, g, effects, calls);
            }
        }
        Expr::Call { func, args, kwargs } => {
            if let Expr::Ident(name) = func.as_ref() {
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
        Expr::ListLit(xs) | Expr::ParAll(xs) | Expr::ParRace(xs) => {
            for x in xs {
                direct_effects(x, g, effects, calls);
            }
        }
        Expr::Field { obj, .. } => direct_effects(obj, g, effects, calls),
        Expr::Binary { l, r, .. } | Expr::Merge { l, r } => {
            direct_effects(l, g, effects, calls);
            direct_effects(r, g, effects, calls);
        }
        Expr::Route { arms } => {
            for (_, _, cond) in arms {
                if let Some(c) = cond {
                    direct_effects(c, g, effects, calls);
                }
            }
        }
        Expr::Unary { x, .. } => direct_effects(x, g, effects, calls),
        Expr::ParMap { coll, kwargs, body, .. } => {
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
        match st {
            Stmt::Let { value, .. } => direct_effects(value, g, effects, calls),
            Stmt::StateWrite { value, .. } => direct_effects(value, g, effects, calls),
            Stmt::Assert(e) | Stmt::ExprStmt(e) => direct_effects(e, g, effects, calls),
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

/// Check one fn body. `agent_ctx` is `Some((agent_name, state_fields))` when
/// the fn lives inside an `agent` block: `state` becomes a record local and
/// state writes are validated against the declared fields. Outside an agent,
/// a state write is E0701 (design §7).
fn check_fn_body(
    name: &str,
    params: &[Param],
    ret: &TypeExpr,
    body: &[Stmt],
    agent_ctx: Option<(&str, &[(String, TypeExpr, Expr)])>,
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
        match st {
            Stmt::Let { name: n, ty, value, .. } => {
                let vt = check_expr(value, &locals, g, errs);
                let bound = match ty {
                    Some(ann) => {
                        let at = resolve(ann, g, &mut Vec::new(), errs);
                        if !assignable(&vt, &at) {
                            errs.push(CheckError {
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
            Stmt::StateWrite { field, aug, value } => {
                let vt = check_expr(value, &locals, g, errs);
                match agent_ctx {
                    None => errs.push(CheckError {
                        code: "E0701",
                        msg: format!("state write 'state.{field}' outside an agent block — state exists only inside `agent` (design §7)"),
                    }),
                    Some((_, fields)) => {
                        match fields.iter().find(|(f, _, _)| f == field) {
                            None => errs.push(CheckError {
                                code: "E0701",
                                msg: format!("agent state has no field '{field}' — declare it in the state block"),
                            }),
                            Some((_, fty, _)) => {
                                // `=`: the value must fit the declared type.
                                // `+=`: list-concat / numeric add — the runtime
                                // checkpoint stores whatever the write yields.
                                if !*aug {
                                    let want = resolve(fty, g, &mut Vec::new(), errs);
                                    if !assignable(&vt, &want) {
                                        errs.push(CheckError {
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
            Stmt::Assert(e) => {
                check_expr(e, &locals, g, errs);
            }
            Stmt::ExprStmt(e) => {
                last_ty = check_expr(e, &locals, g, errs);
            }
        }
    }
    let want = resolve(ret, g, &mut Vec::new(), errs);
    if !assignable(&last_ty, &want) {
        errs.push(CheckError {
            code: "E0201",
            msg: format!("fn '{name}' declares -> {want} but its body yields {last_ty}"),
        });
    }
}

// ── expression checking + inference ─────────────────────────────────

fn schema_expr_ty(e: &Expr, g: &Globals, errs: &mut Vec<CheckError>) -> Ty {
    let as_type = match e {
        Expr::Ident(n) => Some(TypeExpr::Named(n.clone())),
        Expr::ListLit(xs) if xs.len() == 1 => match &xs[0] {
            Expr::Ident(n) => Some(TypeExpr::List(Box::new(TypeExpr::Named(n.clone())))),
            _ => None,
        },
        _ => None,
    };
    match as_type {
        Some(t) => resolve(&t, g, &mut Vec::new(), errs),
        None => {
            errs.push(CheckError {
                code: "E0202",
                msg: format!("schema must be a type (e.g. schema: Report or schema: [Finding]) — got {e:?}"),
            });
            Ty::Unknown
        }
    }
}

fn check_expr(e: &Expr, locals: &HashMap<String, Ty>, g: &Globals, errs: &mut Vec<CheckError>) -> Ty {
    match e {
        Expr::Int(_) => Ty::Int,
        Expr::Float(_) => Ty::Float,
        Expr::Money(_, unit) => {
            // v0.1 speaks USD only (design §4.3)
            if unit != "USD" {
                errs.push(CheckError {
                    code: "E0501",
                    msg: format!("unknown budget unit '{unit}' (v0.1 supports USD only)"),
                });
            }
            Ty::Float
        }
        Expr::Str(_) | Expr::Prompt { .. } => Ty::Str,
        Expr::Bool(_) => Ty::Bool,
        Expr::None => Ty::None_,
        Expr::Ident(n) => locals.get(n).cloned().unwrap_or(Ty::Unknown),
        Expr::ListLit(xs) => {
            let mut elem = Ty::Unknown;
            for (i, x) in xs.iter().enumerate() {
                let t = check_expr(x, locals, g, errs);
                if i == 0 {
                    elem = t;
                }
            }
            Ty::List(Box::new(elem))
        }
        Expr::LlmCall { prompt, options, .. } => {
            if let Expr::Prompt { interpolations, .. } = prompt.as_ref() {
                for name in interpolations {
                    let root = name.split('.').next().unwrap_or(name);
                    if !locals.contains_key(root) {
                        errs.push(CheckError {
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
        Expr::Call { func, args, kwargs } => {
            let arg_tys: Vec<Ty> = args.iter().map(|a| check_expr(a, locals, g, errs)).collect();
            for (_, v) in kwargs {
                check_expr(v, locals, g, errs);
            }
            if let Expr::Ident(name) = func.as_ref() {
                match name.as_str() {
                    "len" => return Ty::Int,
                    "zip" => {
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
                    if params.len() != args.len() {
                        errs.push(CheckError {
                            code: "E0201",
                            msg: format!("'{name}' takes {} argument(s), got {}", params.len(), args.len()),
                        });
                    } else {
                        for (at, pt) in arg_tys.iter().zip(params.iter()) {
                            let want = resolve(pt, g, &mut Vec::new(), errs);
                            if !assignable(at, &want) {
                                errs.push(CheckError {
                                    code: "E0201",
                                    msg: format!("argument to '{name}' must be {want}, got {at}"),
                                });
                            }
                        }
                    }
                    return resolve(ret, g, &mut Vec::new(), errs);
                }
            }
            Ty::Unknown
        }
        Expr::Field { obj, name } => match check_expr(obj, locals, g, errs) {
            Ty::Record(fs) => {
                fs.iter().find(|(k, _)| k == name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
            }
            _ => Ty::Unknown,
        },
        Expr::Binary { op, l, r } => {
            let lt = check_expr(l, locals, g, errs);
            let rt_ = check_expr(r, locals, g, errs);
            match op {
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq
                | BinOp::And | BinOp::Or => Ty::Bool,
                _ => match (&lt, &rt_) {
                    (Ty::Float, _) | (_, Ty::Float) => Ty::Float,
                    (Ty::Int, Ty::Int) => Ty::Int,
                    _ => Ty::Unknown,
                },
            }
        }
        Expr::Unary { op, x } => {
            let t = check_expr(x, locals, g, errs);
            match op {
                BinOp::Not => Ty::Bool,
                _ => t,
            }
        }
        Expr::ParMap { coll, kwargs, params, body } => {
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
                            let t = fs.iter().find(|(k, _)| k == key).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
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
        Expr::ParAll(xs) => {
            let t = xs.first().map(|x| check_expr(x, locals, g, errs)).unwrap_or(Ty::Unknown);
            for x in &xs[1.min(xs.len())..] {
                check_expr(x, locals, g, errs);
            }
            Ty::List(Box::new(t))
        }
        Expr::ParRace(xs) => {
            let t = xs.first().map(|x| check_expr(x, locals, g, errs)).unwrap_or(Ty::Unknown);
            for x in &xs[1.min(xs.len())..] {
                check_expr(x, locals, g, errs);
            }
            t
        }
        // design §7 reducer join: two records (dict union) or two lists
        // (append-dedup); anything else is a type error
        Expr::Merge { l, r } => {
            let lt = check_expr(l, locals, g, errs);
            let rt_ = check_expr(r, locals, g, errs);
            match (&lt, &rt_) {
                (Ty::Record(_), Ty::Record(_)) => lt,
                (Ty::List(a), Ty::List(b)) => {
                    if !assignable(b, a) {
                        errs.push(CheckError {
                            code: "E0201",
                            msg: format!("merge list element mismatch: {lt} vs {rt_}"),
                        });
                    }
                    lt
                }
                (Ty::Unknown, t) | (t, Ty::Unknown) => t.clone(),
                _ => {
                    errs.push(CheckError {
                        code: "E0201",
                        msg: format!("merge expects two records or two lists, got {lt} | merge {rt_}"),
                    });
                    Ty::Unknown
                }
            }
        }
        // design §4.4: route arms — every `when` condition must be bool and
        // the block needs an `otherwise` fallback (E0702)
        Expr::Route { arms } => {
            let mut has_otherwise = false;
            for (label, _, cond) in arms {
                match cond {
                    Some(c) => {
                        let ct = check_expr(c, locals, g, errs);
                        if !assignable(&ct, &Ty::Bool) {
                            errs.push(CheckError {
                                code: "E0201",
                                msg: format!("route arm '{label}' has a non-bool when condition ({ct})"),
                            });
                        }
                    }
                    None => has_otherwise = true,
                }
            }
            if !has_otherwise {
                errs.push(CheckError {
                    code: "E0702",
                    msg: "route block needs an `otherwise` arm — no model is chosen when every `when` is false (design §4.4)".into(),
                });
            }
            Ty::Str
        }
    }
}

// ── entry point ─────────────────────────────────────────────────────

/// Check a whole program. Returns every diagnostic found (deterministic order).
pub fn check(items: &[Item]) -> Vec<CheckError> {
    let mut g = Globals::default();
    for item in items {
        match item {
            Item::TypeAlias { name, ty } => {
                g.aliases.insert(name.clone(), ty.clone());
            }
            Item::Fn { name, params, ret, .. } => {
                g.fns.insert(name.clone(), (params.iter().map(|p| p.ty.clone()).collect(), ret.clone()));
            }
            Item::Tool { name, params, ret, .. } => {
                g.tools.insert(name.clone(), (params.iter().map(|p| p.ty.clone()).collect(), ret.clone()));
            }
            Item::Agent { fns, .. } => {
                for f in fns {
                    if let Item::Fn { name, params, ret, .. } = f {
                        g.fns.insert(name.clone(), (params.iter().map(|p| p.ty.clone()).collect(), ret.clone()));
                    }
                }
            }
            Item::Test { .. } => {}
        }
    }

    let mut errs = Vec::new();

    // resolve every alias body once, so unused-but-broken types still error
    let names: Vec<String> = g.aliases.keys().cloned().collect();
    for name in names {
        let body = g.aliases[&name].clone();
        resolve(&body, &g, &mut vec![name], &mut errs);
    }

    for item in items {
        match item {
            Item::Fn { name, params, ret, body, .. } => {
                check_fn_body(name, params, ret, body, None, &g, &mut errs);
            }
            Item::Agent { name: agent, state, fns } => {
                // resolve state field types once; unknown types already error
                for (_, ty, _) in state {
                    resolve(ty, &g, &mut Vec::new(), &mut errs);
                }
                for f in fns {
                    if let Item::Fn { name, params, ret, body, .. } = f {
                        check_fn_body(name, params, ret, body, Some((agent, state)), &g, &mut errs);
                    }
                }
            }
            Item::Tool { params, ret, fields, .. } => {
                let mut locals: HashMap<String, Ty> = HashMap::new();
                for p in params {
                    locals.insert(p.name.clone(), resolve(&p.ty, &g, &mut Vec::new(), &mut errs));
                }
                resolve(ret, &g, &mut Vec::new(), &mut errs);
                for (_, v) in fields {
                    check_expr(v, &locals, &g, &mut errs);
                }
            }
            Item::Test { body, .. } => {
                let mut locals: HashMap<String, Ty> = HashMap::new();
                for st in body {
                    match st {
                        Stmt::Let { name, value, .. } => {
                            let vt = check_expr(value, &locals, &g, &mut errs);
                            locals.insert(name.clone(), vt);
                        }
                        Stmt::StateWrite { field, .. } => {
                            errs.push(CheckError {
                                code: "E0701",
                                msg: format!("state write 'state.{field}' outside an agent block — state exists only inside `agent` (design §7)"),
                            });
                        }
                        Stmt::Assert(e) | Stmt::ExprStmt(e) => {
                            check_expr(e, &locals, &g, &mut errs);
                        }
                    }
                }
            }
            Item::TypeAlias { .. } => {}
        }
    }

    // ── effect inference + signature verification (design §3.2) ────
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

    for item in fn_items(items) {
        if let Item::Fn { name, effects: declared, .. } = item {
            for d in declared {
                if !KNOWN_EFFECTS.contains(&d.as_str()) {
                    errs.push(CheckError {
                        code: "E0101",
                        msg: format!("unknown effect '{d}' in fn '{name}' (known: LLM, Tool, IO)"),
                    });
                }
            }
            let want = &inferred[name];
            let missing: Vec<&String> =
                want.iter().filter(|e| !declared.iter().any(|d| d == *e)).collect();
            if missing.is_empty() {
                continue;
            }
            let list = missing.iter().map(|e| e.as_str()).collect::<Vec<_>>().join(", ");
            if declared.is_empty() {
                errs.push(CheckError {
                    code: "E0301",
                    msg: format!(
                        "fn '{name}' uses {list} but has no `uses` clause — add `uses {list}` to its signature"
                    ),
                });
            } else {
                errs.push(CheckError {
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

// ── tests ────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn check_src(src: &str) -> Vec<CheckError> {
        check(&parse(lex(src).unwrap()).unwrap())
    }

    #[test]
    fn research_agent_checks_clean() {
        let src = include_str!("../../../examples/research_agent.ndg");
        assert_eq!(check_src(src), vec![], "expected zero diagnostics");
    }

    #[test]
    fn schema_must_match_return_type() {
        let errs = check_src(
            "type Finding = { claim: string }\ntype Report = { title: string }\nfn f() -> [Finding] uses LLM { llm\"\"\"x\"\"\" with { schema: Report } }",
        );
        assert!(errs.iter().any(|e| e.code == "E0201" && e.msg.contains("declares ->")), "got {errs:?}");
        assert_eq!(errs.len(), 1, "got {errs:?}");
    }

    #[test]
    fn unknown_interpolation_identifier() {
        let errs = check_src("fn f(q: string) -> string uses LLM { llm\"\"\"hi {qusetion}\"\"\" with { model: \"m\" } }");
        assert!(errs.iter().any(|e| e.code == "E0101" && e.msg.contains("qusetion")), "got {errs:?}");
    }

    #[test]
    fn unknown_type_name() {
        let errs = check_src("fn f(x: Strnig) -> int { 1 }");
        assert!(errs.iter().any(|e| e.code == "E0101" && e.msg.contains("Strnig")), "got {errs:?}");
    }

    #[test]
    fn malformed_range_refinement() {
        let errs = check_src("type S = float @range(1)");
        assert!(errs.iter().any(|e| e.code == "E0202" && e.msg.contains("@range")), "got {errs:?}");
    }

    #[test]
    fn unknown_refinement() {
        let errs = check_src("type S = float @between(0, 1)");
        assert!(errs.iter().any(|e| e.code == "E0202" && e.msg.contains("@between")), "got {errs:?}");
    }

    #[test]
    fn let_annotation_mismatch() {
        let errs = check_src("fn f() -> int { let x: int = \"nope\" }");
        assert!(errs.iter().any(|e| e.code == "E0201" && e.msg.contains("let 'x'")), "got {errs:?}");
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
        assert!(errs.iter().any(|e| e.code == "E0201" && e.msg.contains("argument to 'web'")), "got {errs:?}");
        let errs2 = check_src(
            "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f() -> [R] uses Tool { web() }",
        );
        assert!(errs2.iter().any(|e| e.code == "E0201" && e.msg.contains("takes 1 argument")), "got {errs2:?}");
    }

    #[test]
    fn cyclic_alias_is_an_error_not_a_hang() {
        let errs = check_src("type A = { b: B }\ntype B = { a: A }");
        assert!(errs.iter().any(|e| e.code == "E0202" && e.msg.contains("cyclic")), "got {errs:?}");
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
        assert!(errs.iter().any(|e| e.code == "E0301" && e.msg.contains("LLM") && e.msg.contains("f")), "got {errs:?}");
    }

    #[test]
    fn narrow_annotation_is_e0302() {
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f(q: string) -> [R] uses LLM { web(q) }";
        let errs = check_src(src);
        assert!(errs.iter().any(|e| e.code == "E0302" && e.msg.contains("Tool") && e.msg.contains("too narrow")), "got {errs:?}");
    }

    #[test]
    fn effects_propagate_through_user_fn_calls() {
        let src = "fn a() -> string uses LLM { llm\"\"\"x\"\"\" with { model: \"m\" } }\nfn b() -> string { a() }";
        let errs = check_src(src);
        assert_eq!(errs.len(), 1, "got {errs:?}");
        assert!(errs[0].code == "E0301" && errs[0].msg.contains("'b'"), "got {errs:?}");
    }

    #[test]
    fn replay_and_python_are_io() {
        let errs = check_src("fn f(p: string) -> string { replay(p) }");
        assert!(errs.iter().any(|e| e.code == "E0301" && e.msg.contains("IO")), "got {errs:?}");
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
        assert!(errs.iter().any(|e| e.code == "E0101" && e.msg.contains("Magic")), "got {errs:?}");
    }

    #[test]
    fn test_blocks_are_exempt_from_effect_rules() {
        let errs = check_src("test \"t\" { let x = replay(\"t.jsonl\")\nassert true }");
        assert_eq!(errs, vec![]);
    }

    #[test]
    fn non_usd_budget_is_e0501() {
        let errs = check_src("fn f() -> string uses LLM { llm\"\"\"x\"\"\" with { budget: 5 EUR } }");
        assert!(errs.iter().any(|e| e.code == "E0501" && e.msg.contains("EUR")), "got {errs:?}");
    }

    #[test]
    fn usd_budget_is_clean() {
        let errs = check_src("fn f() -> string uses LLM { llm\"\"\"x\"\"\" with { budget: 0.02 USD } }");
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
        assert!(errs.iter().any(|e| e.code == "E0701" && e.msg.contains("no field 'missing'")), "got {errs:?}");
        // E0201: `=` write with a mismatched type
        let errs = check_src("agent C {\n    state {\n        round: int = 0,\n    }\n    fn step() -> int { state.round = \"oops\"\n        0\n    }\n}");
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }

    #[test]
    fn merge_reducer_checks_operands() {
        // clean: list append-dedup and record union
        assert_eq!(check_src("fn f(x: [int]) -> [int] { x | merge x }"), vec![]);
        assert_eq!(check_src("type R = { a: int }\nfn f(x: R) -> R { x | merge x }"), vec![]);
        // E0201: mismatched / scalar operands
        let errs = check_src("fn f(x: int) -> int { x | merge x }");
        assert!(errs.iter().any(|e| e.code == "E0201" && e.msg.contains("merge expects")), "got {errs:?}");
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
        assert!(errs.iter().any(|e| e.code == "E0201" && e.msg.contains("non-bool when")), "got {errs:?}");
    }
}
