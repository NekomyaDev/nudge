//! Nudge ByteCode (NBC) representation and compiler for the pure-compute core.
//!
//! Compiles the pure-compute subset of Nudge AST items into bytecode. Any effectful
//! or unsupported construct (e.g. LLM calls, tool calls, computer calls, par map,
//! decide, merge, agent state writes, stream let) is rejected with a clear
//! "not in bytecode subset" error so callers can fall back to the interpreter.

use std::collections::HashMap;

use crate::ast::{self, BinOp, Expr, ExprKind, Item, Stmt, StmtKind};
use crate::eval::NudgeVal;

/// Opcodes for the Nudge ByteCode (NBC) virtual machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Push constant from chunk's constant pool onto the stack.
    Const(u32),
    /// Push `NudgeVal::Unit`.
    Unit,
    /// Push `NudgeVal::Bool(true)`.
    True,
    /// Push `NudgeVal::Bool(false)`.
    False,
    /// Push local from slot `i` onto the stack.
    LoadLocal(u32),
    /// Pop value from stack and store into slot `i`.
    StoreLocal(u32),
    /// Binary arithmetic / concatenation: pop right, pop left, push left + right.
    Add,
    /// Binary arithmetic: pop right, pop left, push left - right.
    Sub,
    /// Binary arithmetic: pop right, pop left, push left * right.
    Mul,
    /// Binary arithmetic: pop right, pop left, push left / right (float division).
    Div,
    /// Binary modulo: pop right, pop left, push left % right.
    Mod,
    /// Equality comparison (`loose_eq`): pop right, pop left, push bool.
    Eq,
    /// Inequality comparison: pop right, pop left, push bool.
    NotEq,
    /// Order comparison: pop right, pop left, push left < right.
    Lt,
    /// Order comparison: pop right, pop left, push left <= right.
    LtEq,
    /// Order comparison: pop right, pop left, push left > right.
    Gt,
    /// Order comparison: pop right, pop left, push left >= right.
    GtEq,
    /// Unary negate: pop val, push -val.
    Neg,
    /// Logical not: pop val, push `!val.is_truthy()`.
    Not,
    /// Convert top of stack to its boolean truthiness (`NudgeVal::Bool(val.is_truthy())`).
    ToBool,
    /// Unconditional jump to instruction index.
    Jump(usize),
    /// Conditional jump for short-circuit `||`: pop val.
    /// If truthy, push `NudgeVal::Bool(true)` and jump to target.
    /// If falsey, do not jump (and push nothing, leaving stack clean).
    JumpIfTrue(usize),
    /// Conditional jump for short-circuit `&&`: pop val.
    /// If falsey, push `NudgeVal::Bool(false)` and jump to target.
    /// If truthy, do not jump (and push nothing, leaving stack clean).
    JumpIfFalse(usize),
    /// Pure conditional branch for control flow: pop val.
    /// If not truthy (`!val.is_truthy()`), jump to target instruction index.
    /// Leaves stack clean (pushes nothing back).
    BranchIfFalse(usize),
    /// Construct list: pop `n` values (reverse order) and push `NudgeVal::List`.
    MakeList(usize),
    /// Construct record/map: pop values for each key (reverse order) and push `NudgeVal::Map`.
    MakeRecord(Vec<String>),
    /// Record field access: pop map object, push field value.
    Field(String),
    /// Call user function: `(fn_index, argc)`.
    Call(u32, usize),
    /// Call pure built-in function: `(name, argc)`.
    CallBuiltin(String, usize),
    /// Assertion check: pop val. If not truthy, fail with exact error message.
    Assert(String),
    /// Immediately abort execution with a runtime error message.
    Fail(String),
    /// Pop top of stack and discard.
    Pop,
    /// Return from current function frame.
    Return,
}

/// A sequence of bytecode instructions along with a constant pool.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chunk {
    pub code: Vec<Op>,
    pub constants: Vec<NudgeVal>,
}

impl Chunk {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, op: Op) -> usize {
        let idx = self.code.len();
        self.code.push(op);
        idx
    }

    pub fn add_constant(&mut self, val: NudgeVal) -> u32 {
        self.constants.push(val);
        (self.constants.len() - 1) as u32
    }
}

/// A compiled function holding its name, parameter metadata, chunk, and local slot count.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub chunk: Chunk,
    pub local_count: usize,
}

/// A compiled NBC program containing function chunks and an entry index table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub functions: Vec<Function>,
    pub entry_indices: HashMap<String, usize>,
}

impl Program {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_fn(&self, name: &str) -> Option<&Function> {
        self.entry_indices
            .get(name)
            .map(|&idx| &self.functions[idx])
    }
}

