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
    Named(String),                            // string, Url, Plan
    List(Box<TypeExpr>),                      // [T]
    Record(Vec<(String, TypeExpr)>),          // {k: T, ...}
    Refine(Box<TypeExpr>, String, Vec<Expr>), // float @range(0, 1)
    Generic(String, Vec<TypeExpr>),           // Task[T]
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
            TypeExpr::Generic(name, args) => {
                write!(
                    f,
                    "{name}[{}]",
                    args.iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
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

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Eq => write!(f, "=="),
            BinOp::NotEq => write!(f, "!="),
            BinOp::Lt => write!(f, "<"),
            BinOp::LtEq => write!(f, "<="),
            BinOp::Gt => write!(f, ">"),
            BinOp::GtEq => write!(f, ">="),
            BinOp::And => write!(f, "and"),
            BinOp::Or => write!(f, "or"),
            BinOp::Not => write!(f, "not"),
        }
    }
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

    /// `{ field: expr, ... }` (Pack 1 control-flow & data) — record literal.
    RecordLit(Vec<(String, Expr)>),

    /// `obj[idx]` (Pack 1 control-flow & data) — indexing into list or record.
    Index {
        obj: Box<Expr>,
        idx: Box<Expr>,
    },

    /// `if cond { then } else { else_ }` (Pack 1 control-flow & data).
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Option<Box<Expr>>,
    },

    /// `spawn <expr-or-block>` (Pack 4 task lifecycle)
    Spawn(Box<Expr>),

    /// `{ stmt; ...; expr }` block expression
    Block(Vec<Stmt>),
}

