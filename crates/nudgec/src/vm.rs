//! Nudge ByteCode (NBC) virtual machine for the pure-compute core.
//!
//! Executes compiled `Program`s using a stack-based runtime with exact semantic
//! parity against `nudgec::eval::Interpreter`. Reuses `NudgeVal` directly so that
//! evaluation results and parity checks are exact.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::bytecode::{Op, Program};
use crate::eval::NudgeVal;

/// A request to delegate execution to the interpreter.
pub enum DelegateRequest<'a> {
    /// Delegate an effectful named function call: `(callee_name, args)`.
    Call(&'a str, Vec<NudgeVal>),
    /// Delegate an effectful expression evaluation: `(expr, locals)`.
    Expr(&'a Expr, &'a [(String, NudgeVal)]),
}

/// Hook signature for delegating effectful operations to the interpreter.
pub type DelegateHook<'a> = dyn FnMut(DelegateRequest<'_>) -> Result<NudgeVal, String> + 'a;

/// Maximum call frame depth before halting with a stack overflow error.
///
/// The VM uses an explicit heap-allocated frame stack, so it recurses ~8x
/// deeper than the AST interpreter (which recurses on the OS stack and caps
/// at ~1,250 frames before a hard SIGABRT crash). Exceeding this limit fails
/// CLEANLY — a catchable `call stack overflow` error (exit 1) rather than
/// crashing the process.
const MAX_CALL_FRAMES: usize = 10_000;

/// An activation frame on the call stack.
#[derive(Debug, Clone)]
struct CallFrame {
    fn_idx: usize,
    ip: usize,
    stack_base: usize,
}

/// The NBC stack-based virtual machine.
pub struct VM<'a> {
    program: &'a Program,
    stack: Vec<NudgeVal>,
    frames: Vec<CallFrame>,
}

