//! Nudge AST — produced by the parser (design doc §10 pipeline).
//! Statements carry source spans (spanned AST stage 1); diagnostics and the
//! LSP attach them for precise locations. Expression-level spans land in
//! stage 2.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Half-open byte span `[start, end)` into the source file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    Named(String),                   // string, Url, Plan
    List(Box<TypeExpr>),             // [T]
    Record(Vec<(String, TypeExpr)>), // {k: T, ...}
    Refine(Box<TypeExpr>, String, Vec<Expr>), // float @range(0, 1)
                                     // TODO: Optional (T?) and union (T | U) — lexer/parser support lands with the type checker
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpr::Named(n) => write!(f, "{n}"),
            TypeExpr::List(inner) => write!(f, "[{inner}]"),
            TypeExpr::Record(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            TypeExpr::Refine(inner, name, _) => write!(f, "{inner} @{name}(…)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Not, // unary only
}

/// One question inside a `decide { ... }` block (v1.4 "Decision"):
/// a choice over named options, a yes/no probability (noul), or an
/// ordinal rubric score — the JEV-family question kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecisionQ {
    Choice {
        prompt: String,
        options: Vec<String>,
    },
    Noul {
        prompt: String,
    },
    Score {
        prompt: String,
        levels: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Money(f64, String),
    Bool(bool),
    None,
    Ident(String),
    ListLit(Vec<Expr>),

    Prompt {
        body: String,
        interpolations: Vec<String>,
    },

    /// llm"""...""" with { ... } — options are the with-block fields;
    /// `repair` is true when `retry: N with repair` was written.
    LlmCall {
        prompt: Box<Expr>,
        options: Vec<(String, Expr)>,
        repair: bool,
    },

    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        kwargs: Vec<(String, Expr)>,
    },
    Field {
        obj: Box<Expr>,
        name: String,
    },
    Binary {
        op: BinOp,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    Unary {
        op: BinOp,
        x: Box<Expr>,
    }, // op is Sub (negate) or Not

    ParMap {
        coll: Box<Expr>,
        kwargs: Vec<(String, Expr)>,
        params: Vec<String>,
        body: Box<Expr>,
    },
    ParAll(Vec<Expr>),
    ParRace(Vec<Expr>),

    /// `l | merge r` (design §7): CRDT-style join used by state reducer
    /// writes — dicts union (right wins), lists append-dedup.
    Merge {
        l: Box<Expr>,
        r: Box<Expr>,
    },

    /// `route{ cheap: "m1" when cond, strong: "m2" otherwise }` (design §4.4):
    /// arms evaluated in order; the first true `when` wins, `otherwise` is
    /// the fallback arm (cond = None). A bare string arm value keeps the
    /// model-routing semantics (v1.4 generalization: any expression —
    /// value-level policy switch, e.g. on decision confidence).
    Route {
        arms: Vec<(String, Expr, Option<Expr>)>,
    },

    /// `decide { q: "..." choose [..] / noul / score [..], ... } on <state>
    /// with { model, deadline, null_option }` (v1.4) — one batched call
    /// against a JEV-family decision model. Evaluates to a record with one
    /// field per question (choice: winner/p/distribution/confidence,
    /// noul: p, score: score/distribution).
    DecideCall {
        questions: Vec<(String, DecisionQ)>,
        state: Box<Expr>,
        options: Vec<(String, Expr)>,
    },

    /// `computer.observe(app, ...)` / `computer.click(target, ...)` (v1.5):
    /// the native computer-use surface. The parser only produces this when
    /// `computer` is directly followed by `.` + a known method + `(` — a
    /// bare `computer` stays an ordinary identifier (design §12 keyword
    /// policy). Every call carries the `Computer` effect.
    ComputerCall {
        method: String,
        args: Vec<Expr>,
        kwargs: Vec<(String, Expr)>,
    },
}

/// An expression with its source span attached (spanned AST, stage 2).
/// Composite nodes derive their span from their children; leaves use their
/// token's span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StateOp {
    Set, // state.x = v
    Add, // state.x += v
    Sub, // state.x -= v
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)] // `ExprStmt` reads better than `Expr` here
pub enum StmtKind {
    /// `stream` is true for `stream let` (design §4.5): the bound LLM call
    /// is consumed incrementally; codegen lowers it to `rt.llm_stream`.
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
        stream: bool,
    },
    /// `state.x = v` / `+=` / `-=` inside an `agent` block (design §7):
    /// every write is an automatic checkpoint.
    StateWrite {
        field: String,
        op: StateOp,
        value: Expr,
    },
    Assert(Expr),
    ExprStmt(Expr),
    /// `for_all x in gen.int(0, 10) { ... }` (design §6.4, v1.3) — a
    /// property-based test case loop; only valid inside test blocks.
    ForAll {
        var: String,
        gen: String,
        args: Vec<Expr>,
        body: Vec<Stmt>,
    },
}

