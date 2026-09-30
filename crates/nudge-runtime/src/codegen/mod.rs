//! Bytecode Compiler
//!
//! Compiles Nudge AST to bytecode instructions.
//! Uses the existing lexer and parser from nudgec.

use crate::bytecode::{Constant, Function, Instruction, OpCode, Program};
use std::collections::HashMap;

/// Compilation context
pub struct Compiler {
    constants: Vec<Constant>,
    constant_map: HashMap<String, u32>,
    functions: Vec<Function>,
    current_function: Option<usize>,
    locals: HashMap<String, u32>,
    local_count: u32,
    label_counter: u32,
    labels: HashMap<String, u32>,
    jumps: Vec<(u32, String)>, // (instruction_index, label_name)
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            constants: Vec::new(),
            constant_map: HashMap::new(),
            functions: Vec::new(),
            current_function: None,
            locals: HashMap::new(),
            local_count: 0,
            label_counter: 0,
            labels: HashMap::new(),
            jumps: Vec::new(),
        }
    }

    /// Add a constant to the pool and return its index
    pub fn add_constant(&mut self, constant: Constant) -> u32 {
        let key = match &constant {
            Constant::Int(v) => format!("int:{}", v),
            Constant::Float(v) => format!("float:{}", v),
            Constant::String(v) => format!("string:{}", v),
            Constant::Bool(v) => format!("bool:{}", v),
            Constant::None => "none".to_string(),
        };

        if let Some(&idx) = self.constant_map.get(&key) {
            return idx;
        }

        let idx = self.constants.len() as u32;
        self.constants.push(constant);
        self.constant_map.insert(key, idx);
        idx
    }

    /// Start a new function
    pub fn begin_function(&mut self, name: String, arity: u32) {
        let idx = self.functions.len();
        self.functions.push(Function {
            name,
            arity,
            locals: 0,
            instructions: Vec::new(),
            effects: Vec::new(),
        });
        self.current_function = Some(idx);
        self.locals.clear();
        self.local_count = 0;
        self.labels.clear();
        self.jumps.clear();
    }

    /// End current function
    pub fn end_function(&mut self) {
        if let Some(idx) = self.current_function {
            self.functions[idx].locals = self.local_count;
        }
        self.current_function = None;
    }

    /// Emit an instruction
    pub fn emit(&mut self, op: OpCode, arg: Option<u32>, line: u32) {
        if let Some(idx) = self.current_function {
            self.functions[idx]
                .instructions
                .push(Instruction { op, arg, line });
        }
    }

    /// Get or create a local variable
    pub fn get_or_create_local(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.locals.get(name) {
            return idx;
        }
        let idx = self.local_count;
        self.locals.insert(name.to_string(), idx);
        self.local_count += 1;
        idx
    }

    /// Create a new label
    pub fn create_label(&mut self) -> String {
        let label = format!("L{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    /// Mark current position with a label
    pub fn mark_label(&mut self, label: String) {
        if let Some(idx) = self.current_function {
            let pos = self.functions[idx].instructions.len() as u32;
            self.labels.insert(label, pos);
        }
    }

    /// Add a jump to a label (call AFTER emitting the jump instruction)
    pub fn add_jump(&mut self, label: String) {
        if let Some(idx) = self.current_function {
            let pos = self.functions[idx].instructions.len() as u32 - 1;
            self.jumps.push((pos, label));
        }
    }

    /// Resolve all jumps
    pub fn resolve_jumps(&mut self) {
        let jumps = std::mem::take(&mut self.jumps);
        for (pos, label) in jumps {
            if let Some(&target) = self.labels.get(&label) {
                if let Some(idx) = self.current_function {
                    self.functions[idx].instructions[pos as usize].arg = Some(target);
                }
            }
        }
    }

    /// Compile a simple expression
    pub fn compile_expr(&mut self, expr: &Expr, line: u32) {
        match expr {
            Expr::IntLiteral(v) => {
                let idx = self.add_constant(Constant::Int(*v));
                self.emit(OpCode::Push, Some(idx), line);
            }
            Expr::FloatLiteral(v) => {
                let idx = self.add_constant(Constant::Float(*v));
                self.emit(OpCode::Push, Some(idx), line);
            }
            Expr::StringLiteral(v) => {
                let idx = self.add_constant(Constant::String(v.clone()));
                self.emit(OpCode::Push, Some(idx), line);
            }
            Expr::BoolLiteral(v) => {
                let idx = self.add_constant(Constant::Bool(*v));
                self.emit(OpCode::Push, Some(idx), line);
            }
            Expr::NoneLiteral => {
                let idx = self.add_constant(Constant::None);
                self.emit(OpCode::Push, Some(idx), line);
            }
            Expr::Identifier(name) => {
                if let Some(&idx) = self.locals.get(name) {
                    self.emit(OpCode::Load, Some(idx), line);
                } else {
                    let idx = self.add_constant(Constant::String(name.clone()));
                    self.emit(OpCode::LoadGlobal, Some(idx), line);
                }
            }
            Expr::BinaryOp { left, op, right } => {
                self.compile_expr(left, line);
                self.compile_expr(right, line);
                match op {
                    BinaryOp::Add => self.emit(OpCode::Add, None, line),
                    BinaryOp::Sub => self.emit(OpCode::Sub, None, line),
                    BinaryOp::Mul => self.emit(OpCode::Mul, None, line),
                    BinaryOp::Div => self.emit(OpCode::Div, None, line),
                    BinaryOp::Mod => self.emit(OpCode::Mod, None, line),
                    BinaryOp::Eq => self.emit(OpCode::Eq, None, line),
                    BinaryOp::NotEq => self.emit(OpCode::Ne, None, line),
                    BinaryOp::Lt => self.emit(OpCode::Lt, None, line),
                    BinaryOp::LtEq => self.emit(OpCode::Le, None, line),
                    BinaryOp::Gt => self.emit(OpCode::Gt, None, line),
                    BinaryOp::GtEq => self.emit(OpCode::Ge, None, line),
                    BinaryOp::And => self.emit(OpCode::And, None, line),
                    BinaryOp::Or => self.emit(OpCode::Or, None, line),
                }
            }
            Expr::UnaryOp { op, expr } => {
                self.compile_expr(expr, line);
                match op {
                    UnaryOp::Neg => self.emit(OpCode::Neg, None, line),
                    UnaryOp::Not => self.emit(OpCode::Not, None, line),
                }
            }
            Expr::Call { func, args } => {
                // Push function
                self.compile_expr(func, line);
                // Push arguments
                for arg in args {
                    self.compile_expr(arg, line);
                }
                self.emit(OpCode::Call, Some(args.len() as u32), line);
            }
            Expr::ListLiteral(items) => {
                self.emit(OpCode::NewList, None, line);
                for item in items {
                    self.compile_expr(item, line);
                    self.emit(OpCode::ListPush, None, line);
                }
            }
            Expr::Index { collection, index } => {
                self.compile_expr(collection, line);
                self.compile_expr(index, line);
                self.emit(OpCode::Index, None, line);
            }
            Expr::Field { object, field } => {
                self.compile_expr(object, line);
                let idx = self.add_constant(Constant::String(field.clone()));
                self.emit(OpCode::Field, Some(idx), line);
            }
        }
    }

    /// Compile a statement
    pub fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, line } => {
                self.compile_expr(value, *line);
                let idx = self.get_or_create_local(name);
                self.emit(OpCode::Store, Some(idx), *line);
            }
            Stmt::ExprStmt { expr, line } => {
                self.compile_expr(expr, *line);
                self.emit(OpCode::Pop, None, *line);
            }
            Stmt::Return { value, line } => {
                if let Some(expr) = value {
                    self.compile_expr(expr, *line);
                }
                self.emit(OpCode::Return, None, *line);
            }
            Stmt::If {
                condition,
                body,
                else_body,
                line,
            } => {
                self.compile_expr(condition, *line);
                let else_label = self.create_label();
                let end_label = self.create_label();

                self.emit(OpCode::JumpIfNot, None, *line);
                self.add_jump(else_label.clone());

                for stmt in body {
                    self.compile_stmt(stmt);
                }

                // Always jump to end after if body (skip else)
                self.emit(OpCode::Jump, None, *line);
                self.add_jump(end_label.clone());

                // Mark else label
                self.mark_label(else_label);

                if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        self.compile_stmt(stmt);
                    }
                }

                // Always mark end label
                self.mark_label(end_label);
            }
            Stmt::While {
                condition,
                body,
                line,
            } => {
                let loop_label = self.create_label();
                let end_label = self.create_label();

                self.mark_label(loop_label.clone());
                self.compile_expr(condition, *line);
                self.emit(OpCode::JumpIfNot, None, *line);
                self.add_jump(end_label.clone());

                for stmt in body {
                    self.compile_stmt(stmt);
                }

                self.emit(OpCode::Jump, None, *line);
                self.add_jump(loop_label);
                self.mark_label(end_label);
            }
            Stmt::Print { args, line } => {
                for arg in args {
                    self.compile_expr(arg, *line);
                }
                self.emit(OpCode::Print, Some(args.len() as u32), *line);
            }
            Stmt::Assign { name, value, line } => {
                self.compile_expr(value, *line);
                if let Some(&idx) = self.locals.get(name) {
                    self.emit(OpCode::Store, Some(idx), *line);
                } else {
                    let idx = self.add_constant(Constant::String(name.clone()));
                    self.emit(OpCode::StoreGlobal, Some(idx), *line);
                }
            }
        }
    }

    /// Compile a function definition
    pub fn compile_function(&mut self, func: &FunctionDef) {
        self.begin_function(func.name.clone(), func.params.len() as u32);

        // Store parameters as locals
        for param in &func.params {
            self.get_or_create_local(param);
        }

        // Compile body
        for stmt in &func.body {
            self.compile_stmt(stmt);
        }

        // Add implicit return with None value if function doesn't end with return
        let none_idx = self.add_constant(Constant::None);
        self.emit(OpCode::Push, Some(none_idx), 0);
        self.emit(OpCode::Return, None, 0);

        self.resolve_jumps();
        self.end_function();
    }

    /// Compile a complete program
    pub fn compile_program(&mut self, program: &ProgramDef) -> Program {
        // Compile all functions
        for func in &program.functions {
            self.compile_function(func);
        }

        // Create main function if not exists
        let has_main = self.functions.iter().any(|f| f.name == "main");
        if !has_main {
            self.begin_function("main".to_string(), 0);

            // Register all user-defined functions as globals
            for (i, func) in program.functions.iter().enumerate() {
                let name_idx = self.add_constant(Constant::String(func.name.clone()));
                let func_idx = self.add_constant(Constant::Int(i as i64));
                self.emit(OpCode::Push, Some(func_idx), 0);
                self.emit(OpCode::StoreGlobal, Some(name_idx), 0);
            }

            // stdlib modules (io, math, str) are registered by VM::new()

            for stmt in &program.body {
                self.compile_stmt(stmt);
            }
            self.resolve_jumps();
            self.emit(OpCode::Halt, None, 0);
            self.end_function();
        }

        // Find main function index
        let entry_point = self
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap_or(0) as u32;

        Program {
            version: 1,
            constants: self.constants.clone(),
            functions: self.functions.clone(),
            entry_point,
        }
    }
}