/// Metadata recorded for each declared function during the first pass.
struct FnMeta {
    name: String,
    params: Vec<String>,
    param_types: Vec<String>,
    body: Vec<Stmt>,
}

/// Context for compiling a single function body.
struct Compiler<'a> {
    chunk: Chunk,
    locals: HashMap<String, u32>,
    next_local: u32,
    fn_map: &'a HashMap<String, usize>,
    fn_metas: &'a [FnMeta],
    tool_names: &'a [String],
}

impl<'a> Compiler<'a> {
    fn new(
        fn_meta: &FnMeta,
        fn_map: &'a HashMap<String, usize>,
        fn_metas: &'a [FnMeta],
        tool_names: &'a [String],
    ) -> Self {
        let mut locals = HashMap::new();
        for (i, p) in fn_meta.params.iter().enumerate() {
            locals.insert(p.clone(), i as u32);
        }
        let next_local = fn_meta.params.len() as u32;

        Self {
            chunk: Chunk::new(),
            locals,
            next_local,
            fn_map,
            fn_metas,
            tool_names,
        }
    }

    fn compile_body(mut self, body: &[Stmt]) -> Result<Function, String> {
        if body.is_empty() {
            self.chunk.emit(Op::Unit);
            self.chunk.emit(Op::Return);
        } else {
            let last_idx = body.len() - 1;
            for (i, stmt) in body.iter().enumerate() {
                if i == last_idx {
                    if let StmtKind::ExprStmt(expr) = &stmt.kind {
                        self.compile_expr(expr)?;
                        self.chunk.emit(Op::Return);
                        continue;
                    }
                }
                self.compile_stmt(stmt)?;
            }

            // If the last statement was a Let or Assert, return Unit
            let ends_with_expr_stmt =
                matches!(body.last(), Some(s) if matches!(s.kind, StmtKind::ExprStmt(_)));
            if !ends_with_expr_stmt {
                self.chunk.emit(Op::Unit);
                self.chunk.emit(Op::Return);
            }
        }

        Ok(Function {
            name: String::new(),
            params: Vec::new(),
            param_types: Vec::new(),
            chunk: self.chunk,
            local_count: self.next_local as usize,
        })
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let {
                name,
                value,
                stream,
                ..
            } => {
                if *stream {
                    return Err("not in bytecode subset: stream let".to_string());
                }
                self.compile_expr(value)?;
                let slot = if let Some(&s) = self.locals.get(name) {
                    s
                } else {
                    let s = self.next_local;
                    self.next_local += 1;
                    self.locals.insert(name.clone(), s);
                    s
                };
                self.chunk.emit(Op::StoreLocal(slot));
                Ok(())
            }
            StmtKind::ExprStmt(expr) => {
                self.compile_expr(expr)?;
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            StmtKind::Assert(expr) => {
                self.compile_expr(expr)?;
                // Preserve the exact debug representation matching interpreter.rs
                self.chunk.emit(Op::Assert(format!("{expr:?}")));
                Ok(())
            }
            StmtKind::StateWrite { .. } => {
                Err("not in bytecode subset: state write (agent state)".to_string())
            }
            StmtKind::ForAll { .. } => {
                Err("not in bytecode subset: for_all property loop".to_string())
            }
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match &expr.kind {
            ExprKind::Int(v) => {
                let idx = self.chunk.add_constant(NudgeVal::Int(*v));
                self.chunk.emit(Op::Const(idx));
                Ok(())
            }
            ExprKind::Float(v) => {
                let idx = self.chunk.add_constant(NudgeVal::Float(*v));
                self.chunk.emit(Op::Const(idx));
                Ok(())
            }
            ExprKind::Str(s) => {
                let idx = self.chunk.add_constant(NudgeVal::Str(s.clone()));
                self.chunk.emit(Op::Const(idx));
                Ok(())
            }
            ExprKind::Money(v, _) => {
                // Match interpreter.rs line 243: Money evaluates directly to NudgeVal::Float
                let idx = self.chunk.add_constant(NudgeVal::Float(*v));
                self.chunk.emit(Op::Const(idx));
                Ok(())
            }
            ExprKind::Bool(b) => {
                self.chunk.emit(if *b { Op::True } else { Op::False });
                Ok(())
            }
            ExprKind::None => {
                self.chunk.emit(Op::Unit);
                Ok(())
            }
            ExprKind::Ident(name) => {
                if name == "state" {
                    return Err("not in bytecode subset: agent state read".to_string());
                }
                if name == "true" {
                    self.chunk.emit(Op::True);
                    return Ok(());
                }
                if name == "false" {
                    self.chunk.emit(Op::False);
                    return Ok(());
                }
                if name == "none" {
                    self.chunk.emit(Op::Unit);
                    return Ok(());
                }
                if let Some(&slot) = self.locals.get(name) {
                    self.chunk.emit(Op::LoadLocal(slot));
                    Ok(())
                } else {
                    Err(format!("unknown identifier '{name}'"))
                }
            }
            ExprKind::ListLit(items) => {
                for item in items {
                    self.compile_expr(item)?;
                }
                self.chunk.emit(Op::MakeList(items.len()));
                Ok(())
            }
            ExprKind::Field { obj, name } => {
                if let ExprKind::Ident(id) = &obj.kind {
                    if id == "state" {
                        return Err("not in bytecode subset: agent state read".to_string());
                    }
                }
                self.compile_expr(obj)?;
                self.chunk.emit(Op::Field(name.clone()));
                Ok(())
            }
            ExprKind::Unary { op, x } => {
                self.compile_expr(x)?;
                match op {
                    BinOp::Sub => {
                        self.chunk.emit(Op::Neg);
                        Ok(())
                    }
                    BinOp::Not => {
                        self.chunk.emit(Op::Not);
                        Ok(())
                    }
                    _ => Err(format!(
                        "not in bytecode subset: unsupported unary op {:?}",
                        op
                    )),
                }
            }
            ExprKind::Binary { op, l, r } => match op {
                BinOp::And => {
                    self.compile_expr(l)?;
                    let jump_idx = self.chunk.emit(Op::JumpIfFalse(0));
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::ToBool);
                    let target = self.chunk.code.len();
                    self.chunk.code[jump_idx] = Op::JumpIfFalse(target);
                    Ok(())
                }
                BinOp::Or => {
                    self.compile_expr(l)?;
                    let jump_idx = self.chunk.emit(Op::JumpIfTrue(0));
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::ToBool);
                    let target = self.chunk.code.len();
                    self.chunk.code[jump_idx] = Op::JumpIfTrue(target);
                    Ok(())
                }
                BinOp::Add => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Add);
                    Ok(())
                }
                BinOp::Sub => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Sub);
                    Ok(())
                }
                BinOp::Mul => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Mul);
                    Ok(())
                }
                BinOp::Div => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Div);
                    Ok(())
                }
                BinOp::Mod => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Mod);
                    Ok(())
                }
                BinOp::Eq => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Eq);
                    Ok(())
                }
                BinOp::NotEq => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::NotEq);
                    Ok(())
                }
                BinOp::Lt => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Lt);
                    Ok(())
                }
                BinOp::LtEq => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::LtEq);
                    Ok(())
                }
                BinOp::Gt => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::Gt);
                    Ok(())
                }
                BinOp::GtEq => {
                    self.compile_expr(l)?;
                    self.compile_expr(r)?;
                    self.chunk.emit(Op::GtEq);
                    Ok(())
                }
                BinOp::Not => Err("not in bytecode subset: invalid binary op Not".to_string()),
            },
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

                let name = match callee_name {
                    Some(n) => n,
                    None => {
                        return Err(
                            "not in bytecode subset: dynamic function or computed call".to_string()
                        )
                    }
                };

                if name == "len" {
                    if !kwargs.is_empty() {
                        return Err("'len' takes 1 argument, got kwargs".to_string());
                    }
                    for a in args {
                        self.compile_expr(a)?;
                    }
                    self.chunk
                        .emit(Op::CallBuiltin("len".to_string(), args.len()));
                    return Ok(());
                }

                if name == "zip" {
                    if !kwargs.is_empty() {
                        return Err("'zip' takes 2 arguments, got kwargs".to_string());
                    }
                    for a in args {
                        self.compile_expr(a)?;
                    }
                    self.chunk
                        .emit(Op::CallBuiltin("zip".to_string(), args.len()));
                    return Ok(());
                }

                if let Some(&fn_idx) = self.fn_map.get(&name) {
                    let target_meta = &self.fn_metas[fn_idx];
                    let total_params = target_meta.params.len();

                    if args.len() > total_params {
                        return Err(format!(
                            "function takes at most {} arguments, got {}",
                            total_params,
                            args.len()
                        ));
                    }

                    // Map positional args and kwargs to parameter slots
                    let mut bound_exprs: Vec<Option<&Expr>> = vec![None; total_params];
                    for (i, a) in args.iter().enumerate() {
                        bound_exprs[i] = Some(a);
                    }
                    for (k, v) in kwargs {
                        let pos =
                            target_meta
                                .params
                                .iter()
                                .position(|p| p == k)
                                .ok_or_else(|| {
                                    format!("unknown parameter '{k}' in call to '{name}'")
                                })?;
                        if bound_exprs[pos].is_some() {
                            return Err(format!(
                                "duplicate argument for parameter '{k}' in call to '{name}'"
                            ));
                        }
                        bound_exprs[pos] = Some(v);
                    }

                    for (i, p) in target_meta.params.iter().enumerate() {
                        if bound_exprs[i].is_none() {
                            return Err(format!("missing argument for parameter '{p}'"));
                        }
                    }

                    for bound in bound_exprs {
                        self.compile_expr(bound.expect("all parameters checked"))?;
                    }

                    self.chunk.emit(Op::Call(fn_idx as u32, total_params));
                    Ok(())
                } else if self.tool_names.iter().any(|t| t == &name) {
                    Err(format!("not in bytecode subset: call to tool '{name}'"))
                } else {
                    Err(format!(
                        "not in bytecode subset: unknown function or effect '{name}'"
                    ))
                }
            }
            ExprKind::Prompt { .. } => Err("not in bytecode subset: prompt literal".to_string()),
            ExprKind::LlmCall { .. } => Err("not in bytecode subset: llm call".to_string()),
            ExprKind::ComputerCall { .. } => {
                Err("not in bytecode subset: computer call".to_string())
            }
            ExprKind::ParMap { .. } => Err("not in bytecode subset: par map".to_string()),
            ExprKind::ParAll(_) => Err("not in bytecode subset: par all".to_string()),
            ExprKind::ParRace(_) => Err("not in bytecode subset: par race".to_string()),
            ExprKind::Merge { .. } => Err("not in bytecode subset: merge".to_string()),
            ExprKind::Route { arms } => {
                let mut end_jumps = Vec::new();
                let mut has_otherwise = false;

                for (_label, val_expr, cond_expr) in arms {
                    if let Some(cond) = cond_expr {
                        self.compile_expr(cond)?;
                        let jump_next = self.chunk.emit(Op::BranchIfFalse(0));
                        self.compile_expr(val_expr)?;
                        end_jumps.push(self.chunk.emit(Op::Jump(0)));
                        let next_target = self.chunk.code.len();
                        self.chunk.code[jump_next] = Op::BranchIfFalse(next_target);
                    } else {
                        self.compile_expr(val_expr)?;
                        end_jumps.push(self.chunk.emit(Op::Jump(0)));
                        has_otherwise = true;
                    }
                }

                if !has_otherwise {
                    self.chunk.emit(Op::Fail(
                        "route block matched no arm and has no otherwise fallback".to_string(),
                    ));
                }

                let end_target = self.chunk.code.len();
                for j in end_jumps {
                    self.chunk.code[j] = Op::Jump(end_target);
                }

                Ok(())
            }
            ExprKind::DecideCall { .. } => Err("not in bytecode subset: decide call".to_string()),
        }
    }
}

