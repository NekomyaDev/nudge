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
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    pub code: &'static str,
    pub msg: String,
    /// Statement span the error was raised in (spanned AST, stage 1) —
    /// `None` for item-level diagnostics that have no single statement home.
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VerificationStage {
    Unverified,
    Validated,
    Verified,
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
    Confidence(Box<Ty>, u32), // confidence threshold in basis points: 0..=10000 (8500 = 0.85)
    Stage(Box<Ty>, VerificationStage),
    View,
    Task(Box<Ty>),
    Optional(Box<Ty>),
    Union(Vec<Ty>),
    Map(Box<Ty>, Box<Ty>),
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
            Ty::Map(k, v) => write!(f, "Map[{k}, {v}]"),
            Ty::Record(fs) => {
                let inner: Vec<String> = fs.iter().map(|(k, t)| format!("{k}: {t}")).collect();
                write!(f, "{{{}}}", inner.join(", "))
            }
            Ty::Refine(base, name) => write!(f, "{base} @{name}"),
            Ty::Confidence(inner, bp) => {
                let score = (*bp as f64) / 10000.0;
                write!(f, "{inner} @confidence({score})")
            }
            Ty::Stage(inner, stage) => match stage {
                VerificationStage::Unverified => write!(f, "Unverified[{inner}]"),
                VerificationStage::Validated => write!(f, "Validated[{inner}]"),
                VerificationStage::Verified => write!(f, "Verified[{inner}]"),
            },
            Ty::View => write!(f, "View"),
            Ty::Task(inner) => write!(f, "Task[{inner}]"),
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::Union(variants) => {
                let inner: Vec<String> = variants.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", inner.join(" | "))
            }
        }
    }
}

#[allow(clippy::type_complexity)]
#[derive(Default)]
struct Globals {
    aliases: HashMap<String, TypeExpr>,
    // name → ((param name, param type)…, return type) — kwargs checking
    // needs the names, not just the positional types
    fns: HashMap<String, (Vec<(String, TypeExpr)>, TypeExpr)>,
    tools: HashMap<String, (Vec<(String, TypeExpr)>, TypeExpr)>,
    views: HashMap<String, (Vec<(String, TypeExpr)>, TypeExpr)>,
    windows: HashMap<String, Vec<(String, Expr)>>,
    agents: HashMap<String, (Vec<(String, TypeExpr, Expr)>, Vec<Item>)>,
    tool_servers: HashMap<String, String>,
    fn_effects: HashMap<String, Vec<String>>,
    auth_providers: HashMap<String, Vec<(String, Expr)>>,
    accounts: HashMap<String, (String, Vec<(String, Expr)>)>,
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
            "View" | "view" => Ty::View,
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
            let inner = resolve(base, g, visiting, errs);
            match name.as_str() {
                "confidence" => {
                    let score = if args.len() == 1 {
                        match &args[0].kind {
                            ExprKind::Int(v) if *v == 0 => Some(0),
                            ExprKind::Int(v) if *v == 1 => Some(10000),
                            ExprKind::Float(f) if *f >= 0.0 && *f <= 1.0 => {
                                Some((*f * 10000.0).round() as u32)
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(bp) = score {
                        Ty::Confidence(Box::new(inner), bp)
                    } else {
                        Ty::Refine(Box::new(inner), name.clone())
                    }
                }
                "stage" => {
                    let st = if args.len() == 1 {
                        match &args[0].kind {
                            ExprKind::Ident(s) | ExprKind::Str(s) => match s.as_str() {
                                "unverified" => Some(VerificationStage::Unverified),
                                "validated" => Some(VerificationStage::Validated),
                                "verified" => Some(VerificationStage::Verified),
                                _ => None,
                            },
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(s) = st {
                        Ty::Stage(Box::new(inner), s)
                    } else {
                        Ty::Refine(Box::new(inner), name.clone())
                    }
                }
                "unverified" => Ty::Stage(Box::new(inner), VerificationStage::Unverified),
                "validated" => Ty::Stage(Box::new(inner), VerificationStage::Validated),
                "verified" => Ty::Stage(Box::new(inner), VerificationStage::Verified),
                _ => Ty::Refine(Box::new(inner), name.clone()),
            }
        }
        TypeExpr::Generic(name, args) => match name.as_str() {
            "Task" if args.len() == 1 => Ty::Task(Box::new(resolve(&args[0], g, visiting, errs))),
            "Unverified" if args.len() == 1 => Ty::Stage(
                Box::new(resolve(&args[0], g, visiting, errs)),
                VerificationStage::Unverified,
            ),
            "Validated" if args.len() == 1 => Ty::Stage(
                Box::new(resolve(&args[0], g, visiting, errs)),
                VerificationStage::Validated,
            ),
            "Verified" if args.len() == 1 => Ty::Stage(
                Box::new(resolve(&args[0], g, visiting, errs)),
                VerificationStage::Verified,
            ),
            "Confidence" if args.len() == 1 => {
                Ty::Confidence(Box::new(resolve(&args[0], g, visiting, errs)), 0)
            }
            "Map" if args.len() == 2 => Ty::Map(
                Box::new(resolve(&args[0], g, visiting, errs)),
                Box::new(resolve(&args[1], g, visiting, errs)),
            ),
            _ => {
                errs.push(CheckError {
                    span: None,
                    code: "E0201",
                    msg: format!("unknown generic type '{name}'"),
                });
                Ty::Unknown
            }
        },
        TypeExpr::Optional(inner) => Ty::Optional(Box::new(resolve(inner, g, visiting, errs))),
        TypeExpr::Union(variants) => Ty::Union(
            variants
                .iter()
                .map(|v| resolve(v, g, visiting, errs))
                .collect(),
        ),
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
        "confidence" => {
            let valid = if args.len() == 1 {
                match &args[0].kind {
                    ExprKind::Int(v) => *v == 0 || *v == 1,
                    ExprKind::Float(f) => (0.0..=1.0).contains(f),
                    _ => false,
                }
            } else {
                false
            };
            if !valid {
                errs.push(CheckError {
                    span: None,
                    code: "E0202",
                    msg: format!(
                        "@confidence expects a numeric threshold between 0.0 and 1.0, e.g. @confidence(0.8) — got {args:?}"
                    ),
                });
            }
        }
        "stage" => {
            let ok = args.len() == 1
                && match &args[0].kind {
                    ExprKind::Ident(s) | ExprKind::Str(s) => {
                        matches!(s.as_str(), "unverified" | "validated" | "verified")
                    }
                    _ => false,
                };
            if !ok {
                errs.push(CheckError {
                    span: None,
                    code: "E0202",
                    msg: format!(
                        "@stage expects one of (unverified, validated, verified) — got {args:?}"
                    ),
                });
            }
        }
        "unverified" | "validated" | "verified" => {
            if !args.is_empty() {
                errs.push(CheckError {
                    span: None,
                    code: "E0202",
                    msg: format!("@{name} expects no arguments, got {args:?}"),
                });
            }
        }
        _ => errs.push(CheckError {
            span: None,
            code: "E0202",
            msg: format!(
                "unknown refinement '@{name}' (known: @range, @format, @confidence, @stage)"
            ),
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
    if let Ty::Union(src_variants) = src {
        return src_variants.iter().all(|s| assignable(s, dst));
    }
    if let Ty::Union(dst_variants) = dst {
        return dst_variants.iter().any(|u| assignable(src, u));
    }
    if let Ty::Optional(inner) = dst {
        if matches!(src, Ty::None_) {
            return true;
        }
        if let Ty::Optional(s_inner) = src {
            return assignable(s_inner, inner);
        }
        return assignable(src, inner);
    }
    match (src, dst) {
        (Ty::Int, Ty::Float) => true, // numeric widening
        (Ty::List(a), Ty::List(b)) => assignable(a, b),
        (Ty::Map(k1, v1), Ty::Map(k2, v2)) => assignable(k1, k2) && assignable(v1, v2),
        (Ty::Record(fields), Ty::Map(k, v)) => {
            if fields.is_empty() {
                true
            } else if assignable(&Ty::Str, k) {
                fields.iter().all(|(_, f_ty)| assignable(f_ty, v))
            } else {
                false
            }
        },
        (Ty::Task(a), Ty::Task(b)) => assignable(a, b),
        (Ty::Record(a), Ty::Record(b)) => {
            a.len() == b.len()
                && b.iter().all(|(k, bt)| {
                    a.iter()
                        .find(|(ak, _)| ak == k)
                        .map(|(_, at)| assignable(at, bt))
                        .unwrap_or(false)
                })
        }
        // Verification stage subtyping:
        // Verified[T] <: Validated[T] <: Unverified[T]
        (Ty::Stage(a, src_stage), Ty::Stage(b, dst_stage)) => {
            src_stage >= dst_stage && assignable(a, b)
        }
        (a, Ty::Stage(b, dst_stage)) => {
            *dst_stage == VerificationStage::Unverified && assignable(a, b)
        }
        (Ty::Stage(a, _), b) => assignable(a, b),

        // Confidence subtyping:
        // T @confidence(c1) <: T @confidence(c2) iff c1 >= c2
        (Ty::Confidence(a, c1), Ty::Confidence(b, c2)) => c1 >= c2 && assignable(a, b),
        (a, Ty::Confidence(b, c2)) => *c2 == 0 && assignable(a, b),
        (Ty::Confidence(a, _), b) => assignable(a, b),

        (Ty::Refine(a, _), b) => assignable(a, b),
        (a, Ty::Refine(b, _)) => assignable(a, b),
        _ => false,
    }
}

fn elem_of(t: &Ty) -> Ty {
    match t {
        Ty::List(e) => (**e).clone(),
        Ty::Stage(inner, st) => match elem_of(inner) {
            Ty::Unknown => Ty::Unknown,
            other => Ty::Stage(Box::new(other), *st),
        },
        Ty::Confidence(inner, c) => match elem_of(inner) {
            Ty::Unknown => Ty::Unknown,
            other => Ty::Confidence(Box::new(other), *c),
        },
        Ty::Optional(inner) => elem_of(inner),
        Ty::Union(variants) => {
            let elems: Vec<Ty> = variants.iter().map(elem_of).collect();
            Ty::Union(elems)
        }
        _ => Ty::Unknown,
    }
}

// ── effect inference (design §3.2, v1.4) ──────────────────────────

const KNOWN_EFFECTS: [&str; 14] = [
    "LLM",
    "Tool",
    "IO",
    "Decision",
    "Computer",
    "Browser",
    "Workspace",
    "WorkspaceDelete",
    "Store",
    "MemStore",
    "UI",
    "Auth",
    "FsWatch",
    "Process",
];

fn is_known_effect(eff: &str) -> bool {
    if KNOWN_EFFECTS.contains(&eff) {
        return true;
    }
    if eff.starts_with("Auth(") && eff.ends_with(')') {
        let inner = &eff[5..eff.len() - 1];
        return !inner.trim().is_empty();
    }
    if eff.starts_with("Tool(") && eff.ends_with(')') {
        let inner = &eff[5..eff.len() - 1];
        return !inner.trim().is_empty();
    }
    false
}

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
        "FsEvent" => r(vec![("kind", Ty::Str), ("path", Ty::Str)]),
        "ProcessOutput" | "ProcResult" => r(vec![
            ("code", Ty::Int),
            ("stdout", Ty::Str),
            ("stderr", Ty::Str),
        ]),
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
        "Snapshot" => r(vec![
            ("page", Ty::Str),
            ("url", Ty::Str),
            ("title", Ty::Str),
            ("state_id", Ty::Str),
            (
                "elements",
                Ty::List(Box::new(builtin_record_ty("Element")?)),
            ),
            ("tree", Ty::Str),
            ("count", Ty::Int),
            ("screenshot", Ty::Str),
            ("drift", builtin_record_ty("Drift")?),
        ]),
        "Header" => r(vec![("name", Ty::Str), ("value", Ty::Str)]),
        "Response" => r(vec![
            ("status", Ty::Int),
            ("headers", Ty::List(Box::new(builtin_record_ty("Header")?))),
            ("body", Ty::Str),
            ("url", Ty::Str),
            ("latency_ms", Ty::Int),
        ]),
        "ActResult" => r(vec![
            ("ok", Ty::Bool),
            ("outcome", Ty::Str),
            ("latency_ms", Ty::Int),
            ("error", Ty::Str),
            ("action_sent", Ty::Bool),
            ("snapshot", builtin_record_ty("Snapshot")?),
        ]),
        "WaitResult" => r(vec![
            ("ok", Ty::Bool),
            ("timed_out", Ty::Bool),
            ("latency_ms", Ty::Int),
            ("snapshot", builtin_record_ty("Snapshot")?),
        ]),
        "Link" => r(vec![
            ("href", Ty::Str),
            ("text", Ty::Str),
            ("download", Ty::Bool),
        ]),
        "Page" => r(vec![
            ("page", Ty::Str),
            ("url", Ty::Str),
            ("title", Ty::Str),
            ("depth", Ty::Int),
        ]),
        "Element" => r(vec![
            ("index", Ty::Int),
            ("ref", Ty::Int),
            ("role", Ty::Str),
            ("title", Ty::Str),
            ("name", Ty::Str),
            ("value", Ty::Str),
            ("tag", Ty::Str),
            ("href", Ty::Str),
            ("pressable", Ty::Bool),
            ("clickable", Ty::Bool),
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
        "CursorPos" => r(vec![("x", Ty::Int), ("y", Ty::Int)]),
        "ScreenSize" => r(vec![("width", Ty::Int), ("height", Ty::Int)]),
        "WindowInfo" => r(vec![
            ("app", Ty::Str),
            ("title", Ty::Str),
            ("width", Ty::Int),
            ("height", Ty::Int),
            ("x", Ty::Int),
            ("y", Ty::Int),
            ("focused", Ty::Bool),
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
            let mut explicit_account = None;
            let mut is_authenticated = false;
            for (k, v) in options {
                if k == "account" {
                    if let ExprKind::Str(acc) = &v.kind {
                        explicit_account = Some(acc.clone());
                    } else if let ExprKind::Ident(acc) = &v.kind {
                        explicit_account = Some(acc.clone());
                    }
                    is_authenticated = true;
                } else if k == "auth" {
                    if let ExprKind::Str(acc) = &v.kind {
                        explicit_account = Some(acc.clone());
                    } else if let ExprKind::Ident(acc) = &v.kind {
                        explicit_account = Some(acc.clone());
                    } else if let ExprKind::Bool(b) = &v.kind {
                        if *b {
                            is_authenticated = true;
                        }
                    }
                } else if k == "live" {
                    if let ExprKind::Bool(b) = &v.kind {
                        if *b {
                            is_authenticated = true;
                        }
                    }
                }
                direct_effects(v, g, effects, calls);
            }

            if let Some(acc) = explicit_account {
                effects.insert(format!("Auth({acc})"));
            } else if !g.accounts.is_empty() {
                if let Some(acc) = g.accounts.keys().next() {
                    effects.insert(format!("Auth({acc})"));
                } else {
                    effects.insert("Auth".into());
                }
            } else if is_authenticated {
                effects.insert("Auth".into());
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
            let callee_name = match &func.kind {
                ExprKind::Ident(n) => Some(n.clone()),
                ExprKind::Field { obj, name } => {
                    if let ExprKind::Ident(prefix) = &obj.kind {
                        Some(format!("{prefix}.{name}"))
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(ref name) = callee_name {
                match name.as_str() {
                    "replay" | "python" => {
                        effects.insert("IO".into());
                    }
                    "run" => {
                        effects.insert("Process".into());
                    }
                    "mcp" => {
                        effects.insert("Tool".into());
                    }
                    "len" | "zip" => {}
                    _ => {
                        if name.starts_with("io.") {
                            effects.insert("IO".into());
                        } else if name.starts_with("browser.") {
                            effects.insert("Browser".into());
                        } else if name.starts_with("workspace.") {
                            // workspace.delete requires both Workspace AND WorkspaceDelete
                            effects.insert("Workspace".into());
                            if name == "workspace.delete" {
                                effects.insert("WorkspaceDelete".into());
                            }
                        } else if name.starts_with("store.") {
                            effects.insert("Store".into());
                        } else if name.starts_with("mem.") {
                            effects.insert("MemStore".into());
                        } else if name.starts_with("fs.") {
                            effects.insert("FsWatch".into());
                        } else if name.starts_with("proc.") {
                            effects.insert("Process".into());
                        } else if name.starts_with("task.") {
                            // Task lifecycle primitives are standard runtime primitives requiring no new effects
                        } else if name.ends_with(".open")
                            || name.ends_with(".mount_headless")
                            || name.ends_with(".dispatch_click")
                            || name.starts_with("ui.")
                        {
                            effects.insert("UI".into());
                        } else if g.tools.contains_key(name) {
                            effects.insert("Tool".into());
                            effects.insert(format!("Tool({name})"));
                        } else if g.fns.contains_key(name) {
                            calls.insert(name.clone());
                        }
                    }
                }
            } else {
                if let ExprKind::Field { name, .. } = &func.as_ref().kind {
                    if name == "open" || name == "mount_headless" || name == "dispatch_click" {
                        effects.insert("UI".into());
                    }
                }
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
        ExprKind::RecordLit(fields) => {
            for (_, v) in fields {
                direct_effects(v, g, effects, calls);
            }
        }
        ExprKind::Index { obj, idx } => {
            direct_effects(obj, g, effects, calls);
            direct_effects(idx, g, effects, calls);
        }
        ExprKind::If { cond, then, else_ } => {
            direct_effects(cond, g, effects, calls);
            direct_effects(then, g, effects, calls);
            if let Some(el) = else_ {
                direct_effects(el, g, effects, calls);
            }
        }
        ExprKind::ParMap {
            coll, kwargs, body, ..
        } => {
            direct_effects(coll, g, effects, calls);
            for (_, v) in kwargs {
                direct_effects(v, g, effects, calls);
            }
            direct_effects(body, g, effects, calls);
        }
        ExprKind::Block(stmts) => body_effects(stmts, g, effects, calls),
        ExprKind::Spawn(_) => {}
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
            StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } => {
                direct_effects(value, g, effects, calls)
            }
            StmtKind::IndexAssign {
                collection,
                index,
                value,
            } => {
                direct_effects(collection, g, effects, calls);
                direct_effects(index, g, effects, calls);
                direct_effects(value, g, effects, calls);
            }
            StmtKind::StateWrite { value, .. } => direct_effects(value, g, effects, calls),
            StmtKind::Assert(e) | StmtKind::ExprStmt(e) => direct_effects(e, g, effects, calls),
            StmtKind::For { coll, body, .. } => {
                direct_effects(coll, g, effects, calls);
                body_effects(body, g, effects, calls);
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                direct_effects(cond, g, effects, calls);
                body_effects(then_body, g, effects, calls);
                if let Some(else_stmts) = else_body {
                    body_effects(else_stmts, g, effects, calls);
                }
            }
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
            Item::View { .. } => out.push(item),
            Item::Agent { fns, .. } => {
                for f in fns {
                    match f {
                        Item::Fn { .. } | Item::View { .. } => out.push(f),
                        _ => {}
                    }
                }
            }
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
                ExprKind::Field { obj, name } => {
                    if let ExprKind::Ident(prefix) = &obj.kind {
                        Some(format!("{prefix}.{name}"))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let tool = callee
                .as_deref()
                .map(|n| {
                    g.tools.contains_key(n)
                        || n.starts_with("io.")
                        || n.starts_with("workspace.")
                        || n.starts_with("store.")
                        || n.starts_with("mem.")
                        || n.starts_with("fs.")
                        || n.starts_with("proc.")
                })
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
        ExprKind::RecordLit(fields) => fields
            .iter()
            .any(|(_, v)| expr_has_effect_call(v, g, inferred)),
        ExprKind::Index { obj, idx } => {
            expr_has_effect_call(obj, g, inferred) || expr_has_effect_call(idx, g, inferred)
        }
        ExprKind::If { cond, then, else_ } => {
            expr_has_effect_call(cond, g, inferred)
                || expr_has_effect_call(then, g, inferred)
                || else_
                    .as_ref()
                    .is_some_and(|el| expr_has_effect_call(el, g, inferred))
        }
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
        ExprKind::Block(stmts) => stmts_have_effect_call(stmts, g, inferred),
        ExprKind::Spawn(_) => false,
        _ => false,
    }
}

fn stmts_have_effect_call(
    body: &[Stmt],
    g: &Globals,
    inferred: &HashMap<String, BTreeSet<String>>,
) -> bool {
    body.iter().any(|st| match &st.kind {
        StmtKind::Let { value, .. }
        | StmtKind::Assign { value, .. }
        | StmtKind::IndexAssign { value, .. }
        | StmtKind::StateWrite { value, .. } => expr_has_effect_call(value, g, inferred),
        StmtKind::Assert(e) | StmtKind::ExprStmt(e) => expr_has_effect_call(e, g, inferred),
        StmtKind::For { coll, body, .. } => {
            expr_has_effect_call(coll, g, inferred) || stmts_have_effect_call(body, g, inferred)
        }
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_effect_call(cond, g, inferred)
                || stmts_have_effect_call(then_body, g, inferred)
                || else_body
                    .as_ref()
                    .is_some_and(|b| stmts_have_effect_call(b, g, inferred))
        }
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
    let mut_locals = HashSet::new();
    check_test_body_inner(body, locals, &mut_locals, g, errs, inferred);
}

fn check_test_body_inner(
    body: &[Stmt],
    locals: &HashMap<String, Ty>,
    mut_locals: &HashSet<String>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
    inferred: &HashMap<String, BTreeSet<String>>,
) {
    let mut locals = locals.clone();
    let mut mut_locals = mut_locals.clone();
    for st in body {
        let before = errs.len();
        match &st.kind {
            StmtKind::Let {
                name,
                ty,
                value,
                mutable,
                ..
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
                if *mutable {
                    mut_locals.insert(name.clone());
                } else {
                    mut_locals.remove(name);
                }
            }
            StmtKind::Assign { name, value } => {
                let vt = check_expr(value, &locals, g, errs);
                if let Some(target_ty) = locals.get(name) {
                    if !mut_locals.contains(name) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0706",
                            msg: format!(
                                "cannot assign to immutable local '{name}'; declare it with `let mut`"
                            ),
                        });
                    } else if !assignable(&vt, target_ty) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "local '{name}' has type {target_ty} but assigned value is {vt}"
                            ),
                        });
                    }
                } else {
                    let registry = g
                        .fns
                        .keys()
                        .chain(g.tools.keys())
                        .chain(g.aliases.keys())
                        .chain(locals.keys());
                    let did = crate::hints::closest(name, registry)
                        .map(|c| format!(", did you mean '{c}'?"))
                        .unwrap_or_default();
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown identifier '{name}'{did}"),
                    });
                }
            }
            StmtKind::IndexAssign {
                collection,
                index,
                value,
            } => {
                let col_ty = check_expr(collection, &locals, g, errs);
                let idx_ty = check_expr(index, &locals, g, errs);
                let val_ty = check_expr(value, &locals, g, errs);

                if let ExprKind::Ident(name) = &collection.kind {
                    if !mut_locals.contains(name) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0706",
                            msg: format!(
                                "cannot mutate immutable local '{name}'; declare it with `let mut`"
                            ),
                        });
                    }
                }

                match col_ty {
                    Ty::List(elem) => {
                        if !assignable(&idx_ty, &Ty::Int) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("list index must be an integer, got {idx_ty}"),
                            });
                        }
                        if !assignable(&val_ty, &elem) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "cannot assign value of type {val_ty} to list of element type {elem}"
                                ),
                            });
                        }
                    }
                    Ty::Map(k, v) => {
                        if !assignable(&idx_ty, &k) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("map key mismatch: expected {k}, got {idx_ty}"),
                            });
                        }
                        if !assignable(&val_ty, &v) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "cannot assign value of type {val_ty} to map of value type {v}"
                                ),
                            });
                        }
                    }
                    Ty::Unknown => {}
                    other => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("cannot index-assign into type {other}"),
                        });
                    }
                }
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
                check_expr_pos(e, &locals, g, errs, true);
            }
            StmtKind::For {
                var,
                idx_var,
                coll,
                body: for_body,
            } => {
                let coll_ty = check_expr(coll, &locals, g, errs);
                let inner_ty = match &coll_ty {
                    Ty::List(elem) => (**elem).clone(),
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("for loop requires a list collection, got {coll_ty}"),
                        });
                        Ty::Unknown
                    }
                };
                check_reserved_binding(var, errs);
                if let Some(idx) = idx_var {
                    check_reserved_binding(idx, errs);
                }
                let mut loop_locals = locals.clone();
                let mut loop_mut_locals = mut_locals.clone();
                loop_locals.insert(var.clone(), inner_ty);
                loop_mut_locals.remove(var);
                if let Some(idx) = idx_var {
                    loop_locals.insert(idx.clone(), Ty::Int);
                    loop_mut_locals.remove(idx);
                }
                check_test_body_inner(for_body, &loop_locals, &loop_mut_locals, g, errs, inferred);
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_ty = check_expr(cond, &locals, g, errs);
                if !assignable(&cond_ty, &Ty::Bool) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("if condition must be bool, got {cond_ty}"),
                    });
                }
                let then_locals = locals.clone();
                let then_mut_locals = mut_locals.clone();
                check_test_body_inner(then_body, &then_locals, &then_mut_locals, g, errs, inferred);
                if let Some(else_stmts) = else_body {
                    let else_locals = locals.clone();
                    let else_mut_locals = mut_locals.clone();
                    check_test_body_inner(
                        else_stmts,
                        &else_locals,
                        &else_mut_locals,
                        g,
                        errs,
                        inferred,
                    );
                }
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

