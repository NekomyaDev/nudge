//! Nudge parser — recursive descent, zero dependencies (design §12 grammar).
//! Day 1–3 MVP scope: everything needed to parse `examples/research_agent.ndg`.
//!
//! Intentionally deferred (with the type checker, day 4–6):
//!   optional types (T?), union types, record literals, pipe operator,
//!   agent/state blocks, spans on AST nodes.
//!
//! MVP infix rule: the identifier `zip` between two expressions parses as an
//! infix call (`a zip b` → `zip(a, b)`). Whitelisted, not general.
//!
//! Contextual keywords (design §12): `schema`, `retry`, `repair`, `impl`,
//! `replay`, … lex as ordinary identifiers; the parser matches them by string
//! where the grammar expects them, so they remain usable as names.

use crate::ast::*;
use crate::lexer::{Spanned, Tok};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    pub at: usize,
}

pub struct Parser {
    t: Vec<Spanned>,
    i: usize,
}

type PResult<T> = Result<T, ParseError>;

/// Positional args + keyword args of one call site.
type CallArgs = (Vec<Expr>, Vec<(String, Expr)>);

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Parser { t: tokens, i: 0 }
    }

    // ── cursor helpers ──────────────────────────────────────────────
    fn peek(&self) -> &Tok {
        &self.t[self.i].tok
    }
    fn peek2(&self) -> &Tok {
        self.t.get(self.i + 1).map(|s| &s.tok).unwrap_or(&Tok::Eof)
    }
    fn pos(&self) -> usize {
        self.t[self.i].start
    }
    fn bump(&mut self) {
        if self.i + 1 < self.t.len() {
            self.i += 1;
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError {
            msg: msg.into(),
            at: self.pos(),
        })
    }

    /// Wrap a statement kind with the span from `start` to the end of the
    /// last consumed token (spanned AST, stage 1).
    fn spanned(&self, start: usize, kind: StmtKind) -> Stmt {
        let end = if self.i == 0 {
            start
        } else {
            self.t[self.i - 1].end
        };
        Stmt {
            kind,
            span: Span {
                start,
                end: end.max(start),
            },
        }
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: &Tok, what: &str) -> PResult<()> {
        if self.eat(t) {
            Ok(())
        } else {
            self.err(format!("expected {what}, found {:?}", self.peek()))
        }
    }

    fn ident(&mut self) -> PResult<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => self.err(format!("expected identifier, found {other:?}")),
        }
    }

    // ── program ─────────────────────────────────────────────────────
    pub fn parse_program(&mut self) -> PResult<Vec<Item>> {
        let mut items = Vec::new();
        while !self.at(&Tok::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(items)
    }

    fn parse_item(&mut self) -> PResult<Item> {
        match self.peek().clone() {
            Tok::Type => self.parse_type_alias(),
            Tok::Fn => self.parse_fn(),
            Tok::Tool => self.parse_tool(),
            Tok::Test => self.parse_test(),
            Tok::Agent => self.parse_agent(),
            other => self.err(format!(
                "expected item (type/fn/tool/test), found {other:?}"
            )),
        }
    }

    // ── items ───────────────────────────────────────────────────────
    fn parse_type_alias(&mut self) -> PResult<Item> {
        self.bump(); // type
        let name = self.ident()?;
        self.expect(&Tok::Assign, "'=' in type alias")?;
        let ty = self.parse_type()?;
        Ok(Item::TypeAlias { name, ty })
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Tok::LParen, "'('")?;
        let mut ps = Vec::new();
        while !self.at(&Tok::RParen) {
            let name = self.ident()?;
            self.expect(&Tok::Colon, "':' after parameter name")?;
            let ty = self.parse_type()?;
            ps.push(Param { name, ty });
            self.eat(&Tok::Comma);
        }
        self.expect(&Tok::RParen, "')'")?;
        Ok(ps)
    }

    fn parse_effects(&mut self) -> PResult<Vec<String>> {
        let mut effs = Vec::new();
        if self.eat(&Tok::Uses) {
            loop {
                effs.push(self.ident()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok(effs)
    }

    fn parse_fn(&mut self) -> PResult<Item> {
        self.bump(); // fn
        let name = self.ident()?;
        let params = self.parse_params()?;
        self.expect(&Tok::Arrow, "'->' before return type")?;
        let ret = self.parse_type()?;
        let effects = self.parse_effects()?;
        let body = self.parse_block()?;
        Ok(Item::Fn {
            name,
            params,
            ret,
            effects,
            body,
        })
    }

    fn parse_tool(&mut self) -> PResult<Item> {
        self.bump(); // tool
        let name = self.ident()?;
        let params = self.parse_params()?;
        self.expect(&Tok::Arrow, "'->' before return type")?;
        let ret = self.parse_type()?;
        self.expect(&Tok::LBrace, "'{' before tool body")?;
        // fields may be comma-separated or newline-separated
        let mut fields = Vec::new();
        while !self.at(&Tok::RBrace) {
            let fname = self.ident()?;
            self.expect(&Tok::Colon, "':' in tool field")?;
            let val = self.parse_expr()?;
            fields.push((fname, val));
            self.eat(&Tok::Comma);
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(Item::Tool {
            name,
            params,
            ret,
            fields,
        })
    }

    fn parse_test(&mut self) -> PResult<Item> {
        self.bump(); // test
        let name = match self.peek().clone() {
            Tok::Str(s) => {
                self.bump();
                s
            }
            other => return self.err(format!("expected test name string, found {other:?}")),
        };
        let body = self.parse_block()?;
        Ok(Item::Test { name, body })
    }

    /// `agent Name { state { ... } fn ... }` (design §7, v0.2c MVP).
    fn parse_agent(&mut self) -> PResult<Item> {
        self.bump(); // agent
        let name = self.ident()?;
        self.expect(&Tok::LBrace, "'{' before agent body")?;
        let mut state = Vec::new();
        let mut fns = Vec::new();
        while !self.at(&Tok::RBrace) {
            if self.at(&Tok::Eof) {
                return self.err("unexpected end of file inside agent");
            }
            match self.peek() {
                Tok::State => {
                    self.bump(); // state
                    self.expect(&Tok::LBrace, "'{' before state block")?;
                    while !self.at(&Tok::RBrace) {
                        let fname = self.ident()?;
                        self.expect(&Tok::Colon, "':' in state field")?;
                        let ty = self.parse_type()?;
                        self.expect(&Tok::Assign, "'=' before state default")?;
                        let default = self.parse_expr()?;
                        state.push((fname, ty, default));
                        self.eat(&Tok::Comma);
                    }
                    self.expect(&Tok::RBrace, "'}' after state block")?;
                }
                Tok::Fn => fns.push(self.parse_fn()?),
                other => {
                    return self.err(format!(
                        "expected `state` or `fn` inside agent, found {other:?}"
                    ))
                }
            }
        }
        self.expect(&Tok::RBrace, "'}' after agent body")?;
        Ok(Item::Agent { name, state, fns })
    }

    fn parse_block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(&Tok::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) {
            if self.at(&Tok::Eof) {
                return self.err("unexpected end of file inside block");
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let start = self.pos();
        match self.peek().clone() {
            Tok::Let => {
                self.bump();
                let name = self.ident()?;
                let ty = if self.eat(&Tok::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(&Tok::Assign, "'=' in let binding")?;
                let value = self.parse_expr()?;
                Ok(self.spanned(
                    start,
                    StmtKind::Let {
                        name,
                        ty,
                        value,
                        stream: false,
                    },
                ))
            }
            // `stream let x: T = llm"""..."""` (design §4.5) — `stream` is a
            // contextual keyword: only special when directly followed by `let`
            Tok::Ident(s) if s == "stream" && self.peek2() == &Tok::Let => {
                self.bump(); // stream
                self.bump(); // let
                let name = self.ident()?;
                let ty = if self.eat(&Tok::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(&Tok::Assign, "'=' in stream let binding")?;
                let value = self.parse_expr()?;
                Ok(self.spanned(
                    start,
                    StmtKind::Let {
                        name,
                        ty,
                        value,
                        stream: true,
                    },
                ))
            }
            Tok::Assert => {
                self.bump();
                let e = self.parse_expr()?;
                Ok(self.spanned(start, StmtKind::Assert(e)))
            }
            // `state.x = v` / `+=` / `-=` (design §7) — only valid inside
            // an agent block; the checker enforces that (E0701). Lookahead
            // past `state . field` keeps a bare `state.x` read an expression.
            Tok::State
                if self.peek2() == &Tok::Dot
                    && matches!(self.t.get(self.i + 2).map(|s| &s.tok), Some(Tok::Ident(_)))
                    && matches!(
                        self.t.get(self.i + 3).map(|s| &s.tok),
                        Some(Tok::Assign) | Some(Tok::PlusEq) | Some(Tok::MinusEq)
                    ) =>
            {
                self.bump(); // state
                self.bump(); // .
                let field = self.ident()?;
                let op = if self.eat(&Tok::PlusEq) {
                    StateOp::Add
                } else if self.eat(&Tok::MinusEq) {
                    StateOp::Sub
                } else {
                    self.expect(&Tok::Assign, "'=', '+=' or '-=' in state write")?;
                    StateOp::Set
                };
                let value = self.parse_expr()?;
                Ok(self.spanned(start, StmtKind::StateWrite { field, op, value }))
            }
            _ => {
                let e = self.parse_expr()?;
                Ok(self.spanned(start, StmtKind::ExprStmt(e)))
            }
        }
    }

    // ── types ───────────────────────────────────────────────────────
    fn parse_type(&mut self) -> PResult<TypeExpr> {
        let mut ty = match self.peek().clone() {
            Tok::Ident(name) => {
                self.bump();
                TypeExpr::Named(name)
            }
            Tok::LBracket => {
                self.bump();
                let inner = self.parse_type()?;
                self.expect(&Tok::RBracket, "']' in list type")?;
                TypeExpr::List(Box::new(inner))
            }
            Tok::LBrace => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(&Tok::RBrace) {
                    let fname = self.ident()?;
                    self.expect(&Tok::Colon, "':' in record type")?;
                    fields.push((fname, self.parse_type()?));
                    self.eat(&Tok::Comma);
                }
                self.expect(&Tok::RBrace, "'}' in record type")?;
                TypeExpr::Record(fields)
            }
            Tok::LParen => {
                self.bump();
                self.expect(&Tok::RParen, "unit type '()'")?;
                TypeExpr::Named("()".into())
            }
            other => return self.err(format!("expected type, found {other:?}")),
        };
        // refinement: @name(args)
        if self.eat(&Tok::At) {
            let rname = self.ident()?;
            self.expect(&Tok::LParen, "'(' after refinement name")?;
            let mut args = Vec::new();
            while !self.at(&Tok::RParen) {
                args.push(self.parse_expr()?);
                self.eat(&Tok::Comma);
            }
            self.expect(&Tok::RParen, "')'")?;
            ty = TypeExpr::Refine(Box::new(ty), rname, args);
        }
        Ok(ty)
    }

    // ── expressions (precedence low → high, design §12) ─────────────
    pub fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_merge()
    }

    /// `l | merge r` (design §7): reducer join — `merge` is a contextual
    /// keyword, only special directly after `|` (so `|a|` lambdas and a
    /// variable named `merge` are unaffected).
    fn parse_merge(&mut self) -> PResult<Expr> {
        let mut l = self.parse_or()?;
        while self.at(&Tok::Bar) && matches!(self.peek2(), Tok::Ident(s) if s == "merge") {
            self.bump(); // |
            self.bump(); // merge
            let r = self.parse_or()?;
            l = Expr::Merge {
                l: Box::new(l),
                r: Box::new(r),
            };
        }
        Ok(l)
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut l = self.parse_and()?;
        while self.eat(&Tok::Or) {
            let r = self.parse_and()?;
            l = Expr::Binary {
                op: BinOp::Or,
                l: Box::new(l),
                r: Box::new(r),
            };
        }
        Ok(l)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut l = self.parse_cmp()?;
        while self.eat(&Tok::And) {
            let r = self.parse_cmp()?;
            l = Expr::Binary {
                op: BinOp::And,
                l: Box::new(l),
                r: Box::new(r),
            };
        }
        Ok(l)
    }

    fn parse_cmp(&mut self) -> PResult<Expr> {
        let mut l = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Tok::EqEq => Some(BinOp::Eq),
                Tok::NotEq => Some(BinOp::NotEq),
                Tok::Lt => Some(BinOp::Lt),
                Tok::LtEq => Some(BinOp::LtEq),
                Tok::Gt => Some(BinOp::Gt),
                Tok::GtEq => Some(BinOp::GtEq),
                _ => None,
            };
            if let Some(op) = op {
                self.bump();
                let r = self.parse_add()?;
                l = Expr::Binary {
                    op,
                    l: Box::new(l),
                    r: Box::new(r),
                };
                continue;
            }
            // MVP infix rule: `a zip b` → zip(a, b)
            if matches!(self.peek(), Tok::Ident(s) if s == "zip") {
                self.bump();
                let r = self.parse_add()?;
                l = Expr::Call {
                    func: Box::new(Expr::Ident("zip".into())),
                    args: vec![l, r],
                    kwargs: vec![],
                };
                continue;
            }
            break;
        }
        Ok(l)
    }

    fn parse_add(&mut self) -> PResult<Expr> {
        let mut l = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let r = self.parse_mul()?;
            l = Expr::Binary {
                op,
                l: Box::new(l),
                r: Box::new(r),
            };
        }
        Ok(l)
    }

    fn parse_mul(&mut self) -> PResult<Expr> {
        let mut l = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            let r = self.parse_unary()?;
            l = Expr::Binary {
                op,
                l: Box::new(l),
                r: Box::new(r),
            };
        }
        Ok(l)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        match self.peek() {
            Tok::Minus => {
                self.bump();
                Ok(Expr::Unary {
                    op: BinOp::Sub,
                    x: Box::new(self.parse_unary()?),
                })
            }
            Tok::Bang => {
                self.bump();
                Ok(Expr::Unary {
                    op: BinOp::Not,
                    x: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Tok::Dot => {
                    self.bump();
                    let name = self.ident()?;
                    e = Expr::Field {
                        obj: Box::new(e),
                        name,
                    };
                }
                Tok::LParen => {
                    let (args, kwargs) = self.parse_call_args()?;
                    e = Expr::Call {
                        func: Box::new(e),
                        args,
                        kwargs,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_call_args(&mut self) -> PResult<CallArgs> {
        self.expect(&Tok::LParen, "'('")?;
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        while !self.at(&Tok::RParen) {
            if let (Tok::Ident(name), Tok::Assign) = (self.peek().clone(), self.peek2().clone()) {
                self.bump();
                self.bump();
                kwargs.push((name, self.parse_expr()?));
            } else {
                args.push(self.parse_expr()?);
            }
            self.eat(&Tok::Comma);
        }
        self.expect(&Tok::RParen, "')'")?;
        Ok((args, kwargs))
    }

    fn parse_lambda(&mut self) -> PResult<(Vec<String>, Expr)> {
        self.expect(&Tok::Bar, "'|' to start lambda parameters")?;
        let params = if self.eat(&Tok::LParen) {
            let mut ps = Vec::new();
            while !self.at(&Tok::RParen) {
                ps.push(self.ident()?);
                self.eat(&Tok::Comma);
            }
            self.expect(&Tok::RParen, "')'")?;
            ps
        } else {
            vec![self.ident()?]
        };
        self.expect(&Tok::Bar, "closing '|'")?;
        self.expect(&Tok::Arrow, "'->' before lambda body")?;
        let body = self.parse_expr()?;
        Ok((params, body))
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr::Int(v))
            }
            Tok::Float(v) => {
                self.bump();
                Ok(Expr::Float(v))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Str(s))
            }
            Tok::Money(v, u) => {
                let (v, u) = (v, u.clone());
                self.bump();
                Ok(Expr::Money(v, u))
            }
            Tok::True => {
                self.bump();
                Ok(Expr::Bool(true))
            }
            Tok::False => {
                self.bump();
                Ok(Expr::Bool(false))
            }
            Tok::None => {
                self.bump();
                Ok(Expr::None)
            }
            // `route{ cheap: "m" when cond, strong: "m2" otherwise }` (design
            // §4.4) — `route` is contextual: only special directly before `{`
            Tok::Ident(s) if s == "route" && self.peek2() == &Tok::LBrace => {
                self.bump(); // route
                self.bump(); // {
                let mut arms = Vec::new();
                while !self.at(&Tok::RBrace) {
                    let label = self.ident()?;
                    self.expect(&Tok::Colon, "':' in route arm")?;
                    let model = match self.peek().clone() {
                        Tok::Str(m) => {
                            self.bump();
                            m
                        }
                        other => {
                            return self.err(format!(
                                "expected model string in route arm, found {other:?}"
                            ))
                        }
                    };
                    let cond = if self.eat(&Tok::Ident("when".into())) {
                        Some(self.parse_expr()?)
                    } else if self.eat(&Tok::Ident("otherwise".into())) {
                        None
                    } else {
                        return self.err("expected `when <cond>` or `otherwise` in route arm");
                    };
                    arms.push((label, model, cond));
                    self.eat(&Tok::Comma);
                }
                self.expect(&Tok::RBrace, "'}' after route block")?;
                Ok(Expr::Route { arms })
            }
            // `state` reads (`state.round`) inside agent fns (design §7);
            // codegen binds it to the agent's checkpointed state object
            Tok::State => {
                self.bump();
                Ok(Expr::Ident("state".into()))
            }
            Tok::Ident(name) => {
                self.bump();
                Ok(Expr::Ident(name))
            }
            Tok::LBracket => {
                self.bump();
                let mut xs = Vec::new();
                while !self.at(&Tok::RBracket) {
                    xs.push(self.parse_expr()?);
                    self.eat(&Tok::Comma);
                }
                self.expect(&Tok::RBracket, "']'")?;
                Ok(Expr::ListLit(xs))
            }
            Tok::LParen => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen, "')'")?;
                Ok(e)
            }
            Tok::Prompt(body) => {
                self.bump();
                let prompt = Expr::Prompt {
                    interpolations: scan_interpolations(&body),
                    body,
                };
                // optional with-block turns it into an LlmCall
                if self.at(&Tok::With) && self.peek2() == &Tok::LBrace {
                    self.bump(); // with
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut options = Vec::new();
                    let mut repair = false;
                    while !self.at(&Tok::RBrace) {
                        let key = self.ident()?;
                        self.expect(&Tok::Colon, "':' in with-block")?;
                        let val = self.parse_expr()?;
                        options.push((key, val));
                        // `retry: N with repair` — `repair` is a contextual keyword (design §12)
                        if self.at(&Tok::With)
                            && matches!(self.peek2(), Tok::Ident(s) if s == "repair")
                        {
                            self.bump();
                            self.bump();
                            repair = true;
                        }
                        self.eat(&Tok::Comma);
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    Ok(Expr::LlmCall {
                        prompt: Box::new(prompt),
                        options,
                        repair,
                    })
                } else {
                    Ok(prompt)
                }
            }
            Tok::Par => {
                self.bump();
                match self.peek().clone() {
                    Tok::Map => {
                        self.bump();
                        let (coll, kwargs) = if self.at(&Tok::LParen) {
                            let (args, kwargs) = self.parse_call_args()?;
                            if args.len() != 1 {
                                return self.err("par map takes exactly one positional collection");
                            }
                            (args.into_iter().next().unwrap(), kwargs)
                        } else {
                            (self.parse_add()?, Vec::new())
                        };
                        let (params, body) = self.parse_lambda()?;
                        Ok(Expr::ParMap {
                            coll: Box::new(coll),
                            kwargs,
                            params,
                            body: Box::new(body),
                        })
                    }
                    Tok::All => {
                        self.bump();
                        let (args, _) = self.parse_call_args()?;
                        Ok(Expr::ParAll(args))
                    }
                    Tok::Race => {
                        self.bump();
                        match self.parse_primary()? {
                            Expr::ListLit(xs) => Ok(Expr::ParRace(xs)),
                            _ => self.err("par race expects a list of callables"),
                        }
                    }
                    other => self.err(format!("expected map/all/race after par, found {other:?}")),
                }
            }
            other => self.err(format!("unexpected token {other:?}")),
        }
    }
}

/// Pull `{name}` / `{path.to.value}` interpolations out of a prompt body.
pub fn scan_interpolations(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(close) => {
                out.push(rest[open + 1..open + close].trim().to_string());
                rest = &rest[open + close + 1..];
            }
            None => break,
        }
    }
    out
}

