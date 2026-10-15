//! Nudge ByteCode (NBC) representation and compiler for the pure-compute core.
//!
//! Compiles the pure-compute subset of Nudge AST items into bytecode. Any effectful
//! or unsupported construct (e.g. LLM calls, tool calls, computer calls, par map,
//! decide, merge, agent state writes, stream let) is rejected with a clear
//! "not in bytecode subset" error so callers can fall back to the interpreter.

use std::collections::{HashMap, HashSet};

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
    /// Index into list or map: pop index, pop collection, push element or field.
    Index,
    /// Verify local slot contains a List: error if not NudgeVal::List.
    CheckList(u32),
    /// Loop iteration limit check on counter slot: error if counter >= NUDGE_MAX_LOOP_ITERATIONS.
    CheckLoopLimit(u32),
    /// Call user function: `(fn_index, argc)`.
    Call(u32, usize),
    /// Delegate call to effectful function in the interpreter: `(fn_name, argc)`.
    DelegateCall(String, usize),
    /// Delegate evaluation of an effectful expression to the interpreter: `expr_index`.
    DelegateExpr(u32),
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

/// An effectful expression delegated to the interpreter along with the local variable bindings in scope.
#[derive(Debug, Clone, PartialEq)]
pub struct DelegatedExpr {
    pub expr: Expr,
    pub scope: Vec<(String, u32)>,
}

/// A compiled NBC program containing function chunks and an entry index table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub functions: Vec<Function>,
    pub entry_indices: HashMap<String, usize>,
    pub delegated_exprs: Vec<DelegatedExpr>,
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
#[derive(Debug, Clone)]
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
    mut_locals: HashSet<String>,
    next_local: u32,
    fn_map: &'a HashMap<String, usize>,
    fn_metas: &'a [FnMeta],
    effectful_fn_metas: &'a HashMap<String, FnMeta>,
    tool_names: &'a [String],
    delegated_exprs: &'a mut Vec<DelegatedExpr>,
}

impl<'a> Compiler<'a> {
    fn new(
        fn_meta: &FnMeta,
        fn_map: &'a HashMap<String, usize>,
        fn_metas: &'a [FnMeta],
        effectful_fn_metas: &'a HashMap<String, FnMeta>,
        tool_names: &'a [String],
        delegated_exprs: &'a mut Vec<DelegatedExpr>,
    ) -> Self {
        let mut locals = HashMap::new();
        for (i, p) in fn_meta.params.iter().enumerate() {
            locals.insert(p.clone(), i as u32);
        }
        let next_local = fn_meta.params.len() as u32;

        Self {
            chunk: Chunk::new(),
            locals,
            mut_locals: HashSet::new(),
            next_local,
            fn_map,
            fn_metas,
            effectful_fn_metas,
            tool_names,
            delegated_exprs,
        }
    }