fn check_fn_stmts(
    body: &[Stmt],
    locals: &mut HashMap<String, Ty>,
    mut_locals: &mut HashSet<String>,
    agent_ctx: AgentCtx<'_>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
) -> Ty {
    let mut last_ty = Ty::None_;
    for st in body {
        let before = errs.len();
        match &st.kind {
            StmtKind::Let {
                name: n,
                ty,
                value,
                mutable,
                ..
            } => {
                let vt = check_expr(value, locals, g, errs);
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
                if *mutable {
                    mut_locals.insert(n.clone());
                } else {
                    mut_locals.remove(n);
                }
            }
            StmtKind::Assign { name, value } => {
                let vt = check_expr(value, locals, g, errs);
                if let Some(target_ty) = locals.get(name) {
                    if !mut_locals.contains(name) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0706",
                            msg: format!(
                                "cannot assign to immutable local '{name}'; declare it with `let mut`"
                            ),
                        });
                    } else if !assignable(&vt, target_ty) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "local '{name}' has type {target_ty} but assigned value is {vt}"
                            ),
                        });
                    }
                } else {
                    let registry = g
                        .fns
                        .keys()
                        .chain(g.tools.keys())
                        .chain(g.aliases.keys())
                        .chain(locals.keys());
                    let did = crate::hints::closest(name, registry)
                        .map(|c| format!(", did you mean '{c}'?"))
                        .unwrap_or_default();
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown identifier '{name}'{did}"),
                    });
                }
                last_ty = Ty::None_;
            }
            StmtKind::IndexAssign {
                collection,
                index,
                value,
            } => {
                let col_ty = check_expr(collection, locals, g, errs);
                let idx_ty = check_expr(index, locals, g, errs);
                let val_ty = check_expr(value, locals, g, errs);

                if let ExprKind::Ident(name) = &collection.kind {
                    if !mut_locals.contains(name) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0706",
                            msg: format!(
                                "cannot mutate immutable local '{name}'; declare it with `let mut`"
                            ),
                        });
                    }
                }

                match col_ty {
                    Ty::List(elem) => {
                        if !assignable(&idx_ty, &Ty::Int) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("list index must be an integer, got {idx_ty}"),
                            });
                        }
                        if !assignable(&val_ty, &elem) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "cannot assign value of type {val_ty} to list of element type {elem}"
                                ),
                            });
                        }
                    }
                    Ty::Map(k, v) => {
                        if !assignable(&idx_ty, &k) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("map key mismatch: expected {k}, got {idx_ty}"),
                            });
                        }
                        if !assignable(&val_ty, &v) {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "cannot assign value of type {val_ty} to map of value type {v}"
                                ),
                            });
                        }
                    }
                    Ty::Unknown => {}
                    other => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("cannot index-assign into type {other}"),
                        });
                    }
                }
                last_ty = Ty::None_;
            }
            StmtKind::StateWrite { field, op, value } => {
                let vt = check_expr(value, locals, g, errs);
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
                let at = check_expr(e, locals, g, errs);
                if !assignable(&at, &Ty::Bool) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("assert expects a bool condition, got {at}"),
                    });
                }
            }
            StmtKind::ExprStmt(e) => {
                last_ty = check_expr_pos(e, locals, g, errs, true);
            }
            StmtKind::For {
                var,
                idx_var,
                coll,
                body: for_body,
            } => {
                let coll_ty = check_expr(coll, locals, g, errs);
                let inner_ty = match &coll_ty {
                    Ty::List(elem) => (**elem).clone(),
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("for loop requires a list collection, got {coll_ty}"),
                        });
                        Ty::Unknown
                    }
                };
                check_reserved_binding(var, errs);
                if let Some(idx) = idx_var {
                    check_reserved_binding(idx, errs);
                }
                let mut loop_locals = locals.clone();
                let mut loop_mut_locals = mut_locals.clone();
                loop_locals.insert(var.clone(), inner_ty);
                loop_mut_locals.remove(var);
                if let Some(idx) = idx_var {
                    loop_locals.insert(idx.clone(), Ty::Int);
                    loop_mut_locals.remove(idx);
                }
                check_fn_stmts(
                    for_body,
                    &mut loop_locals,
                    &mut loop_mut_locals,
                    agent_ctx,
                    g,
                    errs,
                );
                last_ty = Ty::None_;
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_ty = check_expr(cond, locals, g, errs);
                if !assignable(&cond_ty, &Ty::Bool) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("if condition must be bool, got {cond_ty}"),
                    });
                }
                let mut then_locals = locals.clone();
                let mut then_mut_locals = mut_locals.clone();
                check_fn_stmts(
                    then_body,
                    &mut then_locals,
                    &mut then_mut_locals,
                    agent_ctx,
                    g,
                    errs,
                );
                if let Some(else_stmts) = else_body {
                    let mut else_locals = locals.clone();
                    let mut else_mut_locals = mut_locals.clone();
                    check_fn_stmts(
                        else_stmts,
                        &mut else_locals,
                        &mut else_mut_locals,
                        agent_ctx,
                        g,
                        errs,
                    );
                }
                last_ty = Ty::None_;
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
    last_ty
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
    let mut mut_locals: HashSet<String> = HashSet::new();
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
    let last_ty = check_fn_stmts(body, &mut locals, &mut mut_locals, agent_ctx, g, errs);
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
/// would hijack the native computer-use surface (`computer.observe(...)`),
/// and `browser` would hijack the browser-use surface (`browser.goto(...)`).
const RESERVED_BINDINGS: [&str; 5] = ["rt", "nudge_runtime", "state", "computer", "browser"];

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
    check_expr_pos(e, locals, g, errs, false)
}