/// Convenience entry: tokens → program.
pub fn parse(tokens: Vec<Spanned>) -> PResult<Vec<Item>> {
    Parser::new(tokens).parse_program()
}

// ── tests ────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_str(src: &str) -> Vec<Item> {
        parse(lex(src).unwrap()).unwrap()
    }

    #[test]
    fn type_alias_with_record_and_refinement() {
        let items = parse_str("type Finding = { claim: string, confidence: float @range(0, 1) }");
        match &items[0] {
            Item::TypeAlias { name, ty } => {
                assert_eq!(name, "Finding");
                match ty {
                    TypeExpr::Record(fields) => {
                        assert_eq!(fields.len(), 2);
                        assert!(
                            matches!(fields[1].1, TypeExpr::Refine(_, ref r, _) if r == "range")
                        );
                    }
                    other => panic!("expected record type, got {other:?}"),
                }
            }
            other => panic!("expected type alias, got {other:?}"),
        }
    }

    #[test]
    fn fn_with_llm_call_options_and_repair() {
        let src = r#"
fn analyze(q: string) -> [Finding] uses LLM {
    llm"""Extract findings about {q}"""
    with { schema: [Finding], model: "m", budget: 0.03 USD, retry: 2 with repair }
}"#;
        let items = parse_str(src);
        match &items[0] {
            Item::Fn {
                name,
                effects,
                body,
                ..
            } => {
                assert_eq!(name, "analyze");
                assert_eq!(effects, &vec!["LLM".to_string()]);
                match &body[0].kind {
                    StmtKind::ExprStmt(Expr::LlmCall {
                        prompt,
                        options,
                        repair,
                    }) => {
                        assert!(repair);
                        assert_eq!(options.len(), 4);
                        match prompt.as_ref() {
                            Expr::Prompt { interpolations, .. } => {
                                assert_eq!(interpolations, &vec!["q".to_string()]);
                            }
                            other => panic!("expected prompt, got {other:?}"),
                        }
                    }
                    other => panic!("expected llm call, got {other:?}"),
                }
            }
            other => panic!("expected fn, got {other:?}"),
        }
    }

    #[test]
    fn par_map_both_forms() {
        // bare collection form
        let items = parse_str("fn f() -> () { let h = par map angles |a| -> search(a) }");
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::Let {
                    value: Expr::ParMap { params, kwargs, .. },
                    ..
                } => {
                    assert_eq!(params, &vec!["a".to_string()]);
                    assert!(kwargs.is_empty());
                }
                other => panic!("expected par map, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
        // paren form with zip infix + kwargs + tuple params
        let items = parse_str(
            "fn f() -> () { let f2 = par map(angles zip hits, concurrency = 3) |(a, h)| -> analyze(a, h) }",
        );
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::Let {
                    value:
                        Expr::ParMap {
                            coll,
                            params,
                            kwargs,
                            ..
                        },
                    ..
                } => {
                    assert_eq!(params, &vec!["a".to_string(), "h".to_string()]);
                    assert_eq!(kwargs.len(), 1);
                    assert!(matches!(coll.as_ref(), Expr::Call { .. })); // zip(angles, hits)
                }
                other => panic!("expected par map, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn test_block_with_asserts() {
        let src = r#"
test "budget" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25
    assert len(t.output.findings) >= 3
}"#;
        let items = parse_str(src);
        match &items[0] {
            Item::Test { name, body } => {
                assert_eq!(name, "budget");
                assert_eq!(body.len(), 3);
                assert!(matches!(
                    body[1],
                    StmtKind::Assert(Expr::Binary { op: BinOp::Lt, .. })
                ));
                assert!(matches!(
                    body[2],
                    StmtKind::Assert(Expr::Binary {
                        op: BinOp::GtEq,
                        ..
                    })
                ));
            }
            other => panic!("expected test, got {other:?}"),
        }
    }

    #[test]
    fn tool_fields_without_commas() {
        let src = "tool web_search(q: string) -> [SearchResult] { impl: mcp(\"search\").web(q) side_effects: none }";
        let items = parse_str(src);
        match &items[0] {
            Item::Tool { fields, .. } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "impl");
                assert_eq!(fields[1].0, "side_effects");
                assert!(matches!(fields[1].1, Expr::None));
            }
            other => panic!("expected tool, got {other:?}"),
        }
    }

    #[test]
    fn full_research_agent_example_parses() {
        let src = include_str!("../../../examples/research_agent.ndg");
        let items = parse_str(src);
        // 4 type aliases (Url, Finding, SearchResult, Report), 1 tool, 2 fns, 1 test
        assert_eq!(items.len(), 8);
        assert!(matches!(items[0], Item::TypeAlias { .. }));
        assert!(matches!(items[3], Item::TypeAlias { .. }));
        assert!(matches!(items[4], Item::Tool { .. }));
        assert!(matches!(items[5], Item::Fn { .. }));
        assert!(matches!(items[6], Item::Fn { .. }));
        assert!(matches!(items[7], Item::Test { .. }));
    }

    #[test]
    fn stream_let_parses_and_stream_stays_an_identifier() {
        let src = "fn f() -> string uses LLM { stream let t: string = llm\"\"\"x\"\"\" with { model: \"m\" }\n    t }";
        let items = parse_str(src);
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::Let {
                    name,
                    stream,
                    value,
                    ..
                } => {
                    assert_eq!(name, "t");
                    assert!(stream);
                    assert!(matches!(value, Expr::LlmCall { .. }));
                }
                other => panic!("expected stream let, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
        // contextual keyword: `stream` remains usable as a variable name
        let items = parse_str("fn f() -> int { let stream = 3\n    stream }");
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::Let { name, stream, .. } => {
                    assert_eq!(name, "stream");
                    assert!(!stream);
                }
                other => panic!("expected plain let, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn agent_state_block_parses() {
        let src = "agent Researcher {\n    state {\n        notes: [string] = [],\n        round: int = 0,\n    }\n    fn step(q: string) -> int uses LLM {\n        let r = llm\"\"\"next: {q}\"\"\" with { model: \"m\" }\n        state.notes += [r]\n        state.round = state.round + 1\n        state.round\n    }\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        match &items[0] {
            Item::Agent { name, state, fns } => {
                assert_eq!(name, "Researcher");
                assert_eq!(state.len(), 2);
                assert_eq!(state[0].0, "notes");
                assert_eq!(state[1].0, "round");
                assert_eq!(fns.len(), 1);
                match &fns[0] {
                    Item::Fn { body, .. } => {
                        assert!(
                            matches!(&body[1].kind, StmtKind::StateWrite { field, op: StateOp::Add, .. } if field == "notes")
                        );
                        assert!(
                            matches!(&body[2].kind, StmtKind::StateWrite { field, op: StateOp::Set, .. } if field == "round")
                        );
                    }
                    other => panic!("expected fn inside agent, got {other:?}"),
                }
            }
            other => panic!("expected agent, got {other:?}"),
        }
    }

    #[test]
    fn route_block_parses_and_route_stays_an_identifier() {
        let src = "fn f(cheap_ok: bool) -> string uses LLM { llm\"\"\"x\"\"\" with { model: route{ cheap: \"m1\" when cheap_ok, strong: \"m2\" otherwise } } }";
        let items = parse(lex(src).unwrap()).unwrap();
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::ExprStmt(Expr::LlmCall { options, .. }) => match &options[0].1 {
                    Expr::Route { arms } => {
                        assert_eq!(arms.len(), 2);
                        assert_eq!(arms[0].0, "cheap");
                        assert_eq!(arms[0].1, "m1");
                        assert!(arms[0].2.is_some());
                        assert_eq!(arms[1].0, "strong");
                        assert!(arms[1].2.is_none());
                    }
                    other => panic!("expected route, got {other:?}"),
                },
                other => panic!("expected llm call, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
        // `route` alone is an ordinary identifier
        let items = parse(lex("fn f() -> int { let route = 3\n    route }").unwrap()).unwrap();
        match &items[0] {
            Item::Fn { body, .. } => {
                assert!(matches!(&body[0].kind, StmtKind::Let { name, .. } if name == "route"))
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn merge_infix_parses_and_merge_stays_an_identifier() {
        // `l | merge r` is a reducer join (design §7)
        let items = parse(lex("fn f(x: [int]) -> [int] { x | merge x }").unwrap()).unwrap();
        match &items[0] {
            Item::Fn { body, .. } => match &body[0].kind {
                StmtKind::ExprStmt(Expr::Merge { .. }) => {}
                other => panic!("expected merge expr, got {other:?}"),
            },
            _ => panic!("expected fn"),
        }
        // `merge` alone is an ordinary identifier
        let items = parse(lex("fn f() -> int { let merge = 3\n    merge }").unwrap()).unwrap();
        match &items[0] {
            Item::Fn { body, .. } => {
                assert!(matches!(&body[0].kind, StmtKind::Let { name, .. } if name == "merge"));
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn statements_carry_source_spans() {
        let src = "fn f() -> int { let x = 1\n    x }";
        let items = parse(lex(src).unwrap()).unwrap();
        match &items[0] {
            Item::Fn { body, .. } => {
                let sp = body[0].span;
                assert_eq!(&src[sp.start..sp.end], "let x = 1");
                let sp2 = body[1].span;
                assert_eq!(&src[sp2.start..sp2.end], "x");
            }
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn parse_error_is_clean_not_panic() {
        assert!(parse(lex("fn broken(").unwrap()).is_err());
    }

    #[test]
    fn state_write_supports_minus_eq() {
        let src = "agent A {\n    state {\n        round: int = 0,\n    }\n    fn back() -> int {\n        state.round -= 1\n        state.round\n    }\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        match &items[0] {
            Item::Agent { fns, .. } => match &fns[0] {
                Item::Fn { body, .. } => {
                    assert!(
                        matches!(&body[0].kind, StmtKind::StateWrite { field, op: StateOp::Sub, .. } if field == "round")
                    );
                }
                other => panic!("expected fn, got {other:?}"),
            },
            other => panic!("expected agent, got {other:?}"),
        }
    }
}