impl<'a> VM<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            stack: Vec::with_capacity(64),
            frames: Vec::with_capacity(16),
        }
    }

    /// Run the entry function with the provided arguments.
    pub fn execute(
        &mut self,
        entry: &str,
        args: &[NudgeVal],
        delegate: &mut DelegateHook<'_>,
    ) -> Result<NudgeVal, String> {
        let entry_idx = match self.program.entry_indices.get(entry) {
            Some(&idx) => idx,
            None => {
                if entry == "main" {
                    return Err("no entry point: program has no `main` function. \
                         Define `fn main()` or pass --fn <function> to choose an entry."
                        .to_string());
                } else {
                    return Err(format!("function '{entry}' not found in program"));
                }
            }
        };

        let entry_fn = &self.program.functions[entry_idx];
        if args.len() != entry_fn.params.len() {
            let expected = entry_fn.params.len();
            let s = if expected == 1 {
                "argument"
            } else {
                "arguments"
            };
            let param_str = if entry_fn.params.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    entry_fn
                        .params
                        .iter()
                        .zip(&entry_fn.param_types)
                        .map(|(p, ty)| format!("{p}: {ty}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            return Err(format!(
                "entry '{entry}' expects {expected} {s}{param_str}, got {}",
                args.len()
            ));
        }

        // Initialize stack with entry arguments and reserve space for all locals
        for a in args {
            self.stack.push(a.clone());
        }
        while self.stack.len() < entry_fn.local_count {
            self.stack.push(NudgeVal::Unit);
        }

        self.frames.push(CallFrame {
            fn_idx: entry_idx,
            ip: 0,
            stack_base: 0,
        });

        self.run_loop(delegate)
    }

    fn run_loop(&mut self, delegate: &mut DelegateHook<'_>) -> Result<NudgeVal, String> {
        loop {
            if self.frames.is_empty() {
                return Ok(self.stack.pop().unwrap_or(NudgeVal::Unit));
            }

            let frame_idx = self.frames.len() - 1;
            let fn_idx = self.frames[frame_idx].fn_idx;
            let ip = self.frames[frame_idx].ip;
            let chunk = &self.program.functions[fn_idx].chunk;

            if ip >= chunk.code.len() {
                let ret_val = self.stack.pop().unwrap_or(NudgeVal::Unit);
                let base = self.frames[frame_idx].stack_base;
                self.stack.truncate(base);
                let frame = self.frames.pop().unwrap();
                let fn_name = &self.program.functions[frame.fn_idx].name;
                crate::eval::interpreter::emit_fn_return(fn_name, &ret_val)?;
                if self.frames.is_empty() {
                    return Ok(ret_val);
                }
                self.stack.push(ret_val);
                continue;
            }

            let op = &chunk.code[ip];
            self.frames[frame_idx].ip += 1;

            match op {
                Op::Const(idx) => {
                    let val = chunk.constants[*idx as usize].clone();
                    self.stack.push(val);
                }
                Op::Unit => {
                    self.stack.push(NudgeVal::Unit);
                }
                Op::True => {
                    self.stack.push(NudgeVal::Bool(true));
                }
                Op::False => {
                    self.stack.push(NudgeVal::Bool(false));
                }
                Op::LoadLocal(slot) => {
                    let base = self.frames[frame_idx].stack_base;
                    let val = self.stack[base + *slot as usize].clone();
                    self.stack.push(val);
                }
                Op::StoreLocal(slot) => {
                    let base = self.frames[frame_idx].stack_base;
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    self.stack[base + *slot as usize] = val;
                }
                Op::Pop => {
                    self.stack.pop().ok_or("stack underflow")?;
                }
                Op::ToBool => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    self.stack.push(NudgeVal::Bool(val.is_truthy()));
                }
                Op::Jump(target) => {
                    self.frames[frame_idx].ip = *target;
                }
                Op::JumpIfTrue(target) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if val.is_truthy() {
                        self.stack.push(NudgeVal::Bool(true));
                        self.frames[frame_idx].ip = *target;
                    }
                }
                Op::JumpIfFalse(target) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if !val.is_truthy() {
                        self.stack.push(NudgeVal::Bool(false));
                        self.frames[frame_idx].ip = *target;
                    }
                }
                Op::BranchIfFalse(target) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if !val.is_truthy() {
                        self.frames[frame_idx].ip = *target;
                    }
                }
                Op::Neg => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    match val {
                        NudgeVal::Int(i) => self.stack.push(NudgeVal::Int(-i)),
                        NudgeVal::Float(f) => self.stack.push(NudgeVal::Float(-f)),
                        other => return Err(format!("cannot negate {}", other.type_name())),
                    }
                }
                Op::Not => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    self.stack.push(NudgeVal::Bool(!val.is_truthy()));
                }
                Op::Add => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Int(a + b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a + b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a as f64 + b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Float(a + b as f64))
                        }
                        (NudgeVal::Str(a), NudgeVal::Str(b)) => {
                            self.stack.push(NudgeVal::Str(format!("{a}{b}")))
                        }
                        (NudgeVal::List(mut a), NudgeVal::List(b)) => {
                            a.extend(b);
                            self.stack.push(NudgeVal::List(a));
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot add {} and {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::Sub => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Int(a - b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a - b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a as f64 - b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Float(a - b as f64))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot subtract {} from {}",
                                b.type_name(),
                                a.type_name()
                            ))
                        }
                    }
                }
                Op::Mul => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Int(a * b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a * b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a as f64 * b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Float(a * b as f64))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot multiply {} and {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::Div => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            if b == 0 {
                                return Err("division by zero".into());
                            }
                            self.stack.push(NudgeVal::Float(a as f64 / b as f64));
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a / b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Float(a as f64 / b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Float(a / b as f64))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot divide {} by {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::Mod => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            if b == 0 {
                                return Err("modulo by zero".into());
                            }
                            self.stack.push(NudgeVal::Int(a % b));
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot modulo {} and {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::Eq => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    self.stack.push(NudgeVal::Bool(left.loose_eq(&right)));
                }
                Op::NotEq => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    self.stack.push(NudgeVal::Bool(!left.loose_eq(&right)));
                }
                Op::Lt => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a < b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool(a < b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool((a as f64) < b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a < (b as f64)))
                        }
                        (NudgeVal::Str(a), NudgeVal::Str(b)) => {
                            self.stack.push(NudgeVal::Bool(a < b))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot compare {} < {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::LtEq => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a <= b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool(a <= b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool((a as f64) <= b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a <= (b as f64)))
                        }
                        (NudgeVal::Str(a), NudgeVal::Str(b)) => {
                            self.stack.push(NudgeVal::Bool(a <= b))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot compare {} <= {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::Gt => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a > b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool(a > b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool((a as f64) > b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a > (b as f64)))
                        }
                        (NudgeVal::Str(a), NudgeVal::Str(b)) => {
                            self.stack.push(NudgeVal::Bool(a > b))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot compare {} > {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::GtEq => {
                    let right = self.stack.pop().ok_or("stack underflow")?;
                    let left = self.stack.pop().ok_or("stack underflow")?;
                    match (left, right) {
                        (NudgeVal::Int(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a >= b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool(a >= b))
                        }
                        (NudgeVal::Int(a), NudgeVal::Float(b)) => {
                            self.stack.push(NudgeVal::Bool((a as f64) >= b))
                        }
                        (NudgeVal::Float(a), NudgeVal::Int(b)) => {
                            self.stack.push(NudgeVal::Bool(a >= (b as f64)))
                        }
                        (NudgeVal::Str(a), NudgeVal::Str(b)) => {
                            self.stack.push(NudgeVal::Bool(a >= b))
                        }
                        (a, b) => {
                            return Err(format!(
                                "cannot compare {} >= {}",
                                a.type_name(),
                                b.type_name()
                            ))
                        }
                    }
                }
                Op::MakeList(n) => {
                    let mut list = Vec::with_capacity(*n);
                    for _ in 0..*n {
                        list.push(self.stack.pop().ok_or("stack underflow")?);
                    }
                    list.reverse();
                    self.stack.push(NudgeVal::List(list));
                }
                Op::MakeRecord(keys) => {
                    let mut map = HashMap::new();
                    for k in keys.iter().rev() {
                        let v = self.stack.pop().ok_or("stack underflow")?;
                        map.insert(k.clone(), v);
                    }
                    self.stack.push(NudgeVal::Map(map));
                }
                Op::Field(name) => {
                    let obj = self.stack.pop().ok_or("stack underflow")?;
                    match obj {
                        NudgeVal::Map(m) => {
                            let val = m
                                .get(name)
                                .cloned()
                                .ok_or_else(|| format!("map has no key '{name}'"))?;
                            self.stack.push(val);
                        }
                        other => {
                            return Err(format!(
                                "cannot access field '{name}' on type {}",
                                other.type_name()
                            ))
                        }
                    }
                }
                Op::Assert(expr_str) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if !val.is_truthy() {
                        return Err(format!("assertion failed: {expr_str} evaluated to {val}"));
                    }
                }
                Op::Fail(msg) => {
                    return Err(msg.clone());
                }
                Op::CallBuiltin(name, argc) => {
                    if name == "len" {
                        if *argc != 1 {
                            return Err(format!("'len' takes 1 argument, got {argc}"));
                        }
                        let val = self.stack.pop().ok_or("stack underflow")?;
                        let count = match val {
                            NudgeVal::Str(s) => s.chars().count() as i64,
                            NudgeVal::List(l) => l.len() as i64,
                            NudgeVal::Map(m) => m.len() as i64,
                            other => {
                                return Err(format!(
                                    "'len' not supported for type {}",
                                    other.type_name()
                                ))
                            }
                        };
                        self.stack.push(NudgeVal::Int(count));
                    } else if name == "zip" {
                        if *argc != 2 {
                            return Err(format!("'zip' takes 2 arguments, got {argc}"));
                        }
                        let b = self.stack.pop().ok_or("stack underflow")?;
                        let a = self.stack.pop().ok_or("stack underflow")?;
                        match (a, b) {
                            (NudgeVal::List(l1), NudgeVal::List(l2)) => {
                                let mut zipped = Vec::new();
                                for (x, y) in l1.into_iter().zip(l2) {
                                    let mut rec = HashMap::new();
                                    rec.insert("first".to_string(), x);
                                    rec.insert("second".to_string(), y);
                                    zipped.push(NudgeVal::Map(rec));
                                }
                                self.stack.push(NudgeVal::List(zipped));
                            }
                            _ => return Err("'zip' arguments must be lists".into()),
                        }
                    } else {
                        return Err(format!("unsupported builtin '{name}'"));
                    }
                }
                Op::Call(fn_idx, argc) => {
                    if self.frames.len() >= MAX_CALL_FRAMES {
                        return Err("call stack overflow".into());
                    }
                    let callee = &self.program.functions[*fn_idx as usize];
                    let new_base = self.stack.len() - *argc;
                    while self.stack.len() < new_base + callee.local_count {
                        self.stack.push(NudgeVal::Unit);
                    }
                    self.frames.push(CallFrame {
                        fn_idx: *fn_idx as usize,
                        ip: 0,
                        stack_base: new_base,
                    });
                }
                Op::DelegateCall(name, argc) => {
                    let argc = *argc;
                    let start = self
                        .stack
                        .len()
                        .checked_sub(argc)
                        .ok_or_else(|| "stack underflow in DelegateCall".to_string())?;
                    let args: Vec<NudgeVal> = self.stack.drain(start..).collect();
                    let res = delegate(DelegateRequest::Call(name, args))?;
                    self.stack.push(res);
                }
                Op::DelegateExpr(idx) => {
                    let delegated = self
                        .program
                        .delegated_exprs
                        .get(*idx as usize)
                        .ok_or_else(|| format!("invalid delegated expr index {idx}"))?;
                    let base = self.frames[frame_idx].stack_base;
                    let mut locals = Vec::with_capacity(delegated.scope.len());
                    for (name, slot) in &delegated.scope {
                        let val = self
                            .stack
                            .get(base + *slot as usize)
                            .cloned()
                            .unwrap_or(NudgeVal::Unit);
                        locals.push((name.clone(), val));
                    }
                    let res = delegate(DelegateRequest::Expr(&delegated.expr, &locals))?;
                    self.stack.push(res);
                }
                Op::Return => {
                    let ret_val = self.stack.pop().unwrap_or(NudgeVal::Unit);
                    let current_base = self.frames[frame_idx].stack_base;
                    self.stack.truncate(current_base);
                    let frame = self.frames.pop().unwrap();
                    let fn_name = &self.program.functions[frame.fn_idx].name;
                    crate::eval::interpreter::emit_fn_return(fn_name, &ret_val)?;
                    if self.frames.is_empty() {
                        return Ok(ret_val);
                    }
                    self.stack.push(ret_val);
                }
            }
        }
    }
}