/// Compiles pure Nudge AST items into a runnable `Program`.
///
/// Returns an error if any non-pure or unsupported construct is encountered.
pub fn compile(items: &[ast::Item]) -> Result<Program, String> {
    let mut fn_metas = Vec::new();
    let mut fn_map = HashMap::new();
    let mut tool_names = Vec::new();

    // First pass: validate items and gather function signatures and tools
    for item in items {
        match item {
            Item::TypeAlias { .. } => {
                // Type aliases are purely static; ignore
            }
            Item::Fn {
                name,
                params,
                ret: _,
                effects,
                body,
            } => {
                if !effects.is_empty() {
                    return Err(format!(
                        "not in bytecode subset: function '{name}' declares effects: {effects:?}"
                    ));
                }
                let idx = fn_metas.len();
                fn_map.insert(name.clone(), idx);
                fn_metas.push(FnMeta {
                    name: name.clone(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_types: params.iter().map(|p| p.ty.to_string()).collect(),
                    body: body.clone(),
                });
            }
            Item::Tool { name, .. } => {
                tool_names.push(name.clone());
            }
            Item::Agent { name, .. } => {
                return Err(format!(
                    "not in bytecode subset: agent declaration '{name}'"
                ));
            }
            Item::Test { .. } => {
                // Tests are skipped during compilation; only functions are compiled
            }
        }
    }

    // Second pass: compile function bodies
    let mut functions = Vec::with_capacity(fn_metas.len());
    let mut entry_indices = HashMap::new();

    for (idx, meta) in fn_metas.iter().enumerate() {
        let compiler = Compiler::new(meta, &fn_map, &fn_metas, &tool_names);
        let mut compiled_fn = compiler.compile_body(&meta.body)?;
        compiled_fn.name = meta.name.clone();
        compiled_fn.params = meta.params.clone();
        compiled_fn.param_types = meta.param_types.clone();

        entry_indices.insert(meta.name.clone(), idx);
        functions.push(compiled_fn);
    }

    Ok(Program {
        functions,
        entry_indices,
    })
}