fn check_expr_pos(
    e: &Expr,
    locals: &HashMap<String, Ty>,
    g: &Globals,
    errs: &mut Vec<CheckError>,
    in_stmt_pos: bool,
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
                if let Some((state_fields, fns)) = g.agents.get(n) {
                    let mut rec_fields = Vec::new();
                    let s_fields: Vec<(String, Ty)> = state_fields
                        .iter()
                        .map(|(fname, ty, _)| {
                            (fname.clone(), resolve(ty, g, &mut Vec::new(), errs))
                        })
                        .collect();
                    rec_fields.push(("state".into(), Ty::Record(s_fields)));
                    for f in fns {
                        if let Item::Window { name, .. } = f {
                            rec_fields.push((name.clone(), Ty::Unknown));
                        }
                    }
                    return Ty::Record(rec_fields);
                }
                if g.windows.contains_key(n) {
                    return Ty::Unknown;
                }
                if g.views.contains_key(n) {
                    return Ty::View;
                }
                // fn/tool/alias names may be referenced as values (dynamic
                // interop stays permissive); anything else is E0101
                let known = g.fns.contains_key(n)
                    || g.tools.contains_key(n)
                    || g.aliases.contains_key(n)
                    || g.agents.contains_key(n)
                    || g.views.contains_key(n)
                    || g.windows.contains_key(n);
                if !known {
                    let registry = g
                        .fns
                        .keys()
                        .chain(g.tools.keys())
                        .chain(g.aliases.keys())
                        .chain(g.agents.keys())
                        .chain(g.views.keys());
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
                    "run" => {
                        return builtin_record_ty("ProcessOutput").unwrap_or_else(|| {
                            Ty::Record(vec![
                                ("code".to_string(), Ty::Int),
                                ("stdout".to_string(), Ty::Str),
                                ("stderr".to_string(), Ty::Str),
                            ])
                        });
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
                    "stack" | "box" | "container" => {
                        errs.push(CheckError {
                            span: None,
                            code: "E1002",
                            msg: format!("invalid or unknown UI component '{name}': check the element combinator name and its children or properties"),
                        });
                        return Ty::View;
                    }
                    "column" | "row" | "scroll" => {
                        for a in args {
                            check_expr(a, locals, g, errs);
                        }
                        for (k, v) in kwargs {
                            check_expr(v, locals, g, errs);
                            if k == "on_key" {
                                if let ExprKind::Ident(ref hname) = v.kind {
                                    if let Some(effs) = g.fn_effects.get(hname) {
                                        if !effs.iter().any(|e| e == "UI") {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1001",
                                                msg: format!("event handler '{hname}' missing UI effect: event handlers require `uses UI`"),
                                            });
                                        }
                                    }
                                    if let Some((params, _)) = g.fns.get(hname) {
                                        if params.len() > 1 {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1003",
                                                msg: format!("event handler '{hname}' signature mismatch: expected at most 1 parameter"),
                                            });
                                        } else if params.len() == 1 {
                                            let param_ty =
                                                resolve(&params[0].1, g, &mut Vec::new(), errs);
                                            if param_ty != Ty::Str && param_ty != Ty::Unknown {
                                                errs.push(CheckError {
                                                    span: None,
                                                    code: "E1003",
                                                    msg: format!("event handler '{hname}' signature mismatch: parameter '{}' must be of type string", params[0].0),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ty::View;
                    }
                    // A boolean control hands its handler the new value as a
                    // Bool — that is what the widget *is* — so a handler
                    // declaring `v: string` would be handed something the type
                    // could not hold. Caught here rather than at the first
                    // click, which is the whole point of checking.
                    "checkbox" | "toggle" => {
                        for a in args {
                            check_expr(a, locals, g, errs);
                        }
                        for (k, v) in kwargs {
                            check_expr(v, locals, g, errs);
                            if k == "on_change" {
                                if let ExprKind::Ident(ref hname) = v.kind {
                                    if let Some(effs) = g.fn_effects.get(hname) {
                                        if !effs.iter().any(|e| e == "UI") {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1001",
                                                msg: format!("event handler '{hname}' missing UI effect: event handlers require `uses UI`"),
                                            });
                                        }
                                    }
                                    if let Some((params, _)) = g.fns.get(hname) {
                                        if params.len() > 1 {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1003",
                                                msg: format!("event handler '{hname}' signature mismatch: expected at most 1 parameter"),
                                            });
                                        } else if params.len() == 1 {
                                            let param_ty =
                                                resolve(&params[0].1, g, &mut Vec::new(), errs);
                                            if param_ty != Ty::Bool && param_ty != Ty::Unknown {
                                                errs.push(CheckError {
                                                    span: None,
                                                    code: "E1003",
                                                    msg: format!("event handler '{hname}' signature mismatch: parameter '{}' must be of type bool", params[0].0),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ty::View;
                    }
                    // A slider hands its handler the new value as a number —
                    // an Int when the value lands on a whole number, a Float
                    // otherwise — so a handler declaring `v: string` would be
                    // handed something the type could not hold. Caught here
                    // rather than at the first drag.
                    "slider" => {
                        for a in args {
                            check_expr(a, locals, g, errs);
                        }
                        for (k, v) in kwargs {
                            check_expr(v, locals, g, errs);
                            if k == "on_change" {
                                if let ExprKind::Ident(ref hname) = v.kind {
                                    if let Some(effs) = g.fn_effects.get(hname) {
                                        if !effs.iter().any(|e| e == "UI") {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1001",
                                                msg: format!("event handler '{hname}' missing UI effect: event handlers require `uses UI`"),
                                            });
                                        }
                                    }
                                    if let Some((params, _)) = g.fns.get(hname) {
                                        if params.len() > 1 {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1003",
                                                msg: format!("event handler '{hname}' signature mismatch: expected at most 1 parameter"),
                                            });
                                        } else if params.len() == 1 {
                                            let param_ty =
                                                resolve(&params[0].1, g, &mut Vec::new(), errs);
                                            if param_ty != Ty::Int
                                                && param_ty != Ty::Float
                                                && param_ty != Ty::Unknown
                                            {
                                                errs.push(CheckError {
                                                    span: None,
                                                    code: "E1003",
                                                    msg: format!("event handler '{hname}' signature mismatch: parameter '{}' must be of type int or float", params[0].0),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ty::View;
                    }
                    "text" | "button" | "input" | "bubble" | "select" | "progress" | "spinner"
                    | "image" | "icon" | "tabs" | "code_block" | "divider" | "spacer" | "badge"
                    | "tag" | "status_line" => {
                        for a in args {
                            check_expr(a, locals, g, errs);
                        }
                        for (k, v) in kwargs {
                            check_expr(v, locals, g, errs);
                            if k == "on_click" || k == "on_change" || k == "on_key" {
                                if let ExprKind::Ident(ref hname) = v.kind {
                                    if let Some(effs) = g.fn_effects.get(hname) {
                                        if !effs.iter().any(|e| e == "UI") {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1001",
                                                msg: format!("event handler '{hname}' missing UI effect: event handlers require `uses UI`"),
                                            });
                                        }
                                    }
                                    if let Some((params, _)) = g.fns.get(hname) {
                                        if params.len() > 1 {
                                            errs.push(CheckError {
                                                span: None,
                                                code: "E1003",
                                                msg: format!("event handler '{hname}' signature mismatch: expected at most 1 parameter"),
                                            });
                                        } else if params.len() == 1
                                            && (k == "on_change" || k == "on_key" || k == "on_click")
                                        {
                                            let param_ty =
                                                resolve(&params[0].1, g, &mut Vec::new(), errs);
                                            if param_ty != Ty::Str && param_ty != Ty::Unknown {
                                                errs.push(CheckError {
                                                    span: None,
                                                    code: "E1003",
                                                    msg: format!("event handler '{hname}' signature mismatch: parameter '{}' must be of type string", params[0].0),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ty::View;
                    }
                    _ => {}
                }
                if let Some((params, ret)) = g.views.get(name) {
                    if args.len() > params.len() {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "view '{name}' takes {} argument(s), got {}",
                                params.len(),
                                args.len()
                            ),
                        });
                    }
                    return resolve(ret, g, &mut Vec::new(), errs);
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
            if let ExprKind::Field { obj: _, name } = &func.as_ref().kind {
                match name.as_str() {
                    "exists" => return Ty::Bool,
                    "open" | "mount_headless" => return Ty::Unknown,
                    "find" | "find_text" => return Ty::Unknown,
                    "dispatch_click" => return Ty::None_,
                    "get_text" | "text" => return Ty::Str,
                    _ => {}
                }
            }
            if let ExprKind::Field { obj, name } = &func.as_ref().kind {
                if let ExprKind::Ident(prefix) = &obj.kind {
                    if prefix == "store" || prefix == "mem" {
                        match name.as_str() {
                            "get" => {
                                if let Some((_, dt)) =
                                    kwarg_tys.iter().find(|(k, _)| *k == "default")
                                {
                                    return dt.clone();
                                }
                                if let Some(dt) = arg_tys.get(1) {
                                    return dt.clone();
                                }
                                return Ty::Unknown;
                            }
                            "set" | "clear" => return Ty::None_,
                            "has" | "delete" => return Ty::Bool,
                            "keys" => return Ty::List(Box::new(Ty::Str)),
                            _ => {}
                        }
                    } else if prefix == "workspace" {
                        match name.as_str() {
                            "read" => return Ty::Str,
                            "write" | "append" | "delete" => return Ty::None_,
                            "exists" => return Ty::Bool,
                            "list" => return Ty::List(Box::new(Ty::Str)),
                            "stat" => {
                                return Ty::Record(vec![
                                    ("is_dir".to_string(), Ty::Bool),
                                    ("mtime_ms".to_string(), Ty::Int),
                                    ("size".to_string(), Ty::Int),
                                ])
                            }
                            _ => {}
                        }
                    } else if prefix == "fs" {
                        match name.as_str() {
                            "watch" => {
                                let ev_ty = builtin_record_ty("FsEvent").unwrap_or_else(|| {
                                    Ty::Record(vec![
                                        ("kind".to_string(), Ty::Str),
                                        ("path".to_string(), Ty::Str),
                                    ])
                                });
                                return Ty::List(Box::new(ev_ty));
                            }
                            "changed" => return Ty::Bool,
                            _ => {}
                        }
                    } else if prefix == "ui" {
                        match name.as_str() {
                            "column" | "row" | "scroll" | "text" | "button" | "input"
                            | "bubble" | "select" | "checkbox" | "toggle" | "slider"
                            | "progress" | "spinner" | "image" | "icon" | "tabs" | "code_block"
                            | "divider" | "spacer" | "badge" | "tag" | "status_line" => {
                                return Ty::View
                            }
                            other => {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E1002",
                                    msg: format!("invalid or unknown UI component 'ui.{other}': check the element combinator name and its children or properties"),
                                });
                                return Ty::Unknown;
                            }
                        }
                    } else if prefix == "proc" {
                        match name.as_str() {
                            "run" | "await" | "wait" => {
                                return builtin_record_ty("ProcessOutput").unwrap_or_else(|| {
                                    Ty::Record(vec![
                                        ("code".to_string(), Ty::Int),
                                        ("stdout".to_string(), Ty::Str),
                                        ("stderr".to_string(), Ty::Str),
                                    ])
                                });
                            }
                            "spawn" => return Ty::Str,
                            "kill" | "terminate" | "alive" | "is_alive" => return Ty::Bool,
                            _ => {}
                        }
                    } else if prefix == "task" {
                        match name.as_str() {
                            "status" => return Ty::Str,
                            "cancel" => return Ty::None_,
                            "is_cancelled" => return Ty::Bool,
                            "wait_cancelled" => return Ty::None_,
                            "join" => {
                                if !args.is_empty() {
                                    let arg_ty = check_expr(&args[0], locals, g, errs);
                                    if let Ty::Task(inner) = arg_ty {
                                        return *inner;
                                    }
                                }
                                return Ty::Unknown;
                            }
                            "spawn" => {
                                return Ty::Record(vec![
                                    ("id".to_string(), Ty::Str),
                                    ("state".to_string(), Ty::Str),
                                ]);
                            }
                            _ => {}
                        }
                    } else if prefix == "math" {
                        match name.as_str() {
                            "abs" | "floor" | "ceil" | "round" | "sqrt" | "sin" | "cos" | "tan"
                            | "exp" | "ln" => {
                                if args.is_empty() {
                                    errs.push(CheckError {
                                        span: None,
                                        code: "E0201",
                                        msg: format!("'math.{name}' takes 1 argument, got 0"),
                                    });
                                }
                                return arg_tys.first().cloned().unwrap_or(Ty::Float);
                            }
                            "pow" | "log" | "min" | "max" => {
                                return arg_tys.first().cloned().unwrap_or(Ty::Float);
                            }
                            "random_int"
                            | "bitwise_xor"
                            | "bitwise_and"
                            | "bitwise_or"
                            | "bitwise_not"
                            | "shift_left"
                            | "shift_right" => return Ty::Int,
                            "random_float" | "mean" => return Ty::Float,
                            _ => {}
                        }
                    } else if prefix == "int" {
                        match name.as_str() {
                            "bitwise_xor"
                            | "bitwise_and"
                            | "bitwise_or"
                            | "bitwise_not"
                            | "shift_left"
                            | "shift_right" => return Ty::Int,
                            _ => {}
                        }
                    } else if prefix == "str" {
                        match name.as_str() {
                            "length" | "to_int" | "char_code" | "hash" => return Ty::Int,
                            "to_float" => return Ty::Float,
                            "upper" | "lower" | "trim" | "replace" | "substring" | "to_string"
                            | "from_char_code" => return Ty::Str,
                            "contains" | "starts_with" | "ends_with" => return Ty::Bool,
                            "split" => return Ty::List(Box::new(Ty::Str)),
                            "index_of" => return Ty::Int,
                            _ => {}
                        }
                    } else if let Some((params, ret)) = g.fns.get(&format!("{prefix}.{name}")) {
                        let full_name = format!("{prefix}.{name}");
                        if args.len() > params.len() {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!(
                                    "'{full_name}' takes {} argument(s), got {}",
                                    params.len(),
                                    args.len()
                                ),
                            });
                        } else {
                            for (at, (_, pt)) in arg_tys.iter().zip(params.iter()) {
                                let want = resolve(pt, g, &mut Vec::new(), errs);
                                if !assignable(at, &want) {
                                    errs.push(CheckError {
                                        span: None,
                                        code: "E0201",
                                        msg: format!(
                                            "argument to '{full_name}' must be {want}, got {at}"
                                        ),
                                    });
                                }
                            }
                        }
                        return resolve(ret, g, &mut Vec::new(), errs);
                    }
                }
            }
            Ty::Unknown
        }
        ExprKind::Field { obj, name } => {
            let mut obj_ty = check_expr(obj, locals, g, errs);
            while let Ty::Stage(inner, _) | Ty::Confidence(inner, _) | Ty::Refine(inner, _) = obj_ty
            {
                obj_ty = *inner;
            }
            match obj_ty {
                Ty::Task(_) => {
                    if name == "id" || name == "state" || name == "status" {
                        Ty::Str
                    } else {
                        errs.push(CheckError {
                            span: None,
                            code: "E0101",
                            msg: format!("task has no field '{name}'"),
                        });
                        Ty::Unknown
                    }
                }
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
            }
        }
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
                            Ty::Refine(b, ..) | Ty::Confidence(b, ..) | Ty::Stage(b, ..) => {
                                matches!(b.as_ref(), Ty::Int | Ty::Float)
                            }
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
                BinOp::Add if matches!(&lt, Ty::Str) || matches!(&rt_, Ty::Str) => Ty::Str,
                // list concatenation: the VM and the interpreter both implement
                // `[a] + [b]` (vm::tests::test_parity_lists_and_concat), so the
                // checker must not reject what the runtime accepts. The element
                // type widens the same way assignment does; incompatible element
                // types stay an error.
                BinOp::Add if matches!((&lt, &rt_), (Ty::List(_), Ty::List(_))) => {
                    let (Ty::List(l_elem), Ty::List(r_elem)) = (&lt, &rt_) else {
                        unreachable!("guarded by matches! above")
                    };
                    if assignable(l_elem, r_elem) {
                        Ty::List(r_elem.clone())
                    } else if assignable(r_elem, l_elem) {
                        Ty::List(l_elem.clone())
                    } else {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("cannot concatenate {lt} and {rt_}"),
                        });
                        Ty::List(Box::new(Ty::Unknown))
                    }
                }
                _ => {
                    // arithmetic is numeric-only; previously a non-numeric
                    // operand silently fell through to Ty::Unknown and
                    // `true + 1` / `"a" * 3` compiled with no diagnostic
                    let numeric_ty = |t: &Ty| match t {
                        Ty::Int | Ty::Float => true,
                        Ty::Refine(b, ..) | Ty::Confidence(b, ..) | Ty::Stage(b, ..) => {
                            matches!(b.as_ref(), Ty::Int | Ty::Float)
                        }
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
                    builtin_record_ty("Observation").unwrap_or(Ty::Unknown)
                }
                "screen_size" | "cursor_pos" => {
                    if !args.is_empty() {
                        errs.push(CheckError {
                            span: None,
                            code: "E0902",
                            msg: format!("computer.{method} takes 0 arguments, got {}", args.len()),
                        });
                    }
                    builtin_record_ty(if method == "screen_size" {
                        "ScreenSize"
                    } else {
                        "CursorPos"
                    })
                    .unwrap_or(Ty::Unknown)
                }
                "window_info" => {
                    if args.len() > 1 {
                        errs.push(CheckError {
                            span: None,
                            code: "E0902",
                            msg: format!(
                                "computer.window_info takes 0 to 1 argument(s), got {}",
                                args.len()
                            ),
                        });
                    }
                    builtin_record_ty("WindowInfo").unwrap_or(Ty::Unknown)
                }
                "click" | "drag" | "set_value" | "type" | "key" | "scroll" | "perform"
                | "paste" | "mouse_move" | "mouse_down" | "mouse_up" => {
                    // exact signatures: an extra positional would silently
                    // land in a different slot of the generated runtime call
                    // (e.g. allow) and change security semantics
                    let (min_args, max_args) = match method.as_str() {
                        "click" | "type" | "key" | "mouse_down" | "mouse_up" => (1, 1),
                        "paste" => (1, 2),
                        "set_value" | "drag" | "perform" | "mouse_move" => (2, 2),
                        _ => (2, 3), // scroll(target, direction[, pages])
                    };
                    if args.len() < min_args || args.len() > max_args {
                        let expected = if min_args == max_args {
                            format!("exactly {min_args}")
                        } else {
                            format!("{min_args} to {max_args}")
                        };
                        errs.push(CheckError {
                            span: None,
                            code: "E0902",
                            msg: format!(
                                "computer.{method} takes {expected} argument(s), got {}",
                                args.len()
                            ),
                        });
                    }
                    builtin_record_ty("ActionResult").unwrap_or(Ty::Unknown)
                }
                other => {
                    // unreachable via the parser (COMPUTER_METHODS gates it);
                    // kept for ASTs built by other tools
                    errs.push(CheckError {
                        span: None,
                        code: "E0901",
                        msg: format!(
                            "unknown computer method '{other}' (known: observe, click, type, key, scroll, set_value, drag, perform, paste, screen_size, window_info, mouse_move, mouse_down, mouse_up, cursor_pos)"
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
                    "screenshot" if method != "observe" => {
                        // the generated action calls have no screenshot
                        // parameter — a static pass here would TypeError
                        // at runtime
                        errs.push(CheckError {
                            span: None,
                            code: "E0901",
                            msg: format!(
                                "computer.{method} does not take a 'screenshot' option (only computer.observe does)"
                            ),
                        });
                    }
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
        ExprKind::RecordLit(fields) => {
            let mut seen = std::collections::HashSet::new();
            let mut record_fields = Vec::new();
            for (name, val) in fields {
                if !seen.insert(name.clone()) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0203",
                        msg: format!("duplicate field '{name}' in record literal"),
                    });
                }
                let ty = check_expr(val, locals, g, errs);
                record_fields.push((name.clone(), ty));
            }
            Ty::Record(record_fields)
        }
        ExprKind::Index { obj, idx } => {
            let obj_ty = check_expr(obj, locals, g, errs);
            let idx_ty = check_expr(idx, locals, g, errs);
            match obj_ty {
                Ty::List(elem) => {
                    if !assignable(&idx_ty, &Ty::Int) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("list index must be an integer, got {idx_ty}"),
                        });
                    }
                    *elem
                }
                Ty::Record(fields) => {
                    if !assignable(&idx_ty, &Ty::Str) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("record index must be a string, got {idx_ty}"),
                        });
                        Ty::Unknown
                    } else if let ExprKind::Str(s) = &idx.kind {
                        if let Some((_, fty)) = fields.iter().find(|(f, _)| f == s) {
                            fty.clone()
                        } else {
                            errs.push(CheckError {
                                span: None,
                                code: "E0201",
                                msg: format!("record has no field '{s}'"),
                            });
                            Ty::Unknown
                        }
                    } else {
                        Ty::Unknown
                    }
                }
                Ty::Map(k, v) => {
                    if !assignable(&idx_ty, &k) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!("map index must be of type {k}, got {idx_ty}"),
                        });
                    }
                    *v
                }
                Ty::Unknown => Ty::Unknown,
                _ => {
                    errs.push(CheckError {
                        span: None,
                        code: "E0201",
                        msg: format!("cannot index into type {obj_ty}"),
                    });
                    Ty::Unknown
                }
            }
        }
        ExprKind::If { cond, then, else_ } => {
            let cond_ty = check_expr(cond, locals, g, errs);
            if !assignable(&cond_ty, &Ty::Bool) {
                errs.push(CheckError {
                    span: None,
                    code: "E0201",
                    msg: format!("if condition must be bool, got {cond_ty}"),
                });
            }
            match else_ {
                None => {
                    if in_stmt_pos {
                        check_expr_pos(then, locals, g, errs, true);
                        Ty::None_
                    } else {
                        errs.push(CheckError {
                            span: None,
                            code: "E0704",
                            msg: "if expression used as a value must include an else branch".into(),
                        });
                        check_expr(then, locals, g, errs)
                    }
                }
                Some(else_expr) => {
                    let then_ty = check_expr_pos(then, locals, g, errs, in_stmt_pos);
                    let else_ty = check_expr_pos(else_expr, locals, g, errs, in_stmt_pos);
                    if !assignable(&then_ty, &else_ty) && !assignable(&else_ty, &then_ty) {
                        errs.push(CheckError {
                            span: None,
                            code: "E0201",
                            msg: format!(
                                "if and else branches have incompatible types: then yields {then_ty}, else yields {else_ty}"
                            ),
                        });
                    }
                    if assignable(&then_ty, &else_ty) {
                        else_ty
                    } else {
                        then_ty
                    }
                }
            }
        }
        ExprKind::Spawn(body) => {
            let inner_ty = check_expr(body, locals, g, errs);
            Ty::Task(Box::new(inner_ty))
        }
        ExprKind::Block(stmts) => {
            let mut child_locals = locals.clone();
            let mut child_mut_locals = HashSet::new();
            check_fn_stmts(
                stmts,
                &mut child_locals,
                &mut child_mut_locals,
                AgentCtx::None,
                g,
                errs,
            )
        }
    }
}

// ── entry point ─────────────────────────────────────────────────────

fn tool_matches_declared(
    want_tool: &str,
    declared: &[String],
    tool_servers: &HashMap<String, String>,
) -> bool {
    if declared.iter().any(|d| d == "Tool" || d == "Tool(*)") {
        return true;
    }
    let tool_target = if want_tool.starts_with("Tool(") && want_tool.ends_with(')') {
        &want_tool[5..want_tool.len() - 1]
    } else {
        want_tool
    };
    let server_opt = tool_servers.get(tool_target);
    for d in declared {
        if d.starts_with("Tool(") && d.ends_with(')') {
            let rule = &d[5..d.len() - 1];
            if rule == "*" || rule == tool_target {
                return true;
            }
            if nudge_runtime::grants::fnmatch(rule, tool_target) {
                return true;
            }
            if let Some(srv) = server_opt {
                let qualified = format!("{srv}/{tool_target}");
                if rule == qualified || nudge_runtime::grants::fnmatch(rule, &qualified) {
                    return true;
                }
            }
            if tool_target.contains('/') {
                if let Some(unqualified) = tool_target.split('/').next_back() {
                    if rule == unqualified || nudge_runtime::grants::fnmatch(rule, unqualified) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check a whole program. Returns every diagnostic found (deterministic order).
pub fn check(items: &[Item]) -> Vec<CheckError> {
    let mut g = Globals::default();
    let mut errs = Vec::new();

    // generated identifiers must stay owner-only: user code binding these
    // names silently breaks the emitted Python/TS (`rt.llm_call` resolving
    // to a user fn, `_state_A` shadowing a checkpoint object). `computer`
    // is reserved the same way — `computer.observe(...)` is the native
    // computer-use surface (v1.5), and `browser` is the browser surface,
    // where a user binding would hijack it.
    const RESERVED: [&str; 5] = ["rt", "nudge_runtime", "state", "computer", "browser"];
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
                name,
                params,
                ret,
                effects,
                ..
            } => {
                g.fn_effects.insert(name.clone(), effects.clone());
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
                name,
                params,
                ret,
                fields,
            } => {
                if let Some(srv) = crate::codegen::impl_server(fields) {
                    g.tool_servers.insert(name.clone(), srv);
                }
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
            Item::Agent {
                name: agent,
                state,
                fns,
            } => {
                if g.agents
                    .insert(agent.clone(), (state.clone(), fns.clone()))
                    .is_some()
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate agent '{agent}'"),
                    });
                }
                for f in fns {
                    match f {
                        Item::Fn {
                            name,
                            params,
                            ret,
                            effects,
                            ..
                        } => {
                            g.fn_effects.insert(name.clone(), effects.clone());
                            g.fn_effects
                                .insert(format!("{agent}.{name}"), effects.clone());
                            let sig = (
                                params
                                    .iter()
                                    .map(|p| (p.name.clone(), p.ty.clone()))
                                    .collect(),
                                ret.clone(),
                            );
                            g.fns.insert(format!("{agent}.{name}"), sig.clone());
                            if g.fns.insert(name.clone(), sig).is_some() {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E0102",
                                    msg: format!("duplicate fn '{name}' (agent fns share the top-level namespace)"),
                                });
                            }
                        }
                        Item::Window { name, fields } => {
                            if g.windows.insert(name.clone(), fields.clone()).is_some()
                                || g.windows
                                    .insert(format!("{agent}.{name}"), fields.clone())
                                    .is_some()
                            {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E1004",
                                    msg: format!("duplicate window '{name}'"),
                                });
                            }
                        }
                        Item::View {
                            name, params, ret, ..
                        } => {
                            let is_dup = g
                                .views
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
                                || g.fns.contains_key(name);
                            if is_dup {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E1004",
                                    msg: format!("duplicate view '{name}'"),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::View {
                name, params, ret, ..
            } => {
                if g.views
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
                    || g.fns.contains_key(name)
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E1004",
                        msg: format!("duplicate view '{name}'"),
                    });
                }
            }
            Item::Window { name, fields } => {
                if g.windows.insert(name.clone(), fields.clone()).is_some() {
                    errs.push(CheckError {
                        span: None,
                        code: "E1004",
                        msg: format!("duplicate window '{name}'"),
                    });
                }
            }
            Item::AuthProvider { name, fields } => {
                if g.auth_providers
                    .insert(name.clone(), fields.clone())
                    .is_some()
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate auth provider '{name}'"),
                    });
                }
            }
            Item::Account {
                name,
                provider,
                fields,
            } => {
                if g.accounts
                    .insert(name.clone(), (provider.clone(), fields.clone()))
                    .is_some()
                {
                    errs.push(CheckError {
                        span: None,
                        code: "E0102",
                        msg: format!("duplicate account '{name}'"),
                    });
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
            if let Some(entry) = inferred.get_mut(name) {
                for e in add {
                    if entry.insert(e) {
                        changed = true;
                    }
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
                    match f {
                        Item::Fn {
                            name,
                            params,
                            ret,
                            body,
                            ..
                        } => {
                            check_fn_body(
                                name,
                                params,
                                ret,
                                body,
                                Some((agent, state)),
                                &g,
                                &mut errs,
                            );
                        }
                        Item::Window { fields, .. } => {
                            let mut locals = HashMap::new();
                            let state_fields: Vec<(String, Ty)> = state
                                .iter()
                                .map(|(fname, ty, _)| {
                                    (fname.clone(), resolve(ty, &g, &mut Vec::new(), &mut errs))
                                })
                                .collect();
                            locals.insert("state".into(), Ty::Record(state_fields));
                            for (_, v) in fields {
                                check_expr(v, &locals, &g, &mut errs);
                            }
                        }
                        Item::View {
                            name,
                            params,
                            ret,
                            body,
                        } => {
                            check_fn_body(
                                name,
                                params,
                                ret,
                                body,
                                Some((agent, state)),
                                &g,
                                &mut errs,
                            );
                        }
                        _ => {}
                    }
                }
            }
            Item::View {
                name,
                params,
                ret,
                body,
            } => {
                check_fn_body(name, params, ret, body, None, &g, &mut errs);
            }
            Item::Window { fields, .. } => {
                for (_, v) in fields {
                    check_expr(v, &HashMap::new(), &g, &mut errs);
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
            Item::AuthProvider { fields, .. } => {
                for (_, v) in fields {
                    check_expr(v, &HashMap::new(), &g, &mut errs);
                }
            }
            Item::Account {
                provider, fields, ..
            } => {
                if !g.auth_providers.is_empty() && !g.auth_providers.contains_key(provider) {
                    errs.push(CheckError {
                        span: None,
                        code: "E-auth-0404",
                        msg: format!("provider '{provider}' is not declared in auth configuration"),
                    });
                }
                for (_, v) in fields {
                    check_expr(v, &HashMap::new(), &g, &mut errs);
                }
            }
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
                if !is_known_effect(d) {
                    errs.push(CheckError {
                        span: None,
                        code: "E0101",
                        msg: format!("unknown effect '{d}' in fn '{name}' (known: LLM, Tool, IO, Decision, Computer, Workspace, WorkspaceDelete, Store, UI, Auth, FsWatch, Process)"),
                    });
                }
            }
            let want = &inferred[name];
            let specific_tools: Vec<&String> =
                want.iter().filter(|w| w.starts_with("Tool(")).collect();
            let all_tools_satisfied = !specific_tools.is_empty()
                && specific_tools
                    .iter()
                    .all(|st| tool_matches_declared(st, declared, &g.tool_servers));

            let missing: Vec<&String> = want
                .iter()
                .filter(|e| {
                    if *e == "Auth" {
                        !declared
                            .iter()
                            .any(|d| d == "Auth" || d.starts_with("Auth("))
                    } else if e.starts_with("Auth(") {
                        !declared.iter().any(|d| d == *e || d == "Auth")
                    } else if *e == "Tool" {
                        if declared.iter().any(|d| d == "Tool" || d == "Tool(*)") {
                            return false;
                        }
                        if all_tools_satisfied {
                            return false;
                        }
                        specific_tools.is_empty()
                    } else if e.starts_with("Tool(") {
                        !tool_matches_declared(e, declared, &g.tool_servers)
                    } else {
                        !declared.iter().any(|d| d == *e)
                    }
                })
                .collect();
            if missing.is_empty() {
                // Scope verification for declared Auth(acc)
                for d in declared {
                    if d.starts_with("Auth(") && d.ends_with(')') {
                        let acc_name = &d[5..d.len() - 1];
                        if let Some((_, fields)) = g.accounts.get(acc_name) {
                            let mut account_scopes = Vec::new();
                            for (fname, fval) in fields {
                                if fname == "scope" {
                                    match &fval.kind {
                                        ExprKind::ListLit(items) => {
                                            for it in items {
                                                if let ExprKind::Str(s) = &it.kind {
                                                    account_scopes.push(s.clone());
                                                }
                                            }
                                        }
                                        ExprKind::Str(s) => {
                                            for part in s.split_whitespace() {
                                                account_scopes.push(part.to_string());
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            if inferred[name].contains("LLM")
                                && !account_scopes.is_empty()
                                && !account_scopes.iter().any(|s| s == "model:inference")
                            {
                                errs.push(CheckError {
                                    span: None,
                                    code: "E-auth-0403",
                                    msg: format!("account '{acc_name}' lacks required scope 'model:inference' (has: {account_scopes:?})"),
                                });
                            }
                        }
                    }
                }
                continue;
            }
            let list = missing
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let has_ui = missing.iter().any(|e| *e == "UI");
            let has_auth = missing
                .iter()
                .any(|e| *e == "Auth" || e.starts_with("Auth("));
            let has_workspace_delete = missing.iter().any(|e| *e == "WorkspaceDelete");
            if declared.is_empty() {
                errs.push(CheckError {
                    span: None,
                    code: if has_auth {
                        "E-auth-0403"
                    } else if has_ui {
                        "E1001"
                    } else {
                        "E0301"
                    },
                    msg: format!(
                        "fn '{name}' uses {list} but has no `uses` clause — add `uses {list}` to its signature"
                    ),
                });
            } else {
                errs.push(CheckError {
                    span: None,
                    code: if has_auth {
                        "E-auth-0403"
                    } else if has_ui {
                        "E1001"
                    } else if has_workspace_delete {
                        "E0912"
                    } else {
                        "E0302"
                    },
                    msg: format!(
                        "fn '{name}' declares `uses {}` but its body also uses {list} — annotation too narrow",
                        declared.join(", ")
                    ),
                });
            }
        }
    }

    // ── static cost bound verification (roadmap v1.5 Door 3 Safe) ───────
    let fn_costs = crate::cost::calculate_transitive_costs(items);
    for item in fn_items(items) {
        if let Item::Fn {
            name,
            cost_bound: Some(bound_expr),
            ..
        } = item
        {
            // Evaluate the bound expr: must be ExprKind::Money with USD
            match &bound_expr.kind {
                ExprKind::Money(amount, unit) => {
                    if unit != "USD" {
                        errs.push(CheckError {
                            span: Some(bound_expr.span),
                            code: "E0501",
                            msg: format!("unknown budget unit '{unit}' (v0.1 supports USD only)"),
                        });
                    } else if let Some(cost) = fn_costs.get(name) {
                        let worst_case = cost.transitive_max;
                        if worst_case > *amount + 1e-9 {
                            errs.push(CheckError {
                                span: Some(bound_expr.span),
                                code: "E0502",
                                msg: format!(
                                    "static cost bound exceeded: fn '{name}' declared costs < {amount:.2} USD but static call graph worst-case is {worst_case:.2} USD"
                                ),
                            });
                        }
                    }
                }
                _ => {
                    errs.push(CheckError {
                        span: Some(bound_expr.span),
                        code: "E0201",
                        msg: "cost bound must be a money literal like 0.05 USD".to_string(),
                    });
                }
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

/// Diagnostic report item for machine-readable JSON output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckDiagnosticJson {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub span: Option<Span>,
}

/// Structured type-checker report returned by `check_json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckOutputJson {
    pub ok: bool,
    pub items_count: usize,
    pub errors_count: usize,
    pub errors: Vec<CheckDiagnosticJson>,
}

/// 1-based line:col of a byte offset, for diagnostics.
pub fn line_col(src: &str, at: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let at = at.min(src.len());
    for (byte_idx, ch) in src.char_indices() {
        if byte_idx >= at {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Check a whole program and serialize the diagnostics report as structured JSON.
pub fn check_json(items: &[Item], src: &str) -> String {
    let errs = check(items);
    let ok = errs.is_empty();
    let errors: Vec<CheckDiagnosticJson> = errs
        .into_iter()
        .map(|e| {
            let (line, col) = match e.span {
                Some(sp) => {
                    let (l, c) = line_col(src, sp.start);
                    (Some(l), Some(c))
                }
                None => (None, None),
            };
            CheckDiagnosticJson {
                code: e.code.to_string(),
                message: e.msg,
                line,
                col,
                span: e.span,
            }
        })
        .collect();

    let output = CheckOutputJson {
        ok,
        items_count: items.len(),
        errors_count: errors.len(),
        errors,
    };

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
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
    fn optional_type_assignment() {
        // assigning none to string? passes
        let errs = check_src("fn f() -> string? { none }");
        assert!(errs.is_empty(), "got {errs:?}");

        // assigning hello to string? passes
        let errs = check_src("fn f() -> string? { \"hello\" }");
        assert!(errs.is_empty(), "got {errs:?}");

        // assigning in let binding
        let errs = check_src("fn f() { let x: string? = none\nlet y: string? = \"hello\" }");
        assert!(errs.is_empty(), "got {errs:?}");
    }

    #[test]
    fn union_type_assignment() {
        // assigning 10 to int | string passes
        let errs = check_src("fn f() -> int | string { 10 }");
        assert!(errs.is_empty(), "got {errs:?}");

        // assigning hi to int | string passes
        let errs = check_src("fn f() -> int | string { \"hi\" }");
        assert!(errs.is_empty(), "got {errs:?}");

        // assigning true to int | string fails with E0201
        let errs = check_src("fn f() -> int | string { true }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        let errs = check_src("fn f() { let x: int | string = true }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );
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
    fn browser_is_a_reserved_name() {
        let errs = check_src("fn f() -> int { let browser = 3\n    browser }");
        assert!(errs.iter().any(|e| e.code == "E0103"), "{errs:?}");
    }

    #[test]
    fn browser_infers_the_browser_effect() {
        let missing = check_src(
            "fn f(url: string) -> int {\n    let r = browser.fetch(url)\n    r.status\n}",
        );
        assert!(
            missing
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Browser")),
            "{missing:?}"
        );

        let ok = check_src("fn f(url: string) -> int uses Browser {\n    let r = browser.fetch(url)\n    r.status\n}");
        assert!(ok.is_empty(), "{ok:?}");
    }

    #[test]
    fn browser_builtin_types_are_typed() {
        let src = r#"
fn inspect(s: Snapshot) -> bool uses Browser {
    s.count > 0 and s.drift.changed and len(s.elements) >= 0 and s.title != ""
}
"#;
        let ok = check_src(src);
        assert!(ok.is_empty(), "{ok:?}");
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
    fn io_file_functions_require_uses_io_and_roundtrip() {
        // Without uses IO, calling io.read trips E0301
        let errs = check_src("fn f(p: string) -> string { io.read(p) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("IO")),
            "got {errs:?}"
        );

        // With uses IO, checks clean
        let errs2 = check_src("fn f(p: string) -> string uses IO { io.read(p) }");
        assert_eq!(errs2, vec![]);

        let errs3 =
            check_src("fn f(p: string, c: string) -> string uses IO {\nio.write(p, c)\np\n}");
        assert_eq!(errs3, vec![]);

        let errs4 = check_src("fn f(p: string) -> bool uses IO { io.exists(p) }");
        assert_eq!(errs4, vec![]);
    }

    #[test]
    fn workspace_read_requires_workspace_effect() {
        // workspace.read without Workspace declared trips E0301
        let errs = check_src("fn f(p: string) -> string { workspace.read(p) }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Workspace")),
            "expected E0301 for missing Workspace, got {errs:?}"
        );

        // With Workspace declared, clean
        let errs2 = check_src("fn f(p: string) -> string uses Workspace { workspace.read(p) }");
        assert_eq!(errs2, vec![], "unexpected errors: {errs2:?}");
    }

    #[test]
    fn workspace_write_requires_workspace_effect() {
        // workspace.write without Workspace declared trips E0301
        let errs = check_src(
            "fn f(p: string, c: string) -> string uses Workspace { workspace.write(p, c)\np }",
        );
        assert_eq!(errs, vec![], "unexpected errors: {errs:?}");

        let errs2 = check_src("fn f(p: string, c: string) -> string { workspace.write(p, c)\np }");
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Workspace")),
            "expected E0301 for missing Workspace, got {errs2:?}"
        );
    }

    #[test]
    fn workspace_delete_requires_workspace_and_workspace_delete_effect() {
        // workspace.delete without either effect trips E0301 listing both
        let errs = check_src("fn f(p: string) -> string { workspace.delete(p)\np }");
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Workspace")),
            "expected E0301 for Workspace, got {errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.code == "E0301" && e.msg.contains("WorkspaceDelete")),
            "expected E0301 for WorkspaceDelete, got {errs:?}"
        );

        // With only Workspace, still trips E0912 for WorkspaceDelete (annotation too narrow)
        let errs2 =
            check_src("fn f(p: string) -> string uses Workspace { workspace.delete(p)\np }");
        assert!(
            errs2.iter().any(
                |e| (e.code == "E0912" || e.code == "E0301" || e.code == "E0302")
                    && e.msg.contains("WorkspaceDelete")
            ),
            "expected E0912/E0301/E0302 for WorkspaceDelete, got {errs2:?}"
        );
        assert!(
            errs2.iter().any(|e| e.code == "E0912"),
            "expected E0912 for WorkspaceDelete, got {errs2:?}"
        );

        // With both effects declared, clean
        let errs3 = check_src(
            "fn f(p: string) -> string uses Workspace, WorkspaceDelete { workspace.delete(p)\np }",
        );
        assert_eq!(errs3, vec![], "unexpected errors: {errs3:?}");
    }

    #[test]
    fn workspace_effects_are_known() {
        // Workspace and WorkspaceDelete are valid effect names (not E0101)
        let errs = check_src(
            "fn f(p: string) -> string uses Workspace, WorkspaceDelete { workspace.read(p) }",
        );
        assert!(
            !errs.iter().any(|e| e.code == "E0101"),
            "Workspace/WorkspaceDelete should be known effects, got {errs:?}"
        );
    }

    #[test]
    fn store_requires_store_effect() {
        // store.get without Store effect trips E0301
        let errs1 = check_src("fn f() -> int { store.get(\"k\", 0) }");
        assert!(
            errs1
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Store")),
            "expected E0301 for store.get, got {errs1:?}"
        );

        // store.set without Store effect trips E0301
        let errs2 = check_src("fn f() -> () { store.set(\"k\", 42) }");
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Store")),
            "expected E0301 for store.set, got {errs2:?}"
        );

        // store with uses Store is clean
        let clean =
            check_src("fn f() -> int uses Store {\nstore.set(\"k\", 42)\nstore.get(\"k\", 0)\n}");
        assert_eq!(clean, vec![], "unexpected errors: {clean:?}");

        // Store is a known effect (not E0101)
        let errs3 = check_src("fn f() -> int uses Store { 1 }");
        assert!(
            !errs3.iter().any(|e| e.code == "E0101"),
            "Store should be a known effect, got {errs3:?}"
        );
    }

    #[test]
    fn fs_watch_requires_fswatch_effect() {
        // fs.watch without FsWatch effect trips E0301
        let errs1 = check_src("fn f() -> [FsEvent] { fs.watch(\"dir\", 100) }");
        assert!(
            errs1
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("FsWatch")),
            "expected E0301 for fs.watch, got {errs1:?}"
        );

        // fs.changed without FsWatch effect trips E0301
        let errs2 = check_src("fn f() -> bool { fs.changed(\"dir\", 100) }");
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("FsWatch")),
            "expected E0301 for fs.changed, got {errs2:?}"
        );

        // with uses FsWatch is clean
        let clean = check_src("fn f() -> [FsEvent] uses FsWatch { fs.watch(\"dir\", 100) }");
        assert_eq!(clean, vec![], "unexpected errors: {clean:?}");

        // FsWatch is a known effect
        let errs3 = check_src("fn f() -> int uses FsWatch { 1 }");
        assert!(
            !errs3.iter().any(|e| e.code == "E0101"),
            "FsWatch should be a known effect, got {errs3:?}"
        );
    }

    #[test]
    fn proc_requires_process_effect() {
        // proc.run without Process effect trips E0301
        let errs1 = check_src("fn f() -> ProcessOutput { proc.run(\"echo\", [\"hi\"], 1000) }");
        assert!(
            errs1
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Process")),
            "expected E0301 for proc.run, got {errs1:?}"
        );

        // proc.spawn without Process effect trips E0301
        let errs2 = check_src("fn f() -> string { proc.spawn(\"echo\", [\"hi\"], 1000) }");
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Process")),
            "expected E0301 for proc.spawn, got {errs2:?}"
        );

        // run builtin without Process effect trips E0301
        let errs_run = check_src("fn f() -> ProcessOutput { run(\"echo\", [\"hi\"]) }");
        assert!(
            errs_run
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Process")),
            "expected E0301 for run builtin, got {errs_run:?}"
        );

        // with uses Process is clean
        let clean = check_src(
            "fn f() -> ProcessOutput uses Process { proc.run(\"echo\", [\"hi\"], 1000) }",
        );
        assert_eq!(clean, vec![], "unexpected errors: {clean:?}");

        // Process is a known effect
        let errs3 = check_src("fn f() -> int uses Process { 1 }");
        assert!(
            !errs3.iter().any(|e| e.code == "E0101"),
            "Process should be a known effect, got {errs3:?}"
        );
    }

    #[test]
    fn tool_call_declared_is_clean() {
        let src = "type R = { t: string }\ntool web(q: string) -> [R] { impl: mcp(\"s\").web(q) }\nfn f(q: string) -> [R] uses Tool { web(q) }";
        assert_eq!(check_src(src), vec![]);
    }

    #[test]
    fn tool_capability_attenuation_and_reachable_tool_graph() {
        let base = r#"
            tool web(q: string) -> string { impl: mcp("search").web(q) }
            tool drop_db(t: string) -> string { impl: mcp("db").drop(t) }
        "#;

        // 1. Function declaring Tool(web) calling web(q) -> Clean
        let src_clean =
            format!("{base}\nfn search(q: string) -> string uses Tool(web) {{ web(q) }}");
        assert_eq!(check_src(&src_clean), vec![]);

        // 2. Function declaring Tool(web) calling drop_db(t) -> E0302 (annotation too narrow)
        let src_denied =
            format!("{base}\nfn bad(q: string) -> string uses Tool(web) {{ drop_db(q) }}");
        let errs_denied = check_src(&src_denied);
        assert!(
            errs_denied
                .iter()
                .any(|e| e.code == "E0302" && e.msg.contains("Tool(drop_db)")),
            "expected E0302 for Tool(drop_db), got: {errs_denied:?}"
        );

        // 3. Proves reachable-tool graph: safe function calls helper that reaches drop_db -> E0302
        let src_transitive = format!(
            "{base}\nfn helper(t: string) -> string uses Tool(drop_db) {{ drop_db(t) }}\nfn search_flow(q: string) -> string uses Tool(web) {{ helper(q) }}"
        );
        let errs_trans = check_src(&src_transitive);
        assert!(
            errs_trans.iter().any(|e| e.code == "E0302"
                && e.msg.contains("search_flow")
                && e.msg.contains("Tool(drop_db)")),
            "expected E0302 for transitive ungranted tool, got: {errs_trans:?}"
        );

        // 4. Undeclared tool call with no uses clause -> E0301 naming Tool(web)
        let src_no_uses = format!("{base}\nfn f(q: string) -> string {{ web(q) }}");
        let errs_no_uses = check_src(&src_no_uses);
        assert!(
            errs_no_uses
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("Tool(web)")),
            "expected E0301 naming Tool(web), got: {errs_no_uses:?}"
        );

        // 5. Server-scoped wildcard Tool("search/*") matches web tool on server "search"
        let src_server_glob =
            format!("{base}\nfn search(q: string) -> string uses Tool(\"search/*\") {{ web(q) }}");
        assert_eq!(check_src(&src_server_glob), vec![]);

        // 6. Broad Tool or Tool(*) allows any tool call
        let src_broad =
            format!("{base}\nfn f(q: string) -> string uses Tool(*) {{ web(q) + drop_db(q) }}");
        assert_eq!(check_src(&src_broad), vec![]);
    }

    #[test]
    fn agent_tool_capability_attenuation() {
        let base = r#"
            tool web(q: string) -> string { impl: mcp("search").web(q) }
            tool drop_db(t: string) -> string { impl: mcp("db").drop(t) }
        "#;

        // 1. Agent declaring tools [web] attenuates member fn with uses Tool: calling web is clean
        let src_agent_clean = format!(
            r#"
            {base}
            agent SafeAgent {{
                tools [web]
                state {{ round: int = 0 }}
                fn step(q: string) -> string uses Tool {{
                    web(q)
                }}
            }}
        "#
        );
        assert_eq!(check_src(&src_agent_clean), vec![]);

        // 2. Agent declaring tools [web] attenuates member fn: calling drop_db trips E0302!
        let src_agent_denied = format!(
            r#"
            {base}
            agent SafeAgent {{
                tools [web]
                state {{ round: int = 0 }}
                fn step(q: string) -> string uses Tool {{
                    drop_db(q)
                }}
            }}
        "#
        );
        let errs_agent_denied = check_src(&src_agent_denied);
        assert!(
            errs_agent_denied
                .iter()
                .any(|e| e.code == "E0302" && e.msg.contains("Tool(drop_db)")),
            "expected E0302 for agent tool attenuation violation, got: {errs_agent_denied:?}"
        );

        // 3. Agent declaring uses Tool(web) in header attenuates member fns
        let src_agent_header = format!(
            r#"
            {base}
            agent WebBot uses Tool(web) {{
                state {{ round: int = 0 }}
                fn run(q: string) -> string {{
                    web(q)
                }}
                fn dangerous(t: string) -> string {{
                    drop_db(t)
                }}
            }}
        "#
        );
        let errs_header = check_src(&src_agent_header);
        assert!(
            errs_header.iter().any(|e| e.code == "E0302"
                && e.msg.contains("dangerous")
                && e.msg.contains("Tool(drop_db)")),
            "expected E0302 for ungranted dangerous call in WebBot, got: {errs_header:?}"
        );
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

    #[test]
    fn pack1_record_literal_typecheck() {
        // Accept valid record matching type alias
        let src = r#"
            type User = { id: int, name: string }
            fn f() -> User {
                { id: 1, name: "Alice" }
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // E0203 on duplicate field
        let src_dup = r#"
            fn f() -> { a: int } {
                { a: 1, a: 2 }
            }
        "#;
        let errs = check_src(src_dup);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0203"
                    && e.msg.contains("duplicate field 'a' in record literal")),
            "got {errs:?}"
        );

        // E0201 on field type mismatch
        let src_mismatch = r#"
            type User = { id: int, name: string }
            fn f() -> User {
                { id: "wrong", name: "Alice" }
            }
        "#;
        let errs = check_src(src_mismatch);
        assert!(errs.iter().any(|e| e.code == "E0201"), "got {errs:?}");
    }

    #[test]
    fn pack1_indexing_typecheck() {
        // Accept valid list and record indexing
        let src = r#"
            type Item = { id: int, name: string }
            fn f(items: [Item], idx: int) -> string {
                let first = items[0]
                let dynamic_item = items[idx]
                let field_str = first["name"]
                field_str
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // E0201: list index must be integer
        let src_bad_list_idx = "fn f(xs: [int]) -> int { xs[\"bad\"] }";
        let errs = check_src(src_bad_list_idx);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("list index must be an integer")),
            "got {errs:?}"
        );

        // E0201: record index must be string
        let src_bad_rec_idx = "type R = { a: int }\nfn f(r: R) -> int { r[0] }";
        let errs = check_src(src_bad_rec_idx);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("record index must be a string")),
            "got {errs:?}"
        );

        // E0201: cannot index non-indexable type
        let src_non_indexable = "fn f(x: int) -> int { x[0] }";
        let errs = check_src(src_non_indexable);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("cannot index into type int")),
            "got {errs:?}"
        );

        // E0201: record has no field
        let src_no_field = "type R = { a: int }\nfn f(r: R) -> int { r[\"missing\"] }";
        let errs = check_src(src_no_field);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("record has no field 'missing'")),
            "got {errs:?}"
        );
    }

    #[test]
    fn pack1_if_else_typecheck() {
        // Accept valid if-else expression in value position
        let src = r#"
            fn f(score: int) -> string {
                let s = if score >= 80 { "pass" } else { "fail" }
                s
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // Accept if statement without else in statement position
        let src_stmt = r#"
            fn f(cond: bool) -> () {
                if cond {
                    "side_effect"
                }
                none
            }
        "#;
        assert_eq!(check_src(src_stmt), vec![]);

        // E0704: if expression used as value missing else
        let src_missing_else = r#"
            fn f(cond: bool) -> string {
                let s = if cond { "pass" }
                s
            }
        "#;
        let errs = check_src(src_missing_else);
        assert!(
            errs.iter().any(|e| e.code == "E0704"
                && e.msg
                    .contains("if expression used as a value must include an else branch")),
            "got {errs:?}"
        );

        // E0201: non-bool condition
        let src_bad_cond = r#"
            fn f(x: int) -> string {
                if x { "a" } else { "b" }
            }
        "#;
        let errs = check_src(src_bad_cond);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("if condition must be bool")),
            "got {errs:?}"
        );

        // E0201: branch type mismatch
        let src_mismatch = r#"
            fn f(c: bool) -> int {
                if c { 1 } else { "not an int" }
            }
        "#;
        let errs = check_src(src_mismatch);
        assert!(
            errs.iter().any(|e| e.code == "E0201"
                && e.msg
                    .contains("if and else branches have incompatible types")),
            "got {errs:?}"
        );
    }

    #[test]
    fn pack1_for_loop_typecheck() {
        // Accept valid for loops (with and without index)
        let src = r#"
            fn f(items: [string]) -> () {
                for item in items {
                    assert true
                }
                for (item, idx) in items {
                    assert idx >= 0
                }
                none
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // E0201: collection not a list
        let src_bad_coll = r#"
            fn f(x: int) -> () {
                for item in x {
                    assert true
                }
                none
            }
        "#;
        let errs = check_src(src_bad_coll);
        assert!(
            errs.iter().any(|e| e.code == "E0201" && e.msg.contains("for loop requires a list collection")),
            "got {errs:?}"
        );
    }

    #[test]
    fn pack1_effect_propagation() {
        // Tool called inside if branch: pure fn trips E0301
        let src_if_tool = r#"
            tool query_db(q: string) -> string {
                impl: mcp("db").query(q)
            }
            fn f(c: bool) -> string {
                if c {
                    query_db("test")
                } else {
                    "fallback"
                }
            }
        "#;
        let errs = check_src(src_if_tool);
        assert!(errs.iter().any(|e| e.code == "E0301"), "got {errs:?}");

        // Tool called inside for body: pure fn trips E0301
        let src_for_tool = r#"
            tool query_db(q: string) -> string {
                impl: mcp("db").query(q)
            }
            fn f(items: [string]) -> () {
                for it in items {
                    let _ = query_db(it)
                }
                none
            }
        "#;
        let errs = check_src(src_for_tool);
        assert!(errs.iter().any(|e| e.code == "E0301"), "got {errs:?}");

        // Tool called inside record literal: pure fn trips E0301
        let src_record_tool = r#"
            tool query_db(q: string) -> string {
                impl: mcp("db").query(q)
            }
            fn f() -> { res: string } {
                { res: query_db("q") }
            }
        "#;
        let errs = check_src(src_record_tool);
        assert!(errs.iter().any(|e| e.code == "E0301"), "got {errs:?}");

        // Declaring uses Tool satisfies the checker
        let src_declared = r#"
            tool query_db(q: string) -> string {
                impl: mcp("db").query(q)
            }
            fn f(c: bool) -> string uses Tool {
                if c {
                    query_db("test")
                } else {
                    "fallback"
                }
            }
        "#;
        assert_eq!(check_src(src_declared), vec![]);
    }

    #[test]
    fn mutable_locals_typecheck() {
        // 1. Assign to immutable local -> E0706
        let src = r#"
            fn main() -> int {
                let x = 1
                x = 2
                x
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter().any(|e| e.code == "E0706"
                && e.msg
                    .contains("cannot assign to immutable local 'x'; declare it with `let mut`")),
            "got {errs:?}"
        );

        // 2. Assign wrong type to mutable local -> E0201
        let src_mismatch = r#"
            fn main() -> int {
                let mut x = 1
                x = "hello"
                x
            }
        "#;
        let errs_mismatch = check_src(src_mismatch);
        assert!(
            errs_mismatch.iter().any(|e| e.code == "E0201"
                && e.msg
                    .contains("local 'x' has type int but assigned value is string")),
            "got {errs_mismatch:?}"
        );

        // 3. Assign to undeclared variable -> E0101
        let src_undef = r#"
            fn main() -> int {
                unknown_var = 42
                0
            }
        "#;
        let errs_undef = check_src(src_undef);
        assert!(
            errs_undef
                .iter()
                .any(|e| e.code == "E0101" && e.msg.contains("unknown_var")),
            "got {errs_undef:?}"
        );

        // 4. Valid let mut with reassignment -> clean
        let src_valid = r#"
            fn main() -> int {
                let mut x = 1
                x = 2
                x
            }
        "#;
        assert_eq!(check_src(src_valid), vec![]);

        // 5. For-loop accumulation with outer let mut -> clean
        let src_loop = r#"
            fn sum_list(xs: [int]) -> int {
                let mut acc = 0
                for x in xs {
                    acc = acc + x
                }
                acc
            }
        "#;
        assert_eq!(check_src(src_loop), vec![]);

        // 6. Assign to loop variable (immutable) -> E0706
        let src_loop_var = r#"
            fn main() -> int {
                let xs = [1, 2, 3]
                for x in xs {
                    x = 10
                }
                0
            }
        "#;
        let errs_loop_var = check_src(src_loop_var);
        assert!(
            errs_loop_var.iter().any(|e| e.code == "E0706"
                && e.msg
                    .contains("cannot assign to immutable local 'x'; declare it with `let mut`")),
            "got {errs_loop_var:?}"
        );
    }

    #[test]
    fn test_ui_acceptance_check_clean() {
        let src = r#"
            view CounterView(count: int) -> View {
                column [
                    text("Sayı: " + count),
                    button("Artır", id = "btn_inc", on_click = inc)
                ]
            }

            agent CounterApp {
                state {
                    val: int = 0
                }

                fn inc() uses UI {
                    state.val = state.val + 1
                }

                window Win {
                    title: "Test Penceresi",
                    content: CounterView(state.val)
                }
            }

            test "pencere acilisi, girdi tetiklemesi ve gorunum guncellemesi" {
                let app = CounterApp.Win.mount_headless()
                assert app.find("btn_inc").exists()
                assert app.find_text("Sayı: 0").exists()

                app.dispatch_click("btn_inc")

                assert app.find_text("Sayı: 1").exists()
                assert CounterApp.state.val == 1
            }
        "#;
        let errs = check_src(src);
        assert_eq!(errs, vec![]);
    }

    #[test]
    fn test_ui_missing_effect_trips_e1001() {
        // Handler without uses UI referenced in button trips E1001
        let src = r#"
            view CounterView(count: int) -> View {
                column [
                    button("Artır", id = "btn_inc", on_click = inc)
                ]
            }
            agent CounterApp {
                state {
                    val: int = 0
                }
                fn inc() {
                    state.val = state.val + 1
                }
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| (e.code == "E1001" || e.code == "E0301") && e.msg.contains("inc")),
            "expected E1001/E0301, got {errs:?}"
        );

        // Calling window.open() outside test without uses UI trips E1001/E0301
        let src2 = r#"
            window Win {
                title: "W",
                content: text("hello")
            }
            fn main() -> bool {
                Win.open()
            }
        "#;
        let errs2 = check_src(src2);
        assert!(
            errs2
                .iter()
                .any(|e| (e.code == "E1001" || e.code == "E0301") && e.msg.contains("UI")),
            "expected E1001/E0301, got {errs2:?}"
        );

        // Handler referenced in on_key without uses UI trips E1001
        let src3 = r#"
            view KeyView() -> View {
                column [
                    input("val", on_key = on_k)
                ]
            }
            fn on_k(k: string) {
            }
        "#;
        let errs3 = check_src(src3);
        assert!(
            errs3
                .iter()
                .any(|e| e.code == "E1001" && e.msg.contains("on_k")),
            "expected E1001 for on_key missing UI, got {errs3:?}"
        );

        // Handler referenced in on_key with non-string parameter trips E1003
        let src4 = r#"
            view KeyView() -> View {
                column [
                    input("val", on_key = on_k_bad)
                ]
            }
            fn on_k_bad(k: int) uses UI {
            }
        "#;
        let errs4 = check_src(src4);
        assert!(
            errs4
                .iter()
                .any(|e| e.code == "E1003" && e.msg.contains("on_k_bad")),
            "expected E1003 for on_key signature mismatch, got {errs4:?}"
        );
    }

    #[test]
    fn test_ui_duplicate_window_or_view_trips_e1004() {
        let src = r#"
            window Win {
                title: "1",
                content: text("1")
            }
            window Win {
                title: "2",
                content: text("2")
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| e.code == "E1004" && e.msg.contains("Win")),
            "expected E1004 for duplicate window, got {errs:?}"
        );

        let src_view = r#"
            view V() -> View {
                text("a")
            }
            view V() -> View {
                text("b")
            }
        "#;
        let errs_view = check_src(src_view);
        assert!(
            errs_view
                .iter()
                .any(|e| e.code == "E1004" && e.msg.contains("V")),
            "expected E1004 for duplicate view, got {errs_view:?}"
        );
    }

    #[test]
    fn test_ui_unknown_component_trips_e1002() {
        let src1 = r#"
            view MyView() -> View {
                stack [
                    text("hi")
                ]
            }
        "#;
        let errs1 = check_src(src1);
        assert!(
            errs1
                .iter()
                .any(|e| e.code == "E1002" && e.msg.contains("stack")),
            "expected E1002 for stack, got {errs1:?}"
        );

        let src2 = r#"
            view MyView2() -> View {
                ui.custom_widget("hi")
            }
        "#;
        let errs2 = check_src(src2);
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E1002" && e.msg.contains("custom_widget")),
            "expected E1002 for ui.custom_widget, got {errs2:?}"
        );
    }

    #[test]
    fn test_auth_syntax_and_checker_gating_missing_auth_effect_trips_e_auth_0403() {
        let src = r#"
            auth provider github_copilot {
                type: "device_code",
                issuer: "https://github.com/login/device/code",
                client_id: "Iv1.b507a08c87ecfe81",
                scope: "model:inference"
            }
            account dev_tier for github_copilot {
                scope: ["model:inference"]
            }
            fn query_engine(input: string) -> string
                uses LLM
            {
                llm"""Analyze: {input}""" with {}
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| (e.code == "E-auth-0403" || e.code == "E0301")
                    && (e.msg.contains("Auth") || e.msg.contains("dev_tier"))),
            "Expected E-auth-0403 or E0301 for missing Auth effect, got {errs:?}"
        );
    }

    #[test]
    fn test_auth_syntax_clean_with_uses_auth() {
        let src = r#"
            auth provider github_copilot {
                type: "device_code",
                issuer: "https://github.com/login/device/code",
                client_id: "Iv1.b507a08c87ecfe81",
                scope: "model:inference"
            }
            account dev_tier for github_copilot {
                scope: ["model:inference"]
            }
            fn query_engine(input: string) -> string
                uses LLM, Auth(dev_tier)
            {
                llm"""Analyze: {input}""" with {}
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.is_empty(),
            "Expected clean check with uses Auth(dev_tier), got {errs:?}"
        );
    }

    #[test]
    fn test_auth_pure_function_cannot_call_authenticated_op() {
        let src = r#"
            auth provider github_copilot {
                type: "device_code",
                issuer: "https://github.com/login/device/code",
                client_id: "Iv1.b507a08c87ecfe81",
                scope: "model:inference"
            }
            account dev_tier for github_copilot {
                scope: ["model:inference"]
            }
            fn query_engine(input: string) -> string
                uses LLM, Auth(dev_tier)
            {
                llm"""Analyze: {input}""" with {}
            }
            fn pure_caller() -> string {
                query_engine("test")
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| (e.code == "E-auth-0403" || e.code == "E0301")
                    && e.msg.contains("pure_caller")),
            "Expected pure_caller to be rejected for unannotated authenticated call, got {errs:?}"
        );
    }

    #[test]
    fn test_auth_scope_deficit_trips_e_auth_0403() {
        let src = r#"
            auth provider github_copilot {
                type: "device_code",
                issuer: "https://github.com/login/device/code",
                client_id: "Iv1.b507a08c87ecfe81",
                scope: "read:user"
            }
            account dev_tier for github_copilot {
                scope: ["read:user"]
            }
            fn query_engine(input: string) -> string
                uses LLM, Auth(dev_tier)
            {
                llm"""Analyze: {input}""" with {}
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter().any(|e| e.code == "E-auth-0403"
                && e.msg.contains("lacks required scope 'model:inference'")),
            "Expected E-auth-0403 for scope deficit, got {errs:?}"
        );
    }

    #[test]
    fn test_auth_provider_undeclared_in_account_trips_e_auth_0404() {
        let src = r#"
            auth provider anthropic {
                type: "api_key",
                env: "ANTHROPIC_API_KEY"
            }
            account dev_tier for github_copilot {
                scope: ["model:inference"]
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.iter()
                .any(|e| e.code == "E-auth-0404" && e.msg.contains("github_copilot")),
            "Expected E-auth-0404 for undeclared provider in account, got {errs:?}"
        );
    }

    #[test]
    fn valid_cost_bound_passes_check() {
        // analyze: 1 site, retry: 2 with repair -> max calls = 3 -> worst-case cost = 0.003 USD
        // bound is 0.005 USD, 0.003 <= 0.005 -> passes
        let src = r#"
            fn analyze() -> string uses LLM costs < 0.005 USD {
                llm"""prompt""" with { model: "m", retry: 2 with repair }
            }
        "#;
        let errs = check_src(src);
        assert_eq!(errs, vec![], "expected valid cost bound to pass: {errs:?}");

        // Also test without uses clause (pure fn or $0 cost fn)
        let src_pure = r#"
            fn add(a: int, b: int) -> int costs < 0.01 USD {
                a + b
            }
        "#;
        let errs_pure = check_src(src_pure);
        assert_eq!(
            errs_pure,
            vec![],
            "expected pure fn with cost bound to pass: {errs_pure:?}"
        );
    }

    #[test]
    fn exceeded_cost_bound_produces_e0502() {
        // analyze: 1 site, retry: 2 with repair -> max calls = 3 -> worst-case cost = 0.003 USD
        // bound is 0.002 USD -> 0.003 > 0.002 -> trips E0502
        let src = r#"
            fn analyze() -> string uses LLM costs < 0.002 USD {
                llm"""prompt""" with { model: "m", retry: 2 with repair }
            }
        "#;
        let errs = check_src(src);
        let e0502 = errs.iter().find(|e| e.code == "E0502");
        assert!(e0502.is_some(), "expected E0502, got: {errs:?}");
        let msg = &e0502.unwrap().msg;
        assert!(
            msg.contains("static cost bound exceeded: fn 'analyze' declared costs < 0.00 USD")
                || msg.contains(
                    "static cost bound exceeded: fn 'analyze' declared costs < 0.002 USD"
                )
                || msg.contains("static cost bound exceeded: fn 'analyze' declared costs <"),
            "unexpected message: {msg}"
        );
        assert!(msg.contains("worst-case is"), "unexpected message: {msg}");
    }

    #[test]
    fn transitive_callee_cost_causes_caller_to_exceed_bound() {
        // g() has worst case 0.003 USD
        // f() calls g() and declares costs < 0.002 USD -> f's transitive worst case is 0.003 USD -> exceeds 0.002 USD
        let src = r#"
            fn g() -> string uses LLM {
                llm"""prompt""" with { model: "m", retry: 2 with repair }
            }
            fn f() -> string uses LLM costs < 0.002 USD {
                g()
            }
        "#;
        let errs = check_src(src);
        let e0502 = errs.iter().find(|e| e.code == "E0502");
        assert!(
            e0502.is_some(),
            "expected E0502 on caller f due to callee g cost, got: {errs:?}"
        );
        let msg = &e0502.unwrap().msg;
        assert!(
            msg.contains("fn 'f' declared costs <"),
            "message was: {msg}"
        );
    }

    #[test]
    fn confidence_refinement_validation() {
        // Valid confidence bounds: 0.0, 1.0, and 0.85
        let errs = check_src("type C1 = float @confidence(0.85)\ntype C2 = float @confidence(1)\ntype C3 = float @confidence(0)");
        assert!(errs.is_empty(), "expected valid confidence, got {errs:?}");

        // Out-of-range confidence > 1.0
        let errs_high = check_src("type Bad = float @confidence(1.5)");
        assert!(
            errs_high
                .iter()
                .any(|e| e.code == "E0202" && e.msg.contains("@confidence")),
            "expected E0202 for confidence > 1.0, got {errs_high:?}"
        );

        // Out-of-range negative confidence
        let errs_neg = check_src("type Bad = float @confidence(-0.1)");
        assert!(
            errs_neg
                .iter()
                .any(|e| e.code == "E0202" && e.msg.contains("@confidence")),
            "expected E0202 for negative confidence, got {errs_neg:?}"
        );
    }

    #[test]
    fn confidence_subtyping_and_threshold_enforcement() {
        let valid_src = r#"
            type Plan = { title: string }
            fn deploy(p: Plan @confidence(0.85)) -> int { 1 }
            fn caller(high: Plan @confidence(0.90)) -> int {
                deploy(high)
            }
        "#;
        let errs = check_src(valid_src);
        assert!(
            errs.is_empty(),
            "expected subtyping high confidence -> lower threshold to succeed, got {errs:?}"
        );

        let invalid_src = r#"
            type Plan = { title: string }
            fn deploy(p: Plan @confidence(0.85)) -> int { 1 }
            fn caller(low: Plan @confidence(0.70)) -> int {
                deploy(low)
            }
        "#;
        let errs = check_src(invalid_src);
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201 when confidence is insufficient, got {errs:?}"
        );
    }

    #[test]
    fn verification_stages_subtyping_and_lattice() {
        let valid_src = r#"
            type Action = { cmd: string }
            fn sink_verified(a: Verified[Action]) -> int { 1 }
            fn sink_validated(a: Validated[Action]) -> int { 2 }
            fn sink_unverified(a: Unverified[Action]) -> int { 3 }

            fn caller(v: Verified[Action], val: Validated[Action]) -> int {
                sink_verified(v)
                sink_validated(v)
                sink_unverified(v)
                sink_validated(val)
                sink_unverified(val)
                42
            }
        "#;
        let errs = check_src(valid_src);
        assert!(
            errs.is_empty(),
            "expected covariant verification stage subtyping, got {errs:?}"
        );

        // Unverified passed to Verified sink -> E0201
        let err_unv_to_ver = check_src(
            r#"
            type Action = { cmd: string }
            fn execute(a: Verified[Action]) -> int { 1 }
            fn caller(raw: Unverified[Action]) -> int {
                execute(raw)
            }
        "#,
        );
        assert!(
            err_unv_to_ver.iter().any(|e| e.code == "E0201"),
            "expected E0201 for Unverified -> Verified, got {err_unv_to_ver:?}"
        );

        // Validated passed to Verified sink -> E0201
        let err_val_to_ver = check_src(
            r#"
            type Action = { cmd: string }
            fn execute(a: Verified[Action]) -> int { 1 }
            fn caller(val: Validated[Action]) -> int {
                execute(val)
            }
        "#,
        );
        assert!(
            err_val_to_ver.iter().any(|e| e.code == "E0201"),
            "expected E0201 for Validated -> Verified, got {err_val_to_ver:?}"
        );
    }

    #[test]
    fn verification_stage_record_field_access() {
        let src = r#"
            type Doc = { score: float, text: string }
            fn inspect(d: Validated[Doc]) -> float {
                d.score
            }
        "#;
        let errs = check_src(src);
        assert!(
            errs.is_empty(),
            "expected record field access on Validated[Doc] to typecheck, got {errs:?}"
        );
    }

    #[test]
    fn check_all_examples_typecheck() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let examples_dir = manifest_dir.join("../../examples");
        fn collect_ndg(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect_ndg(&path, out);
                    } else if path.extension().and_then(|s| s.to_str()) == Some("ndg") {
                        out.push(path);
                    }
                }
            }
        }
        let mut paths = Vec::new();
        collect_ndg(&examples_dir, &mut paths);
        assert!(
            !paths.is_empty(),
            "examples directory should contain .ndg files"
        );
        paths.sort();
        for path in paths {
            if path.to_string_lossy().contains("file-manager/src") {
                // Sub-modules of file_manager.ndg; full application is verified via file_manager.ndg
                continue;
            }
            let mut src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            if path.to_string_lossy().contains("browser-agent") {
                let html_src =
                    std::fs::read_to_string(manifest_dir.join("../../prelude/web/html.ndg"))
                        .unwrap();
                let dom_src =
                    std::fs::read_to_string(manifest_dir.join("../../prelude/web/dom.ndg"))
                        .unwrap();
                let snap_src =
                    std::fs::read_to_string(manifest_dir.join("../../prelude/web/snapshot.ndg"))
                        .unwrap();
                let drift_src =
                    std::fs::read_to_string(manifest_dir.join("../../prelude/web/drift.ndg"))
                        .unwrap();
                src = format!("{html_src}\n{dom_src}\n{snap_src}\n{drift_src}\n{src}");
            }
            let errs = check_src(&src);
            assert!(
                errs.is_empty(),
                "example '{}' failed typecheck: {:?}",
                path.display(),
                errs.iter().map(|e| &e.msg).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_union_and_optional_edge_cases_and_fail_closed() {
        // Union widening: int | string <: int | string | bool
        let errs = check_src(
            r#"
fn get_val() -> int | string { 42 }
fn f() -> int | string | bool {
    let x: int | string | bool = get_val()
    x
}
"#,
        );
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Union narrowing rejection: int | string | bool NOT <: int | string
        let errs = check_src(
            r#"
fn get_val() -> int | string | bool { 42 }
fn f() -> int | string {
    let x: int | string = get_val()
    x
}
"#,
        );
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201 on union narrowing, got {errs:?}"
        );

        // Assigning none to optional union (int | string)? passes
        let errs = check_src("fn f() -> (int | string)? { none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Assigning int or string to (int | string)? passes
        let errs = check_src("fn f() -> (int | string)? { 10 }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");
        let errs = check_src("fn f() -> (int | string)? { \"hello\" }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Assigning bool to (int | string)? fails E0201
        let errs = check_src("fn f() -> (int | string)? { true }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // Assigning none to non-optional union fails E0201
        let errs = check_src("fn f() -> int | string { none }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // Assigning none to non-optional int fails E0201
        let errs = check_src("fn f() -> int { none }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // Assigning none to int | none passes
        let errs = check_src("fn f() -> int | none { none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Assigning none to union with optional variant passes
        let errs = check_src("fn f() -> int | string? { none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");
        let errs = check_src("fn f() -> int? | string { none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Equality check with optional and none passes
        let errs = check_src("fn f(x: int?) -> bool { x == none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");
        let errs = check_src("fn f(x: string?) -> bool { x != none }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // Equality check non-optional int with none fails E0201
        let errs = check_src("fn f(x: int) -> bool { x == none }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // Arithmetic on optional int fails E0201 fail-closed
        let errs = check_src("fn f(x: int?) -> int { x + 1 }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // Arithmetic on union int | string fails E0201 fail-closed
        let errs = check_src("fn f(x: int | string) -> int { x + 1 }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // List subtyping: [int] <: [int?] passes
        let errs = check_src("fn f(xs: [int]) -> [int?] { xs }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // List subtyping: [int?] NOT <: [int] fails E0201
        let errs = check_src("fn f(xs: [int?]) -> [int] { xs }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );

        // List subtyping: [int] <: [int | string] passes
        let errs = check_src("fn f(xs: [int]) -> [int | string] { xs }");
        assert!(errs.is_empty(), "expected clean check, got {errs:?}");

        // List subtyping: [int | string] NOT <: [int] fails E0201
        let errs = check_src("fn f(xs: [int | string]) -> [int] { xs }");
        assert!(
            errs.iter().any(|e| e.code == "E0201"),
            "expected E0201, got {errs:?}"
        );
    }

    #[test]
    fn test_check_json_valid_and_invalid() {
        // Valid program
        let valid_src = "fn add(a: int, b: int) -> int { a + b }";
        let valid_items = parse(lex(valid_src).unwrap()).unwrap();
        let valid_json_str = check_json(&valid_items, valid_src);
        let valid_val: serde_json::Value = serde_json::from_str(&valid_json_str).unwrap();
        assert_eq!(valid_val["ok"], true);
        assert_eq!(valid_val["items_count"], 1);
        assert_eq!(valid_val["errors_count"], 0);
        assert!(valid_val["errors"].as_array().unwrap().is_empty());

        // Invalid program with span (type mismatch E0201)
        let invalid_src = "fn f() -> int\n    costs < 10 {\n    1\n}";
        let invalid_items = parse(lex(invalid_src).unwrap()).unwrap();
        let invalid_json_str = check_json(&invalid_items, invalid_src);
        let invalid_val: serde_json::Value = serde_json::from_str(&invalid_json_str).unwrap();
        assert_eq!(invalid_val["ok"], false);
        assert_eq!(invalid_val["items_count"], 1);
        assert_eq!(invalid_val["errors_count"], 1);
        let err = &invalid_val["errors"][0];
        assert_eq!(err["code"], "E0201");
        assert!(err["message"]
            .as_str()
            .unwrap()
            .contains("cost bound must be a money literal"));
        assert_eq!(err["line"], 2);
        assert_eq!(err["col"], 13);
        assert!(err["span"].is_object());

        // Invalid program with span: None (e.g. duplicate fn E0102)
        let dup_src = "fn dup() -> int { 1 }\nfn dup() -> int { 2 }";
        let dup_items = parse(lex(dup_src).unwrap()).unwrap();
        let dup_json_str = check_json(&dup_items, dup_src);
        let dup_val: serde_json::Value = serde_json::from_str(&dup_json_str).unwrap();
        assert_eq!(dup_val["ok"], false);
        assert_eq!(dup_val["items_count"], 2);
        assert_eq!(dup_val["errors_count"], 1);
        let dup_err = &dup_val["errors"][0];
        assert_eq!(dup_err["code"], "E0102");
        assert!(dup_err["line"].is_null());
        assert!(dup_err["col"].is_null());
        assert!(dup_err["span"].is_null());
    }

    #[test]
    fn test_math_and_str_stdlib_typechecking() {
        // math stdlib type resolution
        let math_src = r#"
            fn f(x: float) -> float {
                let s = math.sqrt(x)
                let c = math.cos(s)
                let p = math.pow(s, 2.0)
                p
            }
        "#;
        assert_eq!(check_src(math_src), vec![]);

        // str stdlib type resolution
        let str_src = r#"
            fn g(s: string) -> int {
                let n = str.length(s)
                let upper = str.upper(s)
                let idx = str.index_of(upper, "TEST")
                n + idx
            }
        "#;
        assert_eq!(check_src(str_src), vec![]);

        // arity check: math.abs takes 1 argument
        let bad_math = "fn bad() -> float { math.abs() }";
        let errs = check_src(bad_math);
        assert!(errs
            .iter()
            .any(|e| e.code == "E0201" && e.msg.contains("math.abs")));
    }

    /// A boolean control hands its handler a Bool, so a handler declaring
    /// `v: string` would be handed something the type could not hold. Caught
    /// at check time rather than at the first click, which is the whole point
    /// of checking.
    #[test]
    fn test_a_flags_change_handler_must_take_a_bool() {
        let good = check_src(
            r#"
            fn on_dark(v: bool) uses UI {
            }

            view GoodView() -> View {
                column [
                    checkbox("Dark mode", id = "dark", on_change = on_dark)
                ]
            }
        "#,
        );
        assert!(
            good.is_empty(),
            "a bool handler is exactly right, got {good:?}"
        );

        let bad = check_src(
            r#"
            fn on_dark(v: string) uses UI {
            }

            view BadView() -> View {
                column [
                    toggle("Dark mode", id = "dark", on_change = on_dark)
                ]
            }
        "#,
        );
        assert!(
            bad.iter()
                .any(|e| e.code == "E1003" && e.msg.contains("must be of type bool")),
            "expected E1003 requiring bool, got {bad:?}"
        );

        let no_effect = check_src(
            r#"
            fn on_dark(v: bool) {
            }

            view NoEffectView() -> View {
                column [
                    checkbox("Dark mode", id = "dark", on_change = on_dark)
                ]
            }
        "#,
        );
        assert!(
            no_effect
                .iter()
                .any(|e| e.code == "E1001" && e.msg.contains("on_dark")),
            "an event handler still needs `uses UI`, got {no_effect:?}"
        );
    }

    /// A slider hands its handler a *number*, so a handler declaring a
    /// string would be handed something the type cannot hold. Caught at check
    /// time rather than at the first drag, when the value is already gone.
    #[test]
    fn test_a_sliders_change_handler_must_take_a_number() {
        // Both number spellings are right: a whole-numbered step arrives as
        // an Int, a fractional one as a Float, and a handler may declare
        // either — the runtime picks by the step.
        for decl in ["int", "float"] {
            let good = check_src(&format!(
                r#"
            fn on_volume(v: {decl}) uses UI {{
            }}

            view GoodView() -> View {{
                column [
                    slider("Volume", id = "volume", on_change = on_volume)
                ]
            }}
        "#
            ));
            assert!(
                good.is_empty(),
                "a {decl} handler is exactly right, got {good:?}"
            );
        }

        let bad = check_src(
            r#"
            fn on_volume(v: string) uses UI {
            }

            view BadView() -> View {
                column [
                    slider("Volume", id = "volume", on_change = on_volume)
                ]
            }
        "#,
        );
        assert!(
            bad.iter()
                .any(|e| e.code == "E1003" && e.msg.contains("must be of type int or float")),
            "expected E1003 requiring a number, got {bad:?}"
        );

        // A handler with nothing declared still passes: the type is inferred
        // at the call, and refusing it would break every handler that never
        // wrote a signature down.
        let bare = check_src(
            r#"
            fn on_volume() uses UI {
            }

            view BareView() -> View {
                column [
                    slider("Volume", id = "volume", on_change = on_volume)
                ]
            }
        "#,
        );
        assert!(
            bare.is_empty(),
            "a parameterless handler is fine, it just ignores the value: {bare:?}"
        );

        // As with every other control, an event handler still needs the UI
        // effect it is reaching into.
        let no_effect = check_src(
            r#"
            fn on_volume(v: int) {
            }

            view NoEffectView() -> View {
                column [
                    slider("Volume", id = "volume", on_change = on_volume)
                ]
            }
        "#,
        );
        assert!(
            no_effect
                .iter()
                .any(|e| e.code == "E1001" && e.msg.contains("on_volume")),
            "an event handler still needs `uses UI`, got {no_effect:?}"
        );

        // Two parameters is one too many: the slider has only the value to
        // give, and a handler that would need a second is a handler about a
        // different control.
        let too_many = check_src(
            r#"
            fn on_volume(v: int, w: int) uses UI {
            }

            view WideView() -> View {
                column [
                    slider("Volume", id = "volume", on_change = on_volume)
                ]
            }
        "#,
        );
        assert!(
            too_many
                .iter()
                .any(|e| e.code == "E1003" && e.msg.contains("at most 1 parameter")),
            "expected E1003 for the arity, got {too_many:?}"
        );
    }

    /// `slider` is a first-class element name: it typechecks as a View and is
    /// accepted wherever the other controls are.
    #[test]
    fn test_slider_is_an_element_wherever_elements_are() {
        let ok = check_src(
            r#"
            fn on_volume(v: int) uses UI {
            }

            fn quiet() -> bool {
                false
            }

            view DialsView() -> View {
                column [
                    slider("Volume", value = 20, min = 0, max = 100, step = 1, id = "volume", on_change = on_volume, disabled = false, tooltip = "Louder"),
                    text("20")
                ]
            }
        "#,
        );
        assert!(
            ok.is_empty(),
            "every documented property is accepted: {ok:?}"
        );
    }

    #[test]
    fn mem_store_requires_memstore_effect() {
        // mem.get without MemStore effect trips E0301
        let errs1 = check_src("fn f() -> int { mem.get(\"k\", 0) }");
        assert!(
            errs1
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("MemStore")),
            "expected E0301 for mem.get, got {errs1:?}"
        );

        // mem.set without MemStore effect trips E0301
        let errs2 = check_src("fn f() -> () { mem.set(\"k\", 42) }");
        assert!(
            errs2
                .iter()
                .any(|e| e.code == "E0301" && e.msg.contains("MemStore")),
            "expected E0301 for mem.set, got {errs2:?}"
        );

        // mem with uses MemStore is clean
        let clean =
            check_src("fn f() -> int uses MemStore {\nmem.set(\"k\", 42)\nmem.get(\"k\", 0)\n}");
        assert_eq!(clean, vec![], "unexpected errors: {clean:?}");

        // MemStore is a known effect (not E0101)
        let errs3 = check_src("fn f() -> int uses MemStore { 1 }");
        assert!(
            !errs3.iter().any(|e| e.code == "E0101"),
            "MemStore should be a known effect, got {errs3:?}"
        );
    }

    #[test]
    fn test_map_typecheck_and_indexing() {
        let src = r#"
            fn test_map(m: Map[string, int]) -> int {
                let v = m["key"]
                v
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // Key type mismatch
        let src_bad_key = r#"
            fn test_map(m: Map[string, int]) -> int {
                m[42]
            }
        "#;
        let errs = check_src(src_bad_key);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("map index must be of type string")),
            "got {errs:?}"
        );
    }

    #[test]
    fn test_index_assign_typecheck() {
        let src = r#"
            fn f() -> [int] {
                let mut xs = [1, 2, 3]
                xs[0] = 10
                xs
            }
        "#;
        assert_eq!(check_src(src), vec![]);

        // Index assign to immutable local trips E0706
        let src_immut = r#"
            fn f() -> [int] {
                let xs = [1, 2, 3]
                xs[0] = 10
                xs
            }
        "#;
        let errs = check_src(src_immut);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0706" && e.msg.contains("cannot mutate immutable local 'xs'")),
            "got {errs:?}"
        );

        // List index not int trips E0201
        let src_bad_idx = r#"
            fn f() -> [int] {
                let mut xs = [1, 2, 3]
                xs["zero"] = 10
                xs
            }
        "#;
        let errs = check_src(src_bad_idx);
        assert!(
            errs.iter()
                .any(|e| e.code == "E0201" && e.msg.contains("list index must be an integer")),
            "got {errs:?}"
        );

        // Value type mismatch trips E0201
        let src_bad_val = r#"
            fn f() -> [int] {
                let mut xs = [1, 2, 3]
                xs[0] = "string"
                xs
            }
        "#;
        let errs = check_src(src_bad_val);
        assert!(
            errs.iter().any(|e| e.code == "E0201"
                && e.msg.contains("cannot assign value of type string to list")),
            "got {errs:?}"
        );
    }

    #[test]
    fn test_workspace_stat_and_str_primitives_typecheck() {
        let src = r#"
            fn check_stat(p: string) -> bool uses Workspace {
                let st = workspace.stat(p)
                let c = str.char_code(p, 0)
                let h = str.hash(p)
                st.size >= 0 and st.mtime_ms >= 0 and c >= 0 and h != 0
            }
        "#;
        assert_eq!(check_src(src), vec![]);
    }
}
