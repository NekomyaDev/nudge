//! Nudge AST — produced by the parser (design doc §10 pipeline).
//! Spans are dropped at this layer for now; diagnostics re-attach them
//! when the type checker lands (TODO: spanned AST in day 4–6).

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Named(String),                                  // string, Url, Plan
    List(Box<TypeExpr>),                            // [T]
    Record(Vec<(String, TypeExpr)>),                // {k: T, ...}
    Refine(Box<TypeExpr>, String, Vec<Expr>),       // float @range(0, 1)
    // TODO: Optional (T?) and union (T | U) — lexer/parser support lands with the type checker
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
    Not,                        // unary only
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Money(f64, String),
    Bool(bool),
    None,
    Ident(String),
    ListLit(Vec<Expr>),

    Prompt { body: String, interpolations: Vec<String> },

    /// llm"""...""" with { ... } — options are the with-block fields;
    /// `repair` is true when `retry: N with repair` was written.
    LlmCall { prompt: Box<Expr>, options: Vec<(String, Expr)>, repair: bool },

    Call { func: Box<Expr>, args: Vec<Expr>, kwargs: Vec<(String, Expr)> },
    Field { obj: Box<Expr>, name: String },
    Binary { op: BinOp, l: Box<Expr>, r: Box<Expr> },
    Unary { op: BinOp, x: Box<Expr> },              // op is Sub (negate) or Not

    ParMap { coll: Box<Expr>, kwargs: Vec<(String, Expr)>, params: Vec<String>, body: Box<Expr> },
    ParAll(Vec<Expr>),
    ParRace(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let { name: String, ty: Option<TypeExpr>, value: Expr },
    Assert(Expr),
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    TypeAlias { name: String, ty: TypeExpr },
    Fn { name: String, params: Vec<Param>, ret: TypeExpr, effects: Vec<String>, body: Vec<Stmt> },
    Tool { name: String, params: Vec<Param>, ret: TypeExpr, fields: Vec<(String, Expr)> },
    Test { name: String, body: Vec<Stmt> },
}