/// Executes a compiled `Program` starting at `entry` with `args`.
pub fn run(program: &Program, entry: &str, args: &[NudgeVal]) -> Result<NudgeVal, String> {
    let mut noop = |req: DelegateRequest<'_>| -> Result<NudgeVal, String> {
        match req {
            DelegateRequest::Call(name, _) => Err(format!(
                "no delegate configured to execute effectful function '{name}'"
            )),
            DelegateRequest::Expr(_, _) => {
                Err("no delegate configured to execute effectful expression".to_string())
            }
        }
    };
    let mut vm = VM::new(program);
    vm.execute(entry, args, &mut noop)
}

/// Executes a compiled `Program` starting at `entry` with `args`, delegating effectful
/// function calls to `delegate`.
pub fn run_with_delegate(
    program: &Program,
    entry: &str,
    args: &[NudgeVal],
    delegate: &mut DelegateHook<'_>,
) -> Result<NudgeVal, String> {
    let mut vm = VM::new(program);
    vm.execute(entry, args, delegate)
}

#[cfg(all(test, feature = "compiler"))]
mod tests {
    use super::*;
    use crate::bytecode::compile;
    use crate::eval::run_program;
    use crate::lexer;
    use crate::parser;

    fn parse_src(src: &str) -> Vec<crate::ast::Item> {
        let tokens = lexer::lex(src).expect("lexing failed");
        parser::parse(tokens).expect("parsing failed")
    }

    /// Differential parity harness: asserts that the interpreter and the NBC bytecode VM
    /// produce identical outputs for the given program.
    fn assert_parity(src: &str, entry: &str, args: &[NudgeVal]) {
        let items = parse_src(src);
        let interp_res = run_program(&items, entry, args);
        let program = compile(&items).expect("compilation to bytecode failed");
        let vm_res = run(&program, entry, args);
        assert_eq!(
            interp_res, vm_res,
            "Parity mismatch between interpreter and VM for source:\n{src}"
        );
    }

    /// Differential error parity harness: asserts that both interpreter and VM fail with
    /// the exact same error message.
    fn assert_parity_error(src: &str, entry: &str, args: &[NudgeVal]) {
        let items = parse_src(src);
        let interp_err =
            run_program(&items, entry, args).expect_err("interpreter should fail on this program");
        let program = compile(&items).expect("compilation to bytecode should succeed");
        let vm_err = run(&program, entry, args).expect_err("VM should fail on this program");
        assert_eq!(
            interp_err, vm_err,
            "Error parity mismatch between interpreter and VM for source:\n{src}"
        );
    }

