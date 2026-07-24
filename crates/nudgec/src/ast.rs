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

    /// `l | merge r` (design §7): CRDT-style join used by state reducer
    /// writes — dicts union (right wins), lists append-dedup.
    Merge { l: Box<Expr>, r: Box<Expr> },

    /// `route{ cheap: "m1" when cond, strong: "m2" otherwise }` (design §4.4):
    /// arms evaluated in order; the first true `when` wins, `otherwise` is
    /// the fallback arm (cond = None). The model option value is a string.
    Route { arms: Vec<(String, String, Option<Expr>)> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `stream` is true for `stream let` (design §4.5): the bound LLM call
    /// is consumed incrementally; codegen lowers it to `rt.llm_stream`.
    Let { name: String, ty: Option<TypeExpr>, value: Expr, stream: bool },
    /// `state.x = v` / `state.x += v` inside an `agent` block (design §7):
    /// every write is an automatic checkpoint. `aug` is true for `+=`.
    StateWrite { field: String, aug: bool, value: Expr },
    Assert(Expr),
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    TypeAlias { name: String, ty: TypeExpr },
    Fn { name: String, params: Vec<Param>, ret: TypeExpr, effects: Vec<String>, body: Vec<Stmt> },
    Tool { name: String, params: Vec<Param>, ret: TypeExpr, fields: Vec<(String, Expr)> },
    Test { name: String, body: Vec<Stmt> },
    /// `agent Name { state { field: Ty = default, ... } fn ... }` (design §7).
    /// State fields carry their declared type and default value; `fns` are
    /// plain `Item::Fn`s whose bodies may read `state.x` and use StateWrite.
    Agent { name: String, state: Vec<(String, TypeExpr, Expr)>, fns: Vec<Item> },
}