    fn emit_delegated_expr(&mut self, expr: &Expr) -> usize {
        let mut scope: Vec<(String, u32)> =
            self.locals.iter().map(|(k, &v)| (k.clone(), v)).collect();
        scope.sort_by_key(|(_, s)| *s);
        let idx = self.delegated_exprs.len() as u32;
        self.delegated_exprs.push(DelegatedExpr {
            expr: expr.clone(),
            scope,
        });
        self.chunk.emit(Op::DelegateExpr(idx))
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
                mutable,
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
                if *mutable {
                    self.mut_locals.insert(name.clone());
                } else {
                    self.mut_locals.remove(name);
                }
                self.chunk.emit(Op::StoreLocal(slot));
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                self.compile_expr(value)?;
                let slot = *self
                    .locals
                    .get(name)
                    .ok_or_else(|| format!("unknown local '{name}'"))?;
                if !self.mut_locals.contains(name) {
                    self.chunk.emit(Op::Fail(format!(
                        "cannot assign to immutable local '{name}'; declare it with `let mut`"
                    )));
                } else {
                    self.chunk.emit(Op::StoreLocal(slot));
                }
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
            StmtKind::For {
                var,
                idx_var,
                coll,
                body,
            } => {
                // 1. Evaluate collection and store in coll_slot
                self.compile_expr(coll)?;
                let coll_slot = self.next_local;
                self.next_local += 1;
                self.chunk.emit(Op::StoreLocal(coll_slot));

                // 2. Check coll is a list
                self.chunk.emit(Op::CheckList(coll_slot));

                // 3. Counter slot initialized to 0
                let counter_slot = self.next_local;
                self.next_local += 1;
                let zero_const = self.chunk.add_constant(NudgeVal::Int(0));
                self.chunk.emit(Op::Const(zero_const));
                self.chunk.emit(Op::StoreLocal(counter_slot));

                // 4. Compute len(coll) and store in len_slot
                let len_slot = self.next_local;
                self.next_local += 1;
                self.chunk.emit(Op::LoadLocal(coll_slot));
                self.chunk.emit(Op::CallBuiltin("len".to_string(), 1));
                self.chunk.emit(Op::StoreLocal(len_slot));

                // 5. Loop header
                let loop_start = self.chunk.code.len();

                // Check condition: counter < len
                self.chunk.emit(Op::LoadLocal(counter_slot));
                self.chunk.emit(Op::LoadLocal(len_slot));
                self.chunk.emit(Op::Lt);
                let jump_exit = self.chunk.emit(Op::BranchIfFalse(0));

                // Check iteration limit
                self.chunk.emit(Op::CheckLoopLimit(counter_slot));

                // Bind loop var = coll[counter]
                let saved_locals = self.locals.clone();
                let saved_mut_locals = self.mut_locals.clone();

                let var_slot = self.next_local;
                self.next_local += 1;
                self.locals.insert(var.clone(), var_slot);
                self.mut_locals.remove(var);
                self.chunk.emit(Op::LoadLocal(coll_slot));
                self.chunk.emit(Op::LoadLocal(counter_slot));
                self.chunk.emit(Op::Index);
                self.chunk.emit(Op::StoreLocal(var_slot));

                if let Some(i_var) = idx_var {
                    let idx_slot = self.next_local;
                    self.next_local += 1;
                    self.locals.insert(i_var.clone(), idx_slot);
                    self.mut_locals.remove(i_var);
                    self.chunk.emit(Op::LoadLocal(counter_slot));
                    self.chunk.emit(Op::StoreLocal(idx_slot));
                }

                // Compile body
                for st in body {
                    self.compile_stmt(st)?;
                }

                // Restore locals scope so loop body locals and loop vars don't leak
                self.locals = saved_locals;
                self.mut_locals = saved_mut_locals;

                // Increment counter: counter = counter + 1
                self.chunk.emit(Op::LoadLocal(counter_slot));
                let one_const = self.chunk.add_constant(NudgeVal::Int(1));
                self.chunk.emit(Op::Const(one_const));
                self.chunk.emit(Op::Add);
                self.chunk.emit(Op::StoreLocal(counter_slot));

                // Backward jump to loop_start
                self.chunk.emit(Op::Jump(loop_start));

                // Patch jump_exit
                let exit_target = self.chunk.code.len();
                self.chunk.code[jump_exit] = Op::BranchIfFalse(exit_target);

                Ok(())
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
                } else if let Some(target_meta) = self.effectful_fn_metas.get(&name) {
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

                    self.chunk.emit(Op::DelegateCall(name, total_params));
                    Ok(())
                } else if self.tool_names.iter().any(|t| t == &name)
                    || name.starts_with("io.")
                    || name.starts_with("workspace.")
                    || name.starts_with("store.")
                    || name.starts_with("task.")
                    || name.starts_with("ui.")
                    || name.ends_with(".open")
                    || name.ends_with(".mount_headless")
                    || name.ends_with(".dispatch_click")
                    || name.ends_with(".find")
                    || name.ends_with(".find_text")
                    || name.ends_with(".exists")
                    || matches!(
                        name.as_str(),
                        "column" | "row" | "text" | "button" | "input"
                    )
                {
                    self.emit_delegated_expr(expr);
                    Ok(())
                } else {
                    Err(format!(
                        "not in bytecode subset: unknown function or effect '{name}'"
                    ))
                }
            }
            ExprKind::Prompt { .. } => {
                self.emit_delegated_expr(expr);
                Ok(())
            }
            ExprKind::LlmCall { .. } => {
                self.emit_delegated_expr(expr);
                Ok(())
            }
            ExprKind::Spawn(_) | ExprKind::Block(_) => {
                self.emit_delegated_expr(expr);
                Ok(())
            }
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
            ExprKind::RecordLit(fields) => {
                let mut keys = Vec::with_capacity(fields.len());
                for (name, expr) in fields {
                    self.compile_expr(expr)?;
                    keys.push(name.clone());
                }
                self.chunk.emit(Op::MakeRecord(keys));
                Ok(())
            }
            ExprKind::Index { obj, idx } => {
                self.compile_expr(obj)?;
                self.compile_expr(idx)?;
                self.chunk.emit(Op::Index);
                Ok(())
            }
            ExprKind::If { cond, then, else_ } => {
                self.compile_expr(cond)?;
                let jump_else = self.chunk.emit(Op::BranchIfFalse(0));
                self.compile_expr(then)?;
                let jump_end = self.chunk.emit(Op::Jump(0));
                let else_target = self.chunk.code.len();
                self.chunk.code[jump_else] = Op::BranchIfFalse(else_target);
                if let Some(el) = else_ {
                    self.compile_expr(el)?;
                } else {
                    self.chunk.emit(Op::Unit);
                }
                let end_target = self.chunk.code.len();
                self.chunk.code[jump_end] = Op::Jump(end_target);
                Ok(())
            }
        }
    }
}

