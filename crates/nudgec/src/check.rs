//! Nudge type checker core (design §3, §11) — roadmap day 4–6.
//!
//! Scope: alias resolution (records, lists, `@range`/`@format` refinements),
//! schema ↔ return-type agreement, interpolation existence, let/call/return
//! assignability. `Unknown` is the dynamic escape hatch: it is assignable to
//! and from everything, so MCP/Python interop never false-alarms.
//!
//! Diagnostics (design §11, v1.3): E0101 unknown identifier or type name,
//! E0201 type mismatch, E0202 malformed refinement / alias cycle / bad schema.
//!
//! Deferred: effect inference (day 7–8), optional/union types, flow analysis.

use crate::ast::*;
use std::collections::HashMap;
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
        Expr::Float(_) | Expr::Money(_) => Ty::Float, // USD literals are numeric budgets
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
                let mut locals: HashMap<String, Ty> = HashMap::new();
                for p in params {
                    locals.insert(p.name.clone(), resolve(&p.ty, &g, &mut Vec::new(), &mut errs));
                }
                let mut last_ty = Ty::None_;
                for st in body {
                    match st {
                        Stmt::Let { name: n, ty, value } => {
                            let vt = check_expr(value, &locals, &g, &mut errs);
                            let bound = match ty {
                                Some(ann) => {
                                    let at = resolve(ann, &g, &mut Vec::new(), &mut errs);
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
                        Stmt::Assert(e) => {
                            check_expr(e, &locals, &g, &mut errs);
                        }
                        Stmt::ExprStmt(e) => {
                            last_ty = check_expr(e, &locals, &g, &mut errs);
                        }
                    }
                }
                let want = resolve(ret, &g, &mut Vec::new(), &mut errs);
                if !assignable(&last_ty, &want) {
                    errs.push(CheckError {
                        code: "E0201",
                        msg: format!("fn '{name}' declares -> {want} but its body yields {last_ty}"),
                    });
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
                        Stmt::Assert(e) | Stmt::ExprStmt(e) => {
                            check_expr(e, &locals, &g, &mut errs);
                        }
                    }
                }
            }
            Item::TypeAlias { .. } => {}
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
}