/// A statement with its source span attached (spanned AST, stage 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    TypeAlias {
        name: String,
        ty: TypeExpr,
    },
    Fn {
        name: String,
        params: Vec<Param>,
        ret: TypeExpr,
        effects: Vec<String>,
        body: Vec<Stmt>,
    },
    Tool {
        name: String,
        params: Vec<Param>,
        ret: TypeExpr,
        fields: Vec<(String, Expr)>,
    },
    Test {
        name: String,
        body: Vec<Stmt>,
    },
    /// `agent Name { state { field: Ty = default, ... } fn ... }` (design §7).
    /// State fields carry their declared type and default value; `fns` are
    /// plain `Item::Fn`s whose bodies may read `state.x` and use StateWrite.
    Agent {
        name: String,
        state: Vec<(String, TypeExpr, Expr)>,
        fns: Vec<Item>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_serde_round_trip() {
        let dummy_span = Span { start: 0, end: 10 };
        let items: Vec<Item> = vec![
            Item::TypeAlias {
                name: "Url".to_string(),
                ty: TypeExpr::Named("string".to_string()),
            },
            Item::Tool {
                name: "search".to_string(),
                params: vec![Param {
                    name: "query".to_string(),
                    ty: TypeExpr::Named("string".to_string()),
                }],
                ret: TypeExpr::Named("string".to_string()),
                fields: vec![(
                    "impl".to_string(),
                    Expr {
                        kind: ExprKind::Call {
                            func: Box::new(Expr {
                                kind: ExprKind::Ident("mcp".to_string()),
                                span: dummy_span,
                            }),
                            args: vec![Expr {
                                kind: ExprKind::Str("brave_search".to_string()),
                                span: dummy_span,
                            }],
                            kwargs: vec![(
                                "tool".to_string(),
                                Expr {
                                    kind: ExprKind::Str("search".to_string()),
                                    span: dummy_span,
                                },
                            )],
                        },
                        span: dummy_span,
                    },
                )],
            },
            Item::Fn {
                name: "run_agent".to_string(),
                params: vec![Param {
                    name: "prompt_text".to_string(),
                    ty: TypeExpr::Named("string".to_string()),
                }],
                ret: TypeExpr::Named("string".to_string()),
                effects: vec![
                    "IO".to_string(),
                    "Decision".to_string(),
                    "Computer".to_string(),
                ],
                body: vec![
                    Stmt {
                        kind: StmtKind::Let {
                            name: "streamed_chunk".to_string(),
                            ty: Some(TypeExpr::Named("string".to_string())),
                            value: Expr {
                                kind: ExprKind::LlmCall {
                                    prompt: Box::new(Expr {
                                        kind: ExprKind::Prompt {
                                            body: "Hello {prompt_text}".to_string(),
                                            interpolations: vec!["prompt_text".to_string()],
                                        },
                                        span: dummy_span,
                                    }),
                                    options: vec![(
                                        "model".to_string(),
                                        Expr {
                                            kind: ExprKind::Str("fast".to_string()),
                                            span: dummy_span,
                                        },
                                    )],
                                    repair: true,
                                },
                                span: dummy_span,
                            },
                            stream: true,
                        },
                        span: dummy_span,
                    },
                    Stmt {
                        kind: StmtKind::ExprStmt(Expr {
                            kind: ExprKind::DecideCall {
                                questions: vec![
                                    (
                                        "q1".to_string(),
                                        DecisionQ::Choice {
                                            prompt: "choose one".to_string(),
                                            options: vec!["a".to_string(), "b".to_string()],
                                        },
                                    ),
                                    (
                                        "q2".to_string(),
                                        DecisionQ::Noul {
                                            prompt: "yes or no?".to_string(),
                                        },
                                    ),
                                    (
                                        "q3".to_string(),
                                        DecisionQ::Score {
                                            prompt: "rate quality".to_string(),
                                            levels: vec![
                                                "low".to_string(),
                                                "med".to_string(),
                                                "high".to_string(),
                                            ],
                                        },
                                    ),
                                ],
                                state: Box::new(Expr {
                                    kind: ExprKind::Ident("curr_state".to_string()),
                                    span: dummy_span,
                                }),
                                options: vec![],
                            },
                            span: dummy_span,
                        }),
                        span: dummy_span,
                    },
                    Stmt {
                        kind: StmtKind::ExprStmt(Expr {
                            kind: ExprKind::ComputerCall {
                                method: "click".to_string(),
                                args: vec![Expr {
                                    kind: ExprKind::Str("submit_button".to_string()),
                                    span: dummy_span,
                                }],
                                kwargs: vec![],
                            },
                            span: dummy_span,
                        }),
                        span: dummy_span,
                    },
                    Stmt {
                        kind: StmtKind::ExprStmt(Expr {
                            kind: ExprKind::Route {
                                arms: vec![
                                    (
                                        "cheap".to_string(),
                                        Expr {
                                            kind: ExprKind::Str("gpt-4o-mini".to_string()),
                                            span: dummy_span,
                                        },
                                        Some(Expr {
                                            kind: ExprKind::Bool(true),
                                            span: dummy_span,
                                        }),
                                    ),
                                    (
                                        "fallback".to_string(),
                                        Expr {
                                            kind: ExprKind::Str("gpt-4o".to_string()),
                                            span: dummy_span,
                                        },
                                        None,
                                    ),
                                ],
                            },
                            span: dummy_span,
                        }),
                        span: dummy_span,
                    },
                ],
            },
            Item::Agent {
                name: "Worker".to_string(),
                state: vec![(
                    "count".to_string(),
                    TypeExpr::Named("int".to_string()),
                    Expr {
                        kind: ExprKind::Int(0),
                        span: dummy_span,
                    },
                )],
                fns: vec![],
            },
        ];

        let serialized = serde_json::to_string_pretty(&items).expect("Serialization failed");
        let deserialized: Vec<Item> =
            serde_json::from_str(&serialized).expect("Deserialization failed");

        assert_eq!(items, deserialized);
    }
}