fn check_fn_effects(
    name: &str,
    effects: &[String],
    body: &[Stmt],
    tool_names: &[String],
) -> Result<(), String> {
    if !effects.is_empty() {
        return Err(format!(
            "not in bytecode subset: function '{name}' declares effects: {effects:?}"
        ));
    }
    for stmt in body {
        check_stmt_effects(stmt, tool_names)?;
    }
    Ok(())
}

fn check_stmt_effects(stmt: &Stmt, tool_names: &[String]) -> Result<(), String> {
    match &stmt.kind {
        StmtKind::Let { stream, value, .. } => {
            if *stream {
                return Err("not in bytecode subset: stream let".to_string());
            }
            check_expr_effects(value, tool_names)
        }
        StmtKind::Assign { value, .. } => check_expr_effects(value, tool_names),
        StmtKind::ExprStmt(expr) | StmtKind::Assert(expr) => check_expr_effects(expr, tool_names),
        StmtKind::StateWrite { .. } => {
            Err("not in bytecode subset: state write (agent state)".to_string())
        }
        StmtKind::ForAll { .. } => Err("not in bytecode subset: for_all property loop".to_string()),
        StmtKind::For { coll, body, .. } => {
            check_expr_effects(coll, tool_names)?;
            for stmt in body {
                check_stmt_effects(stmt, tool_names)?;
            }
            Ok(())
        }
    }
}

fn check_expr_effects(expr: &Expr, tool_names: &[String]) -> Result<(), String> {
    match &expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Money(_, _)
        | ExprKind::Bool(_)
        | ExprKind::None => Ok(()),
        ExprKind::Ident(name) => {
            if name == "state" {
                return Err("not in bytecode subset: agent state read".to_string());
            }
            Ok(())
        }
        ExprKind::ListLit(items) => {
            for item in items {
                check_expr_effects(item, tool_names)?;
            }
            Ok(())
        }
        ExprKind::Field { obj, .. } => {
            if let ExprKind::Ident(id) = &obj.kind {
                if id == "state" {
                    return Err("not in bytecode subset: agent state read".to_string());
                }
            }
            check_expr_effects(obj, tool_names)
        }
        ExprKind::Unary { x, .. } => check_expr_effects(x, tool_names),
        ExprKind::Binary { l, r, .. } => {
            check_expr_effects(l, tool_names)?;
            check_expr_effects(r, tool_names)
        }
        ExprKind::Call { func, args, kwargs } => {
            let callee_name = match &func.kind {
                ExprKind::Ident(n) => Some(n.as_str()),
                ExprKind::Field { obj, .. } => {
                    if let ExprKind::Ident(prefix) = &obj.kind {
                        if prefix == "computer" {
                            return Err("not in bytecode subset: computer call".to_string());
                        }
                    }
                    None
                }
                _ => None,
            };
            if let Some(name) = callee_name {
                if tool_names.iter().any(|t| t == name)
                    || name.starts_with("io.")
                    || name.starts_with("workspace.")
                    || name.starts_with("store.")
                    || name.starts_with("task.")
                    || name.starts_with("ui.")
                    || name.ends_with(".open")
                    || name.ends_with(".mount_headless")
                    || name.ends_with(".dispatch_click")
                    || name.ends_with(".find")
                    || name.ends_with(".find_text")
                    || name.ends_with(".exists")
                    || matches!(name, "column" | "row" | "text" | "button" | "input")
                {
                    // Tool call / IO / workspace / store / UI expressions are delegated to the interpreter via DelegateExpr.
                    return Ok(());
                }
            }
            check_expr_effects(func, tool_names)?;
            for a in args {
                check_expr_effects(a, tool_names)?;
            }
            for (_, v) in kwargs {
                check_expr_effects(v, tool_names)?;
            }
            Ok(())
        }
        ExprKind::Prompt { .. } => {
            // Prompt literal expressions are delegated to the interpreter via DelegateExpr.
            Ok(())
        }
        ExprKind::LlmCall { .. } => {
            // Inline LLM call expressions are delegated to the interpreter via DelegateExpr.
            Ok(())
        }
        ExprKind::ComputerCall { .. } => Err("not in bytecode subset: computer call".to_string()),
        ExprKind::ParMap { .. } => Err("not in bytecode subset: par map".to_string()),
        ExprKind::ParAll(_) => Err("not in bytecode subset: par all".to_string()),
        ExprKind::ParRace(_) => Err("not in bytecode subset: par race".to_string()),
        ExprKind::Merge { .. } => Err("not in bytecode subset: merge".to_string()),
        ExprKind::Route { arms } => {
            for (_label, val_expr, cond_expr) in arms {
                if let Some(cond) = cond_expr {
                    check_expr_effects(cond, tool_names)?;
                }
                check_expr_effects(val_expr, tool_names)?;
            }
            Ok(())
        }
        ExprKind::DecideCall { .. } => Err("not in bytecode subset: decide call".to_string()),
        ExprKind::RecordLit(fields) => {
            for (_, v) in fields {
                check_expr_effects(v, tool_names)?;
            }
            Ok(())
        }
        ExprKind::Index { obj, idx } => {
            check_expr_effects(obj, tool_names)?;
            check_expr_effects(idx, tool_names)
        }
        ExprKind::If { cond, then, else_ } => {
            check_expr_effects(cond, tool_names)?;
            check_expr_effects(then, tool_names)?;
            if let Some(el) = else_ {
                check_expr_effects(el, tool_names)?;
            }
            Ok(())
        }
        ExprKind::Spawn(_) => Ok(()),
        ExprKind::Block(stmts) => {
            for s in stmts {
                match &s.kind {
                    StmtKind::Let { value, .. }
                    | StmtKind::Assign { value, .. }
                    | StmtKind::StateWrite { value, .. } => {
                        check_expr_effects(value, tool_names)?;
                    }
                    StmtKind::Assert(e) | StmtKind::ExprStmt(e) => {
                        check_expr_effects(e, tool_names)?;
                    }
                    _ => {}
                }
            }
            Ok(())
        }
    }
}