/// AST Expressions
#[derive(Debug, Clone)]
#[allow(dead_code)] // grammar-complete AST: variants not yet lowered
pub enum Expr {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    NoneLiteral,
    Identifier(String),
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    ListLiteral(Vec<Expr>),
    Index {
        collection: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        object: Box<Expr>,
        field: String,
    },
}

/// Binary operators
#[derive(Debug, Clone)]
pub enum BinaryOp {
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
}

/// Unary operators
#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// AST Statements
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)] // *Stmt variants mirror the grammar
pub enum Stmt {
    Let {
        name: String,
        value: Expr,
        line: u32,
    },
    ExprStmt {
        expr: Expr,
        line: u32,
    },
    Return {
        value: Option<Expr>,
        line: u32,
    },
    If {
        condition: Expr,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        line: u32,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        line: u32,
    },
    Print {
        args: Vec<Expr>,
        line: u32,
    },
    Assign {
        name: String,
        value: Expr,
        line: u32,
    },
}

/// Function definition
#[derive(Debug, Clone)]
#[allow(dead_code)] // effects reserved (design §3); not yet read by the VM
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub effects: Vec<String>,
}

/// Program definition
#[derive(Debug, Clone)]
pub struct ProgramDef {
    pub functions: Vec<FunctionDef>,
    pub body: Vec<Stmt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple_expr() {
        let mut compiler = Compiler::new();
        compiler.begin_function("main".to_string(), 0);

        // let x = 42
        let stmt = Stmt::Let {
            name: "x".to_string(),
            value: Expr::IntLiteral(42),
            line: 1,
        };
        compiler.compile_stmt(&stmt);

        // print(x)
        let stmt = Stmt::Print {
            args: vec![Expr::Identifier("x".to_string())],
            line: 2,
        };
        compiler.compile_stmt(&stmt);

        compiler.emit(OpCode::Halt, None, 3);
        compiler.end_function();

        let program = Program {
            version: 1,
            constants: compiler.constants,
            functions: compiler.functions,
            entry_point: 0,
        };

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
        assert!(!program.functions[0].instructions.is_empty());
    }