    // ------------------------------------------------------------------------
    // Arithmetic, Numbers, and Precedence
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_simple_add() {
        let src = r#"
            fn add(a: int, b: int) -> int {
                a + b
            }
            fn main() -> int {
                add(10, 32)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_arithmetic_all_ops() {
        let src = r#"
            fn main() -> float {
                let a = 100
                let b = 25
                let c = 4
                let d = (a + b * c - 50) / 10 % 7
                d + 0.5
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_float_arithmetic() {
        let src = r#"
            fn main() -> float {
                let x = 3.14159
                let y = 2.71828
                let res = (x * y) - (x / y) + 1.5
                res
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_int_float_ops() {
        let src = r#"
            fn main() -> float {
                let i = 10
                let f = 2.5
                let sum = i + f
                let diff = f - i
                let prod = i * f
                let div = i / f
                sum + diff + prod + div
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_negation_and_unary() {
        let src = r#"
            fn main() -> int {
                let x = 42
                let y = -x
                let z = -y
                z + -10
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_big_ints() {
        let src = r#"
            fn main() -> int {
                let big = 9000000000000000000
                let delta = 123456789
                big + delta - delta
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_money_literals() {
        let src = r#"
            fn main() -> float {
                let price = 99.95 USD
                let tax = 0.05 USD
                let discount = 10.0
                price + tax - discount
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Booleans, Truthiness, and Short-Circuiting
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_bool_ops() {
        let src = r#"
            fn main() -> bool {
                let a = true
                let b = false
                let res = (!a or b) == false and (!b and a) == true
                res
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_short_circuit_and() {
        // If left is false, division by zero on the right must NOT be evaluated
        let src = r#"
            fn main() -> bool {
                let safe = false and (1 / 0 == 0)
                safe
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_short_circuit_or() {
        // If left is true, division by zero on the right must NOT be evaluated
        let src = r#"
            fn main() -> bool {
                let safe = true or (1 / 0 == 0)
                safe
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_truthiness_types() {
        let src = r#"
            fn main() -> bool {
                let t_int = !0
                let f_int = !1
                let t_str = !"hello"
                let f_str = !""
                let t_list = ![1]
                let f_list = ![]
                t_int and !f_int and !t_str and f_str and !t_list and f_list
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Comparisons
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_loose_equality() {
        let src = r#"
            fn main() -> bool {
                let eq1 = (42 == 42.0)
                let eq2 = (42.0 == 42)
                let neq = (42 != 43.0)
                eq1 and eq2 and neq
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_order_comparisons() {
        let src = r#"
            fn main() -> bool {
                let c1 = 10 < 20
                let c2 = 20 <= 20
                let c3 = 30 > 25
                let c4 = 30 >= 30
                let c5 = 5.5 < 10
                let c6 = 10 > 5.5
                c1 and c2 and c3 and c4 and c5 and c6
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Strings
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_string_concat_and_cmp() {
        let src = r#"
            fn main() -> string {
                let a = "Hello, "
                let b = "world!"
                let c = a + b
                let is_smaller = "apple" < "banana"
                assert is_smaller
                c
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_len_builtin() {
        let src = r#"
            fn main() -> int {
                let s_len = len("hello world")
                let l_len = len([10, 20, 30, 40])
                s_len + l_len
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Lists and Zip
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_lists_and_concat() {
        let src = r#"
            fn main() -> [int] {
                let a = [1, 2, 3]
                let b = [4, 5]
                a + b
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_zip_and_fields() {
        let src = r#"
            fn main() -> int {
                let pairs = zip([10, 20], [100, 200])
                let pair_count = len(pairs)
                pair_count
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_field_access() {
        let src = r#"
            fn extract(record: {first: int, second: int}) -> int {
                record.first + record.second
            }
        "#;
        let mut map = std::collections::HashMap::new();
        map.insert("first".to_string(), NudgeVal::Int(40));
        map.insert("second".to_string(), NudgeVal::Int(2));
        assert_parity(src, "extract", &[NudgeVal::Map(map)]);
    }

    #[test]
    fn test_parity_field_access_missing_key_error() {
        let src = r#"
            fn extract(record: {first: int}) -> int {
                record.second
            }
        "#;
        let mut map = std::collections::HashMap::new();
        map.insert("first".to_string(), NudgeVal::Int(42));
        assert_parity_error(src, "extract", &[NudgeVal::Map(map)]);
    }

    #[test]
    fn test_parity_field_access_non_map_error() {
        let src = r#"
            fn extract(val: int) -> int {
                val.first
            }
        "#;
        assert_parity_error(src, "extract", &[NudgeVal::Int(42)]);
    }

    // ------------------------------------------------------------------------
    // Scoping, Shadowing, and Return
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_variable_shadowing() {
        let src = r#"
            fn main() -> int {
                let x = 10
                let y = x + 5
                let x = x * 2
                x + y
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_unit_return() {
        let src = r#"
            fn do_asserts(n: int) -> () {
                assert n > 0
                let dummy = n + 1
            }
            fn main() -> () {
                do_asserts(5)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Assert Statement
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_assert_success() {
        let src = r#"
            fn main() -> int {
                let x = 100
                assert x > 50
                assert x == 100
                x
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_assert_failure_exact_message() {
        let src = r#"
            fn main() -> int {
                let x = 10
                assert x > 100
                x
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Functions, Calls, Kwargs, and Recursion
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_multi_function_calls() {
        let src = r#"
            fn square(n: int) -> int {
                n * n
            }
            fn sum_squares(a: int, b: int) -> int {
                square(a) + square(b)
            }
            fn main() -> int {
                sum_squares(3, 4)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_kwargs_reordering() {
        let src = r#"
            fn calc(base: int, mult: int, add: int) -> int {
                base * mult + add
            }
            fn main() -> int {
                calc(10, add=5, mult=3)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_recursion_countdown() {
        let src = r#"
            fn countdown(n: int) -> bool {
                (n <= 0) or countdown(n - 1)
            }
            fn main() -> bool {
                countdown(15)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_recursion_is_even() {
        let src = r#"
            fn is_even(n: int) -> bool {
                (n == 0) or ((n != 1) and is_even(n - 2))
            }
            fn main() -> bool {
                is_even(10) and !is_even(11)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_entry_with_cli_args() {
        let src = r#"
            fn compute(x: int, y: int) -> int {
                x * 2 + y
            }
        "#;
        assert_parity(src, "compute", &[NudgeVal::Int(21), NudgeVal::Int(5)]);
    }

    // ------------------------------------------------------------------------
    // Route Expression Parity
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_route_single_arm_matched() {
        let src = r#"
            fn check(x: int) -> string {
                route {
                    only: "matched" when x == 42
                }
            }
            fn main() -> string {
                check(42)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_single_arm_otherwise() {
        let src = r#"
            fn fallback_only(x: int) -> int {
                route {
                    def: x * 2 otherwise
                }
            }
            fn main() -> int {
                fallback_only(21)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_multi_arm_first_match_wins() {
        let src = r#"
            fn classify(x: int) -> string {
                route {
                    first: "first" when x > 0,
                    second: "second" when x > 5,
                    third: "third" when x > 10,
                    fallback: "other" otherwise
                }
            }
            fn main() -> [string] {
                [classify(15), classify(7), classify(2), classify(-1)]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_multi_arm_ordered() {
        let src = r#"
            fn tier(score: int) -> string {
                route {
                    gold: "gold" when score >= 90,
                    silver: "silver" when score >= 75,
                    bronze: "bronze" when score >= 50,
                    fail: "fail" otherwise
                }
            }
            fn main() -> [string] {
                [tier(95), tier(80), tier(60), tier(40)]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_otherwise_fallback() {
        let src = r#"
            fn default_when_no_match(x: int) -> int {
                route {
                    neg: -1 when x < 0,
                    pos: 1 when x > 0,
                    zero: 0 otherwise
                }
            }
            fn main() -> [int] {
                [default_when_no_match(10), default_when_no_match(-5), default_when_no_match(0)]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_recursive_fib() {
        let src = r#"
            fn fib(n: int) -> int {
                route {
                    base: n when n <= 1,
                    rec: fib(n - 1) + fib(n - 2) otherwise
                }
            }
            fn main() -> int {
                fib(10)
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_compute_ndg_suite() {
        let src = r#"
            fn fib(n: int) -> int {
                route {
                    base: n when n <= 1,
                    rec: fib(n - 1) + fib(n - 2) otherwise
                }
            }

            fn tak(x: int, y: int, z: int) -> int {
                route {
                    base: y when x <= y,
                    rec: tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y)) otherwise
                }
            }

            fn has_factor(n: int, d: int) -> bool {
                route {
                    no: false when d * d > n,
                    yes: true when n % d == 0,
                    rec: has_factor(n, d + 1) otherwise
                }
            }

            fn is_prime(n: int) -> bool {
                route {
                    small: false when n < 2,
                    check: !has_factor(n, 2) otherwise
                }
            }

            fn count_primes(cur: int, max_val: int, acc: int) -> int {
                route {
                    done: acc when cur > max_val,
                    prime: count_primes(cur + 1, max_val, acc + 1) when is_prime(cur),
                    composite: count_primes(cur + 1, max_val, acc) otherwise
                }
            }

            fn main() -> [int] {
                let f = fib(12)
                let t = tak(8, 4, 2)
                let p = count_primes(2, 30, 0)
                [f, t, p]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_nested_expression() {
        let src = r#"
            fn compute(x: int) -> int {
                10 + (route {
                    a: 5 when x > 0,
                    b: 0 otherwise
                }) * 2
            }
            fn main() -> [int] {
                [compute(5), compute(-1)]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_truthiness_conditions() {
        let src = r#"
            fn check_vals(n: int, s: string, xs: [int]) -> [string] {
                let r1 = route {
                    yes: "truthy_int" when n,
                    no: "falsy_int" otherwise
                }
                let r2 = route {
                    yes: "truthy_str" when s,
                    no: "falsy_str" otherwise
                }
                let r3 = route {
                    yes: "truthy_list" when xs,
                    no: "falsy_list" otherwise
                }
                [r1, r2, r3]
            }
            fn main() -> [[string]] {
                [
                    check_vals(1, "hello", [1, 2]),
                    check_vals(0, "", [])
                ]
            }
        "#;
        assert_parity(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Errors & Edge Cases Parity
    // ------------------------------------------------------------------------

    #[test]
    fn test_parity_division_by_zero_exact_error() {
        let src = r#"
            fn main() -> int {
                let zero = 0
                42 / zero
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    #[test]
    fn test_parity_modulo_by_zero_exact_error() {
        let src = r#"
            fn main() -> int {
                100 % 0
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    #[test]
    fn test_parity_arity_mismatch_error() {
        let src = r#"
            fn main(a: int, b: int) -> int {
                a + b
            }
        "#;
        // Call main with 1 argument instead of 2
        assert_parity_error(src, "main", &[NudgeVal::Int(10)]);
    }

    #[test]
    fn test_parity_missing_main_error() {
        let src = r#"
            fn helper() -> int {
                42
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    #[test]
    fn test_parity_unknown_entry_error() {
        let src = r#"
            fn main() -> int {
                42
            }
        "#;
        assert_parity_error(src, "nonexistent", &[]);
    }

    #[test]
    fn test_parity_route_no_match_no_fallback_error() {
        let src = r#"
            fn choose(x: int) -> string {
                route {
                    one: "one" when x == 1,
                    two: "two" when x == 2
                }
            }
            fn main() -> string {
                choose(99)
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    #[test]
    fn test_parity_route_empty_block_error() {
        let src = r#"
            fn empty_route() -> int {
                route {}
            }
            fn main() -> int {
                empty_route()
            }
        "#;
        assert_parity_error(src, "main", &[]);
    }

    // ------------------------------------------------------------------------
    // Non-Pure Construct Rejection Tests (Effects fall back to interpreter)
    // ------------------------------------------------------------------------

    #[test]
    fn test_rejection_llm_call() {
        let src = r#"
            fn ask() -> string {
                llm"""Hello""" with { model: "fake" }
            }
        "#;
        let items = parse_src(src);
        let prog = compile(&items).expect("inline llm call should compile via DelegateExpr");
        assert_eq!(prog.delegated_exprs.len(), 1);
        assert!(prog.functions[0].chunk.code.contains(&Op::DelegateExpr(0)));
    }

    #[test]
    fn test_rejection_tool_call() {
        let src = r#"
            tool search(q: string) -> string {
                server: "kb"
            }
            fn main() -> string {
                search("query")
            }
        "#;
        let items = parse_src(src);
        let prog = compile(&items).expect("inline tool call should compile via DelegateExpr");
        assert_eq!(prog.delegated_exprs.len(), 1);
        assert!(prog.functions[0].chunk.code.contains(&Op::DelegateExpr(0)));
    }

    #[test]
    fn test_rejection_computer_call() {
        let src = r#"
            fn click_it() -> () {
                computer.click(10, 20)
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("computer call must be rejected");
        assert!(
            err.contains("not in bytecode subset: computer call"),
            "Expected 'not in bytecode subset: computer call', got: {err}"
        );
    }

    #[test]
    fn test_rejection_par_map() {
        let src = r#"
            fn main() -> () {
                let res = par map [1, 2, 3] |x| -> x + 1
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("par map must be rejected");
        assert!(
            err.contains("not in bytecode subset: par map"),
            "Expected 'not in bytecode subset: par map', got: {err}"
        );
    }

    #[test]
    fn test_rejection_par_all() {
        let src = r#"
            fn main() -> () {
                par all(1, 2)
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("par all must be rejected");
        assert!(
            err.contains("not in bytecode subset: par all"),
            "Expected 'not in bytecode subset: par all', got: {err}"
        );
    }

    #[test]
    fn test_rejection_par_race() {
        let src = r#"
            fn main() -> () {
                par race [1, 2]
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("par race must be rejected");
        assert!(
            err.contains("not in bytecode subset: par race"),
            "Expected 'not in bytecode subset: par race', got: {err}"
        );
    }

    #[test]
    fn test_rejection_decide() {
        let src = r#"
            fn triage(t: string) -> string {
                let d = decide {
                    dept: "choose team" choose [billing, tech]
                }
                on t
                with { model: "fake" }
                "done"
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("decide must be rejected");
        assert!(
            err.contains("not in bytecode subset: decide call"),
            "Expected 'not in bytecode subset: decide call', got: {err}"
        );
    }

    #[test]
    fn test_rejection_route_with_effectful_arm() {
        let src = r#"
            fn choose(x: int) -> string {
                route {
                    a: llm"""prompt""" with { model: "fake" } when x < 5,
                    b: "large" otherwise
                }
            }
        "#;
        let items = parse_src(src);
        let prog =
            compile(&items).expect("route with effectful arm should compile via DelegateExpr");
        assert_eq!(prog.delegated_exprs.len(), 1);
        assert!(prog.functions[0].chunk.code.contains(&Op::DelegateExpr(0)));
    }

    #[test]
    fn test_rejection_route_with_effectful_cond() {
        let src = r#"
            tool web_search(query: string) -> string {
                server: "kb"
            }
            fn choose(x: int) -> string {
                route {
                    a: "found" when web_search("hello") == "yes",
                    b: "fallback" otherwise
                }
            }
        "#;
        let items = parse_src(src);
        let prog =
            compile(&items).expect("route with effectful cond should compile via DelegateExpr");
        assert_eq!(prog.delegated_exprs.len(), 1);
        assert!(prog.functions[0].chunk.code.contains(&Op::DelegateExpr(0)));
    }

    #[test]
    fn test_rejection_merge() {
        let src = r#"
            fn join_records(a: [int], b: [int]) -> [int] {
                a | merge b
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("merge must be rejected");
        assert!(
            err.contains("not in bytecode subset: merge"),
            "Expected 'not in bytecode subset: merge', got: {err}"
        );
    }

    #[test]
    fn test_rejection_stream_let() {
        let src = r#"
            fn stream_work() -> () {
                stream let s = llm"""talk""" with { model: "fake" }
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("stream let must be rejected");
        assert!(
            err.contains("not in bytecode subset: stream let"),
            "Expected 'not in bytecode subset: stream let', got: {err}"
        );
    }

    #[test]
    fn test_rejection_agent_declaration() {
        let src = r#"
            agent Counter {
                state {
                    count: int = 0
                }
                fn inc() -> int {
                    state.count += 1
                    state.count
                }
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("agent declaration must be rejected");
        assert!(
            err.contains("not in bytecode subset: agent declaration 'Counter'"),
            "Expected 'not in bytecode subset: agent declaration', got: {err}"
        );
    }

    #[test]
    fn test_rejection_declared_effects() {
        let src = r#"
            fn effectful_fn() -> int uses Llm, IO {
                42
            }
        "#;
        let items = parse_src(src);
        let err = compile(&items).expect_err("effectful function must be rejected");
        assert!(
            err.contains("not in bytecode subset: function 'effectful_fn' declares effects"),
            "Expected 'not in bytecode subset: function declares effects', got: {err}"
        );
    }

    // ------------------------------------------------------------------------
    // Mixed-Program Function-Level Effect Delegation Parity
    // ------------------------------------------------------------------------

    static MIXED_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvVarGuard {
        var: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(var: &'static str, val: &str) -> Self {
            let prev = std::env::var(var).ok();
            unsafe {
                std::env::set_var(var, val);
            }
            Self { var, prev }
        }
        #[allow(dead_code)]
        fn remove(var: &'static str) -> Self {
            let prev = std::env::var(var).ok();
            unsafe {
                std::env::remove_var(var);
            }
            Self { var, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(p) => unsafe {
                    std::env::set_var(self.var, p);
                },
                None => unsafe {
                    std::env::remove_var(self.var);
                },
            }
        }
    }

    fn assert_mixed_parity(src: &str, entry: &str, args: &[NudgeVal]) {
        let _lock = MIXED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard_provider = EnvVarGuard::set("NUDGE_PROVIDER", "fake");

        let items = parse_src(src);

        // (a) Interpreter run: NUDGE_VM=0
        let interp_res = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "0");
            run_program(&items, entry, args)
        };

        // (b) VM run: NUDGE_VM set to 1
        let vm_res = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            run_program(&items, entry, args)
        };

        assert_eq!(
            interp_res, vm_res,
            "Mixed parity mismatch between interpreter and VM for source:\n{src}"
        );
    }

    fn assert_mixed_parity_error(src: &str, entry: &str, args: &[NudgeVal]) {
        let _lock = MIXED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard_provider = EnvVarGuard::set("NUDGE_PROVIDER", "fake");

        let items = parse_src(src);

        // (a) Interpreter run: NUDGE_VM=0
        let interp_err = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "0");
            run_program(&items, entry, args).expect_err("interpreter should fail on this program")
        };

        // (b) VM run: NUDGE_VM set to 1
        let vm_err = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            run_program(&items, entry, args).expect_err("VM should fail on this program")
        };

        assert_eq!(
            interp_err, vm_err,
            "Mixed error parity mismatch between interpreter and VM for source:\n{src}"
        );
    }

    #[test]
    fn test_parity_mixed_llm_and_arithmetic() {
        let src = r#"
            fn ask_summary(topic: string) -> string uses LLM {
                llm"""Summarize {topic}""" with { model: "fake:default" }
            }

            fn main() -> int {
                let s = ask_summary("rust")
                len(s) * 3 + 7
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_tool_fn() {
        let src = r#"
            tool web_search(q: string) -> string {
                server: "kb"
            }

            fn search_topic(q: string) -> string uses Tool {
                web_search(q)
            }

            fn main() -> string {
                let res = search_topic("nudge-lang")
                res + " verified"
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_recursion_in_pure_part() {
        let src = r#"
            fn ask_base(msg: string) -> string uses LLM {
                llm"""base: {msg}""" with { model: "fake:default" }
            }

            fn countdown_and_ask(n: int) -> string {
                route {
                    a: ask_base("ready") when n <= 0,
                    b: countdown_and_ask(n - 1) otherwise
                }
            }

            fn main() -> string {
                countdown_and_ask(4)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_pure_program_unchanged() {
        let src = r#"
            fn fib(n: int) -> int {
                route {
                    a: n when n <= 1,
                    b: fib(n - 1) + fib(n - 2) otherwise
                }
            }

            fn main() -> int {
                fib(10)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_kwargs_and_multiple_calls() {
        let src = r#"
            fn ask_step1(input: string, count: int) -> string uses LLM {
                llm"""step1: {input} count: {count}""" with { model: "fake:default" }
            }

            fn ask_step2(input: string) -> string uses LLM {
                llm"""step2: {input}""" with { model: "fake:default" }
            }

            fn format_two(a: string, b: string) -> string {
                a + " => " + b
            }

            fn main() -> string {
                let s1 = ask_step1(count=3, input="start")
                let s2 = ask_step2(s1)
                format_two(s1, s2)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_mixed_entry_with_args() {
        let src = r#"
            fn ask_agent(prompt: string) -> string uses LLM {
                llm"""{prompt}""" with { model: "fake:default" }
            }

            fn orchestrate(topic: string, repeat: int) -> int {
                let ans = ask_agent(topic)
                len(ans) + repeat
            }
        "#;
        assert_mixed_parity(
            src,
            "orchestrate",
            &[NudgeVal::Str("delegation".into()), NudgeVal::Int(100)],
        );
    }

    // ------------------------------------------------------------------------
    // Expression-Level Effect Delegation Parity (Inline llm/tool calls)
    // ------------------------------------------------------------------------

    // (a) pure fn with one inline llm
    #[test]
    fn test_parity_inline_llm_single() {
        let src = r#"
            fn main() -> int {
                let s = llm"""Summarize rust""" with { model: "fake:default" }
                len(s) * 3 + 7
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    // (b) two inline llm calls
    #[test]
    fn test_parity_inline_llm_two_calls() {
        let src = r#"
            fn main() -> int {
                let s1 = llm"""Hello""" with { model: "fake:default" }
                let s2 = llm"""World""" with { model: "fake:default" }
                len(s1) * 10 + len(s2)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    // (c) inline llm mixed with VM arithmetic using a local
    #[test]
    fn test_parity_inline_llm_with_locals_and_arithmetic() {
        let src = r#"
            fn main() -> int {
                let x = 10
                let topic = "rust-lang"
                let s = llm"""Topic: {topic}""" with { model: "fake:default" }
                x * 2 + len(s)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    // (d) inline llm + a delegated effectful named fn
    #[test]
    fn test_parity_inline_llm_and_delegated_fn() {
        let src = r#"
            fn ask_named(topic: string) -> string uses LLM {
                llm"""Topic: {topic}""" with { model: "fake:default" }
            }

            fn main() -> int {
                let s1 = ask_named("rust")
                let s2 = llm"""Followup {s1}""" with { model: "fake:default" }
                len(s1) + len(s2)
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_inline_tool_call() {
        let src = r#"
            tool web_search(q: string) -> string {
                server: "kb"
            }

            fn main() -> string {
                let query = "nudge language"
                let res = web_search(query)
                res + " verified"
            }
        "#;
        assert_mixed_parity(src, "main", &[]);
    }

    #[test]
    fn test_parity_inline_llm_error_unknown_interpolation() {
        let src = r#"
            fn main() -> string {
                llm"""Hello {nonexistent}""" with { model: "fake:default" }
            }
        "#;
        assert_mixed_parity_error(src, "main", &[]);
    }

    #[test]
    fn test_parity_inline_llm_trace_record_and_replay() {
        let _lock = MIXED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard_provider = EnvVarGuard::set("NUDGE_PROVIDER", "fake");

        let src = r#"
            fn main() -> string {
                let topic = "nudge"
                let s = llm"""Info about {topic}""" with { model: "fake:default" }
                s + " verified"
            }
        "#;
        let items = parse_src(src);
        let path_vm =
            std::env::temp_dir().join(format!("nudge-inline-llm-vm-{}.jsonl", std::process::id()));
        let path_interp = std::env::temp_dir().join(format!(
            "nudge-inline-llm-interp-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path_vm);
        let _ = std::fs::remove_file(&path_interp);

        // Record under VM
        let recorded_val_vm = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            let _scope = nudge_runtime::trace::testing::scope(Some(path_vm.clone()), None);
            run_program(&items, "main", &[]).expect("record run under VM")
        };

        let trace_content_vm = std::fs::read_to_string(&path_vm).expect("trace file should exist");
        assert_eq!(
            crate::tracecheck::validate(&trace_content_vm),
            Vec::<String>::new()
        );

        // Replay under VM
        let replayed_val_vm = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path_vm.clone()));
            run_program(&items, "main", &[]).expect("replay run under VM")
        };
        assert_eq!(recorded_val_vm, replayed_val_vm);

        // Record under Interpreter (NUDGE_VM=0)
        let recorded_val_interp = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "0");
            let _scope = nudge_runtime::trace::testing::scope(Some(path_interp.clone()), None);
            run_program(&items, "main", &[]).expect("record run under interpreter")
        };
        assert_eq!(recorded_val_vm, recorded_val_interp);

        let trace_content_interp =
            std::fs::read_to_string(&path_interp).expect("interp trace should exist");
        let _ = std::fs::remove_file(&path_vm);
        let _ = std::fs::remove_file(&path_interp);

        // Byte-identical trace parity check
        assert_eq!(
            trace_content_vm, trace_content_interp,
            "Trace from VM must be byte-identical to trace from interpreter"
        );
    }

    #[test]
    fn test_trace_byte_identical_pure_and_inline_effect() {
        let _lock = MIXED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard_provider = EnvVarGuard::set("NUDGE_PROVIDER", "fake");

        let src = r#"
            fn greeting() -> string {
                llm"""say hi""" with { model: "fake:default" }
            }
            fn main() -> string {
                greeting()
            }
        "#;
        let items = parse_src(src);
        let path_vm = std::env::temp_dir().join(format!(
            "nudge-trace-parity-vm-{}.jsonl",
            std::process::id()
        ));
        let path_interp = std::env::temp_dir().join(format!(
            "nudge-trace-parity-interp-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path_vm);
        let _ = std::fs::remove_file(&path_interp);

        // Record under VM (NUDGE_VM=1)
        let recorded_val_vm = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            let _scope = nudge_runtime::trace::testing::scope(Some(path_vm.clone()), None);
            run_program(&items, "main", &[]).expect("record run under VM")
        };

        let trace_content_vm =
            std::fs::read_to_string(&path_vm).expect("VM trace file should exist");
        assert_eq!(
            crate::tracecheck::validate(&trace_content_vm),
            Vec::<String>::new()
        );

        // Record under Interpreter (NUDGE_VM=0)
        let recorded_val_interp = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "0");
            let _scope = nudge_runtime::trace::testing::scope(Some(path_interp.clone()), None);
            run_program(&items, "main", &[]).expect("record run under interpreter")
        };
        assert_eq!(recorded_val_vm, recorded_val_interp);

        let trace_content_interp =
            std::fs::read_to_string(&path_interp).expect("interp trace should exist");

        // Byte-identical trace parity check
        assert_eq!(
            trace_content_vm, trace_content_interp,
            "Trace from VM must be byte-identical to trace from interpreter"
        );

        // Assert expected fn.return trace records exist in order
        let lines: Vec<&str> = trace_content_vm.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "expected 3 trace records (llm.call, greeting.fn.return, main.fn.return)"
        );
        let r1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let r2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let r3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(r1["kind"], "llm.call");
        assert_eq!(r2["kind"], "fn.return");
        assert_eq!(r2["fn"], "greeting");
        assert_eq!(r2["seq"], 2);
        assert_eq!(r3["kind"], "fn.return");
        assert_eq!(r3["fn"], "main");
        assert_eq!(r3["seq"], 3);

        // Cross-VM replay 1: record under VM, replay under Interpreter
        let replayed_val_interp = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "0");
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path_vm.clone()));
            run_program(&items, "main", &[]).expect("replay under interpreter")
        };
        assert_eq!(replayed_val_interp, recorded_val_vm);

        // Cross-VM replay 2: record under Interpreter, replay under VM
        let replayed_val_vm = {
            let _guard_vm = EnvVarGuard::set("NUDGE_VM", "1");
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path_interp.clone()));
            run_program(&items, "main", &[]).expect("replay under VM")
        };
        assert_eq!(replayed_val_vm, recorded_val_interp);

        let _ = std::fs::remove_file(&path_vm);
        let _ = std::fs::remove_file(&path_interp);
    }
}