/// Compiles pure Nudge AST items into a runnable `Program`.
///
/// Returns an error if the entry function or unsupported construct is encountered.
pub fn compile(items: &[ast::Item]) -> Result<Program, String> {
    compile_internal(items, None)
}

/// Compiles pure Nudge AST items into a runnable `Program` targeting a specific entry function.
///
/// If `entry` itself is effectful, returns an error so caller can fall back to interpreter.
pub fn compile_with_entry(items: &[ast::Item], entry: &str) -> Result<Program, String> {
    compile_internal(items, Some(entry))
}

fn compile_internal(items: &[ast::Item], expected_entry: Option<&str>) -> Result<Program, String> {
    let mut tool_names = Vec::new();

    // First pass: validate agent and collect tool names
    for item in items {
        match item {
            Item::Tool { name, .. } => {
                tool_names.push(name.clone());
            }
            Item::Agent { name, .. } => {
                return Err(format!(
                    "not in bytecode subset: agent declaration '{name}'"
                ));
            }
            _ => {}
        }
    }

    let mut pure_fn_metas = Vec::new();
    let mut fn_map = HashMap::new();
    let mut effectful_fn_metas = HashMap::new();
    let mut effectful_fn_reasons = HashMap::new();
    let mut first_effectful_reason = None;

    for item in items {
        if let Item::Fn {
            name,
            params,
            ret: _,
            effects,
            body,
        } = item
        {
            let meta = FnMeta {
                name: name.clone(),
                params: params.iter().map(|p| p.name.clone()).collect(),
                param_types: params.iter().map(|p| p.ty.to_string()).collect(),
                body: body.clone(),
            };
            match check_fn_effects(name, effects, body, &tool_names) {
                Ok(()) => {
                    let idx = pure_fn_metas.len();
                    fn_map.insert(name.clone(), idx);
                    pure_fn_metas.push(meta);
                }
                Err(reason) => {
                    if first_effectful_reason.is_none() {
                        first_effectful_reason = Some(reason.clone());
                    }
                    effectful_fn_reasons.insert(name.clone(), reason);
                    effectful_fn_metas.insert(name.clone(), meta);
                }
            }
        }
    }

    let target_entry = match expected_entry {
        Some(e) => Some(e),
        None => {
            if fn_map.contains_key("main") || effectful_fn_reasons.contains_key("main") {
                Some("main")
            } else if pure_fn_metas.len() + effectful_fn_metas.len() == 1 {
                if let Some(m) = pure_fn_metas.first() {
                    Some(m.name.as_str())
                } else {
                    effectful_fn_metas.keys().next().map(|s| s.as_str())
                }
            } else {
                None
            }
        }
    };

    if let Some(entry) = target_entry {
        if let Some(err) = effectful_fn_reasons.get(entry) {
            return Err(err.clone());
        }
    }

    if pure_fn_metas.is_empty() {
        if let Some(err) = first_effectful_reason {
            return Err(err);
        }
    }

    // Second pass: compile pure function bodies
    let mut functions = Vec::with_capacity(pure_fn_metas.len());
    let mut entry_indices = HashMap::new();
    let mut delegated_exprs = Vec::new();

    for (idx, meta) in pure_fn_metas.iter().enumerate() {
        let compiler = Compiler::new(
            meta,
            &fn_map,
            &pure_fn_metas,
            &effectful_fn_metas,
            &tool_names,
            &mut delegated_exprs,
        );
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
        delegated_exprs,
    })
}