    #[test]
    fn test_compile_binary_op() {
        let mut compiler = Compiler::new();
        compiler.begin_function("main".to_string(), 0);

        // let x = 10 + 20
        let stmt = Stmt::Let {
            name: "x".to_string(),
            value: Expr::BinaryOp {
                left: Box::new(Expr::IntLiteral(10)),
                op: BinaryOp::Add,
                right: Box::new(Expr::IntLiteral(20)),
            },
            line: 1,
        };
        compiler.compile_stmt(&stmt);

        compiler.emit(OpCode::Halt, None, 2);
        compiler.end_function();

        let program = Program {
            version: 1,
            constants: compiler.constants,
            functions: compiler.functions,
            entry_point: 0,
        };

        // Should have: Push(10), Push(20), Add, Store(x), Halt
        assert_eq!(program.functions[0].instructions.len(), 5);
    }

    #[test]
    fn test_compile_if() {
        let mut compiler = Compiler::new();
        compiler.begin_function("main".to_string(), 0);

        // if true { print("yes") }
        let stmt = Stmt::If {
            condition: Expr::BoolLiteral(true),
            body: vec![Stmt::Print {
                args: vec![Expr::StringLiteral("yes".to_string())],
                line: 2,
            }],
            else_body: None,
            line: 1,
        };
        compiler.compile_stmt(&stmt);

        compiler.emit(OpCode::Halt, None, 3);
        compiler.end_function();

        let program = Program {
            version: 1,
            constants: compiler.constants,
            functions: compiler.functions,
            entry_point: 0,
        };

        assert!(!program.functions[0].instructions.is_empty());
    }

    #[test]
    fn test_compile_list() {
        let mut compiler = Compiler::new();
        compiler.begin_function("main".to_string(), 0);

        // let list = [1, 2, 3]
        let stmt = Stmt::Let {
            name: "list".to_string(),
            value: Expr::ListLiteral(vec![
                Expr::IntLiteral(1),
                Expr::IntLiteral(2),
                Expr::IntLiteral(3),
            ]),
            line: 1,
        };
        compiler.compile_stmt(&stmt);

        compiler.emit(OpCode::Halt, None, 2);
        compiler.end_function();

        let program = Program {
            version: 1,
            constants: compiler.constants,
            functions: compiler.functions,
            entry_point: 0,
        };

        // Should have: NewList, Push(1), ListPush, Push(2), ListPush, Push(3), ListPush, Store(list), Halt
        assert_eq!(program.functions[0].instructions.len(), 9);
    }
}