/// An expression with its source span attached (spanned AST, stage 2).
/// Composite nodes derive their span from their children; leaves use their
/// token's span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl fmt::Display for ExprKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprKind::Int(v) => write!(f, "{v}"),
            ExprKind::Float(v) => write!(f, "{v}"),
            ExprKind::Str(s) => write!(f, "\"{s}\""),
            ExprKind::Money(v, unit) => write!(f, "{v} {unit}"),
            ExprKind::Bool(b) => write!(f, "{b}"),
            ExprKind::None => write!(f, "none"),
            ExprKind::Ident(name) => write!(f, "{name}"),
            ExprKind::ListLit(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            ExprKind::Prompt { body, .. } => write!(f, "\"\"\"{body}\"\"\""),
            ExprKind::LlmCall { prompt, .. } => write!(f, "llm({prompt})"),
            ExprKind::Call { func, args, kwargs } => {
                write!(f, "{func}(")?;
                let mut first = true;
                for a in args {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                    first = false;
                }
                for (k, v) in kwargs {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                    first = false;
                }
                write!(f, ")")
            }
            ExprKind::Field { obj, name } => write!(f, "{obj}.{name}"),
            ExprKind::Binary { op, l, r } => write!(f, "{l} {op} {r}"),
            ExprKind::Unary { op, x } => match op {
                BinOp::Sub => write!(f, "-{x}"),
                BinOp::Not => write!(f, "not {x}"),
                _ => write!(f, "{op} {x}"),
            },
            ExprKind::ParMap { coll, body, .. } => write!(f, "par map ({coll}) -> {body}"),
            ExprKind::ParAll(items) => {
                write!(f, "par all [")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            ExprKind::ParRace(items) => {
                write!(f, "par race [")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            ExprKind::Merge { l, r } => write!(f, "{l} | merge {r}"),
            ExprKind::Route { .. } => write!(f, "route {{ ... }}"),
            ExprKind::DecideCall { .. } => write!(f, "decide {{ ... }}"),
            ExprKind::ComputerCall { method, args, .. } => {
                write!(f, "computer.{method}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
            ExprKind::RecordLit(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            ExprKind::Index { obj, idx } => write!(f, "{obj}[{idx}]"),
            ExprKind::If { cond, then, else_ } => {
                write!(f, "if {cond} {{ {then} }}")?;
                if let Some(el) = else_ {
                    write!(f, " else {{ {el} }}")?;
                }
                Ok(())
            }
            ExprKind::Spawn(body) => write!(f, "spawn {body}"),
            ExprKind::Block(_) => write!(f, "{{ ... }}"),
        }
    }
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
        #[serde(default)]
        mutable: bool,
    },
    /// Reassignment of a local variable: `name = value`.
    Assign {
        name: String,
        value: Expr,
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
    /// `for item in coll { body }` or `for (item, idx) in coll { body }`
    /// (Pack 1 control-flow & data) — sequential collection loop.
    For {
        var: String,
        idx_var: Option<String>,
        coll: Expr,
        body: Vec<Stmt>,
    },
    /// `if cond { then_body } [else { else_body }]` (Pack 1 control-flow & data)
    /// Statement-level conditional execution with statement lists in branches.
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
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
    /// `view Name(params...) -> View { ... }` (Pack 3 Local UI).
    View {
        name: String,
        params: Vec<Param>,
        ret: TypeExpr,
        body: Vec<Stmt>,
    },
    /// `window Name { title: "...", content: ... }` (Pack 3 Local UI).
    Window {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// `auth provider <name> { ... }` (Pack 5 Provider Auth).
    AuthProvider {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// `account <name> for <provider> { ... }` (Pack 5 Provider Auth).
    Account {
        name: String,
        provider: String,
        fields: Vec<(String, Expr)>,
    },
}

/// Magic footer constant identifying a standalone Nudge agent binary.
pub const STANDALONE_MAGIC: &[u8; 8] = b"NDGAGNT1";

/// Length of the footer: 8 bytes payload length (u64 LE) + 8 bytes STANDALONE_MAGIC.
pub const STANDALONE_FOOTER_LEN: u64 = 16;

/// Standalone agent payload bundle containing the entry point and compiled AST items.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StandaloneBundle {
    pub entry: String,
    pub items: Vec<Item>,
}

impl StandaloneBundle {
    /// Serializes the bundle to binary bytes with bincode and encodes the append payload:
    /// `[bundle bytes][bundle_len: u64 LE][STANDALONE_MAGIC]`
    pub fn encode_payload(&self) -> Result<Vec<u8>, bincode::Error> {
        let bytes = bincode::serialize(self)?;
        let len = bytes.len() as u64;
        let mut payload = Vec::with_capacity(bytes.len() + STANDALONE_FOOTER_LEN as usize);
        payload.extend_from_slice(&bytes);
        payload.extend_from_slice(&len.to_le_bytes());
        payload.extend_from_slice(STANDALONE_MAGIC);
        Ok(payload)
    }

    /// Appends the encoded bundle payload to a file.
    pub fn append_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        let payload = self
            .encode_payload()
            .map_err(|e| format!("failed to serialize standalone bundle: {e}"))?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| format!("cannot open {} for appending payload: {e}", path.display()))?;
        file.write_all(&payload)
            .map_err(|e| format!("failed to append payload to {}: {e}", path.display()))?;
        Ok(())
    }

    /// Extracts an embedded `StandaloneBundle` from the end of a file.
    ///
    /// Returns:
    /// - `Ok(Some(bundle))` if a valid embedded bundle with magic tag is present.
    /// - `Ok(None)` if the file is smaller than 16 bytes or does not end with `NDGAGNT1`.
    /// - `Err(msg)` if the footer indicates a bundle is present but reading or deserialization fails.
    pub fn extract_from_file(path: &std::path::Path) -> Result<Option<Self>, String> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let mut file =
            File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let len = file
            .metadata()
            .map_err(|e| format!("cannot read metadata for {}: {e}", path.display()))?
            .len();

        if len < STANDALONE_FOOTER_LEN {
            return Ok(None);
        }

        file.seek(SeekFrom::Start(len - STANDALONE_FOOTER_LEN))
            .map_err(|e| format!("cannot seek footer: {e}"))?;

        let mut footer = [0u8; 16];
        file.read_exact(&mut footer)
            .map_err(|e| format!("cannot read footer: {e}"))?;

        let magic = &footer[8..16];
        if magic != STANDALONE_MAGIC {
            return Ok(None);
        }

        let bundle_len = u64::from_le_bytes(
            footer[0..8]
                .try_into()
                .map_err(|_| "failed to parse bundle length bytes".to_string())?,
        );

        if bundle_len > len - STANDALONE_FOOTER_LEN {
            return Err(format!(
                "invalid bundle length: {bundle_len} exceeds available payload size {}",
                len - STANDALONE_FOOTER_LEN
            ));
        }

        let bundle_len_usize = usize::try_from(bundle_len)
            .map_err(|_| format!("bundle length {bundle_len} exceeds usize capacity"))?;

        file.seek(SeekFrom::Start(len - STANDALONE_FOOTER_LEN - bundle_len))
            .map_err(|e| format!("cannot seek to bundle payload: {e}"))?;

        let mut payload = vec![0u8; bundle_len_usize];
        file.read_exact(&mut payload)
            .map_err(|e| format!("cannot read bundle payload: {e}"))?;

        let bundle: StandaloneBundle = bincode::deserialize(&payload)
            .map_err(|e| format!("failed to deserialize embedded bundle: {e}"))?;

        Ok(Some(bundle))
    }

    /// Extracts an embedded `StandaloneBundle` from a byte slice.
    pub fn extract_from_slice(data: &[u8]) -> Result<Option<Self>, String> {
        let len = data.len() as u64;
        if len < STANDALONE_FOOTER_LEN {
            return Ok(None);
        }

        let footer_start = (len - STANDALONE_FOOTER_LEN) as usize;
        let footer = &data[footer_start..];
        let magic = &footer[8..16];
        if magic != STANDALONE_MAGIC {
            return Ok(None);
        }

        let bundle_len = u64::from_le_bytes(
            footer[0..8]
                .try_into()
                .map_err(|_| "failed to parse bundle length bytes".to_string())?,
        );

        if bundle_len > len - STANDALONE_FOOTER_LEN {
            return Err(format!(
                "invalid bundle length: {bundle_len} exceeds available payload size {}",
                len - STANDALONE_FOOTER_LEN
            ));
        }

        let bundle_len_usize = usize::try_from(bundle_len)
            .map_err(|_| format!("bundle length {bundle_len} exceeds usize capacity"))?;

        let payload_start = (len - STANDALONE_FOOTER_LEN - bundle_len) as usize;
        let payload = &data[payload_start..payload_start + bundle_len_usize];

        let bundle: StandaloneBundle = bincode::deserialize(payload)
            .map_err(|e| format!("failed to deserialize embedded bundle: {e}"))?;

        Ok(Some(bundle))
    }
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
                            mutable: false,
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

    #[test]
    fn test_standalone_bundle_payload_round_trip() {
        let bundle = StandaloneBundle {
            entry: "main".to_string(),
            items: vec![Item::TypeAlias {
                name: "Id".to_string(),
                ty: TypeExpr::Named("int".to_string()),
            }],
        };
        let payload = bundle.encode_payload().expect("encode payload failed");
        assert!(payload.ends_with(STANDALONE_MAGIC));
        assert!(payload.len() > STANDALONE_FOOTER_LEN as usize);

        let decoded = StandaloneBundle::extract_from_slice(&payload)
            .expect("extraction failed")
            .expect("expected Some bundle");
        assert_eq!(decoded, bundle);
    }
}
