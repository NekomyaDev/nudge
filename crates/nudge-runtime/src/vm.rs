//! Nudge Virtual Machine
//!
//! Stack-based bytecode interpreter with:
//! - Call stack for function calls
//! - Local and global variable storage
//! - Effect checking
//! - Trace emission

use crate::bytecode::{Constant, Instruction, OpCode, Program};
use std::collections::HashMap;

/// Runtime value
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    None,
    #[allow(dead_code)] // function values are not constructible from surface syntax yet
    Function(u32), // Function index
    Native(&'static str), // Native function name
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::List(v) => {
                write!(f, "[")?;
                for (i, item) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(v) => {
                write!(f, "{{")?;
                for (i, (key, val)) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", key, val)?;
                }
                write!(f, "}}")
            }
            Value::None => write!(f, "none"),
            Value::Function(idx) => write!(f, "<function:{}>", idx),
            Value::Native(name) => write!(f, "<native:{}>", name),
        }
    }
}

/// Call frame for function calls
#[derive(Debug)]
struct CallFrame {
    function_index: u32,
    #[allow(dead_code)] // advanced with jump-relative frames in the planned flat loop
    ip: u32, // Instruction pointer
    base_pointer: u32,  // Base pointer in value stack
    locals: Vec<Value>, // Local variables
}

/// Virtual Machine
#[allow(dead_code)] // ip: reserved for the planned flat interpreter loop
pub struct VM {
    program: Program,
    stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    globals: HashMap<String, Value>,
    ip: u32, // Current instruction pointer
    running: bool,
    trace_enabled: bool,
}

impl VM {
    /// Create a new VM with the given program
    pub fn new(program: Program) -> Self {
        let mut vm = VM {
            program,
            stack: Vec::new(),
            call_stack: Vec::new(),
            globals: HashMap::new(),
            ip: 0,
            running: false,
            trace_enabled: false,
        };

        // Register stdlib functions and modules
        let stdlib_functions = crate::stdlib::register_all();
        let mut io_module: HashMap<String, Value> = HashMap::new();
        let mut math_module: HashMap<String, Value> = HashMap::new();
        let mut str_module: HashMap<String, Value> = HashMap::new();

        for (name, func) in stdlib_functions {
            // Register as global (e.g., "io.read")
            vm.globals.insert(name.clone(), func.clone());

            // Also register in module maps (e.g., io["read"])
            if name.starts_with("io.") {
                let short = name.strip_prefix("io.").unwrap().to_string();
                io_module.insert(short, func);
            } else if name.starts_with("math.") {
                let short = name.strip_prefix("math.").unwrap().to_string();
                math_module.insert(short, func);
            } else if name.starts_with("str.") {
                let short = name.strip_prefix("str.").unwrap().to_string();
                str_module.insert(short, func);
            }
        }

        vm.globals.insert("io".to_string(), Value::Map(io_module));
        vm.globals
            .insert("math".to_string(), Value::Map(math_module));
        vm.globals.insert("str".to_string(), Value::Map(str_module));

        // Register program functions as globals
        for (i, func) in vm.program.functions.iter().enumerate() {
            vm.globals
                .entry(func.name.clone())
                .or_insert(Value::Function(i as u32));
        }

        vm
    }

    /// Enable trace emission
    pub fn enable_trace(&mut self) {
        self.trace_enabled = true;
    }

    /// Get current stack values
    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    /// Run the program
    pub fn run(&mut self) -> Result<(), String> {
        if self.program.functions.is_empty() {
            return Err("No functions in program".to_string());
        }

        // Initialize with main function
        let main_index = self.program.entry_point;
        self.call_stack.push(CallFrame {
            function_index: main_index,
            ip: 0,
            base_pointer: 0,
            locals: Vec::new(),
        });

        self.running = true;

        while self.running {
            if self.call_stack.is_empty() {
                break;
            }

            let frame = self.call_stack.last().unwrap();
            let func = &self.program.functions[frame.function_index as usize];

            if frame.ip >= func.instructions.len() as u32 {
                return Err("Instruction pointer out of bounds".to_string());
            }

            let inst = &func.instructions[frame.ip as usize].clone();
            let line = inst.line;

            // Execute instruction
            self.execute_instruction(inst, line)?;
        }

        Ok(())
    }

    /// Execute a single instruction
    fn execute_instruction(&mut self, inst: &Instruction, _line: u32) -> Result<(), String> {
        match &inst.op {
            OpCode::Push => {
                let idx = inst.arg.ok_or("Push requires argument")?;
                let constant = self.program.constants[idx as usize].clone();
                let value = match constant {
                    Constant::Int(v) => Value::Int(v),
                    Constant::Float(v) => Value::Float(v),
                    Constant::String(v) => Value::String(v),
                    Constant::Bool(v) => Value::Bool(v),
                    Constant::None => Value::None,
                };
                self.stack.push(value);
                self.advance_ip();
            }

            OpCode::Pop => {
                self.pop()?;
                self.advance_ip();
            }

            OpCode::Dup => {
                let value = self.peek()?.clone();
                self.stack.push(value);
                self.advance_ip();
            }

            OpCode::Swap => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.stack.push(a);
                self.stack.push(b);
                self.advance_ip();
            }

            OpCode::Add => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                    (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                    (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                    (Value::String(a), Value::String(b)) => Value::String(format!("{}{}", a, b)),
                    _ => return Err(format!("Cannot add {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Sub => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
                    (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 - b),
                    (Value::Float(a), Value::Int(b)) => Value::Float(a - *b as f64),
                    _ => return Err(format!("Cannot subtract {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Mul => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
                    (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 * b),
                    (Value::Float(a), Value::Int(b)) => Value::Float(a * *b as f64),
                    _ => return Err(format!("Cannot multiply {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(a), Value::Int(b)) => {
                        if *b == 0 {
                            return Err("Division by zero".to_string());
                        }
                        match a.checked_div(*b) {
                            Some(v) => Value::Int(v),
                            None => return Err("Integer overflow in division".to_string()),
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if *b == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(a / b)
                    }
                    (Value::Int(a), Value::Float(b)) => {
                        if *b == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(*a as f64 / b)
                    }
                    (Value::Float(a), Value::Int(b)) => {
                        if *b == 0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(a / *b as f64)
                    }
                    _ => return Err(format!("Cannot divide {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Mod => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Int(a), Value::Int(b)) => {
                        if *b == 0 {
                            return Err("Division by zero".to_string());
                        }
                        match a.checked_rem(*b) {
                            Some(v) => Value::Int(v),
                            None => return Err("Integer overflow in modulo".to_string()),
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if *b == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(a % b)
                    }
                    (Value::Int(a), Value::Float(b)) => {
                        if *b == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(*a as f64 % b)
                    }
                    (Value::Float(a), Value::Int(b)) => {
                        if *b == 0 {
                            return Err("Division by zero".to_string());
                        }
                        Value::Float(a % *b as f64)
                    }
                    _ => return Err(format!("Cannot modulo {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Neg => {
                let a = self.pop()?;
                let result = match a {
                    Value::Int(v) => Value::Int(v.checked_neg().unwrap_or(v)),
                    Value::Float(v) => Value::Float(-v),
                    _ => return Err(format!("Cannot negate {:?}", a)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Eq => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(self.values_equal(&a, &b));
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Ne => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(!self.values_equal(&a, &b));
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Lt => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(self.compare_values(&a, &b)? < 0);
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Le => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(self.compare_values(&a, &b)? <= 0);
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Gt => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(self.compare_values(&a, &b)? > 0);
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Ge => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = Value::Bool(self.compare_values(&a, &b)? >= 0);
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::And => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Bool(a), Value::Bool(b)) => Value::Bool(*a && *b),
                    _ => return Err(format!("Cannot AND {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Or => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Bool(a), Value::Bool(b)) => Value::Bool(*a || *b),
                    _ => return Err(format!("Cannot OR {:?} and {:?}", a, b)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Not => {
                let a = self.pop()?;
                let result = match a {
                    Value::Bool(v) => Value::Bool(!v),
                    _ => return Err(format!("Cannot NOT {:?}", a)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Load => {
                let idx = inst.arg.ok_or("Load requires argument")?;
                let frame = self.call_stack.last().ok_or("No call frame")?;
                let value = frame
                    .locals
                    .get(idx as usize)
                    .ok_or(format!("Local variable {} not found", idx))?
                    .clone();
                self.stack.push(value);
                self.advance_ip();
            }

            OpCode::Store => {
                let idx = inst.arg.ok_or("Store requires argument")?;
                let value = self.pop()?;
                let frame = self.call_stack.last_mut().ok_or("No call frame")?;
                while frame.locals.len() <= idx as usize {
                    frame.locals.push(Value::None);
                }
                frame.locals[idx as usize] = value;
                self.advance_ip();
            }

            OpCode::LoadGlobal => {
                let idx = inst.arg.ok_or("LoadGlobal requires argument")?;
                let name = match &self.program.constants[idx as usize] {
                    Constant::String(s) => s.clone(),
                    _ => return Err("Global name must be a string".to_string()),
                };
                let value = self
                    .globals
                    .get(&name)
                    .ok_or(format!("Global variable '{}' not found", name))?
                    .clone();
                self.stack.push(value);
                self.advance_ip();
            }

            OpCode::StoreGlobal => {
                let idx = inst.arg.ok_or("StoreGlobal requires argument")?;
                let name = match &self.program.constants[idx as usize] {
                    Constant::String(s) => s.clone(),
                    _ => return Err("Global name must be a string".to_string()),
                };
                let value = self.pop()?;
                self.globals.insert(name, value);
                self.advance_ip();
            }

            OpCode::Jump => {
                let offset = inst.arg.ok_or("Jump requires argument")?;
                let frame = self.call_stack.last_mut().ok_or("No call frame")?;
                frame.ip = offset;
            }

            OpCode::JumpIf => {
                let offset = inst.arg.ok_or("JumpIf requires argument")?;
                let condition = self.pop()?;
                match condition {
                    Value::Bool(true) => {
                        let frame = self.call_stack.last_mut().ok_or("No call frame")?;
                        frame.ip = offset;
                    }
                    Value::Bool(false) => {
                        self.advance_ip();
                    }
                    _ => return Err("JumpIf requires boolean".to_string()),
                }
            }

            OpCode::JumpIfNot => {
                let offset = inst.arg.ok_or("JumpIfNot requires argument")?;
                let condition = self.pop()?;
                match condition {
                    Value::Bool(false) => {
                        let frame = self.call_stack.last_mut().ok_or("No call frame")?;
                        frame.ip = offset;
                    }
                    Value::Bool(true) => {
                        self.advance_ip();
                    }
                    _ => return Err("JumpIfNot requires boolean".to_string()),
                }
            }

            OpCode::Call => {
                let arg_count = inst.arg.ok_or("Call requires argument")?;
                if self.stack.len() < arg_count as usize + 1 {
                    return Err(format!(
                        "Call: stack underflow (stack={}, args={})",
                        self.stack.len(),
                        arg_count
                    ));
                }
                let func_idx = self.stack.len() - arg_count as usize - 1;
                let func_value = self.stack[func_idx].clone();

                // Resolve function index
                let resolved_idx: Option<u32> = match &func_value {
                    Value::Function(idx) => Some(*idx),
                    Value::Int(idx) => Some(*idx as u32),
                    _ => None,
                };

                if let Some(idx) = resolved_idx {
                    let func = &self.program.functions[idx as usize];
                    let arity = func.arity;

                    if arg_count != arity {
                        return Err(format!("Expected {} arguments, got {}", arity, arg_count));
                    }

                    // Pop arguments
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    // Pop function value
                    self.pop()?;

                    // Create new call frame
                    let base_pointer = self.stack.len() as u32;
                    self.call_stack.push(CallFrame {
                        function_index: idx,
                        ip: 0,
                        base_pointer,
                        locals: args,
                    });
                } else if let Value::Native(name) = &func_value {
                    // Pop arguments
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    // Pop function value
                    self.pop()?;

                    // Execute native function
                    let name = name.to_string();
                    let result = crate::stdlib::execute(&name, args)?;
                    self.stack.push(result);
                    self.advance_ip();
                } else {
                    return Err("Cannot call non-function".to_string());
                }
            }

            OpCode::Return => {
                let return_value = if !self.stack.is_empty() {
                    Some(self.pop()?)
                } else {
                    None
                };

                let frame = self.call_stack.pop().ok_or("No call frame")?;

                // Restore stack
                self.stack.truncate(frame.base_pointer as usize);

                // Push return value
                if let Some(value) = return_value {
                    self.stack.push(value);
                }

                // Advance IP in caller
                if !self.call_stack.is_empty() {
                    self.advance_ip();
                }
            }

            OpCode::Print => {
                let count = inst.arg.unwrap_or(1) as usize;
                if count == 0 {
                    println!();
                } else {
                    let mut vals = Vec::with_capacity(count);
                    for _ in 0..count {
                        vals.push(self.pop()?);
                    }
                    vals.reverse();
                    let s = vals.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" ");
                    println!("{s}");
                }
                self.advance_ip();
            }

            OpCode::ReadLine => {
                let prompt = self.pop()?;
                match prompt {
                    Value::String(s) => {
                        print!("{}", s);
                        use std::io::{self, Write};
                        let _ = io::stdout().flush();

                        let mut input = String::new();
                        io::stdin()
                            .read_line(&mut input)
                            .map_err(|e| format!("ReadLine error: {e}"))?;
                        let input = input.trim().to_string();

                        self.stack.push(Value::String(input));
                        self.advance_ip();
                    }
                    _ => return Err("ReadLine requires string prompt".to_string()),
                }
            }

            OpCode::NewList => {
                self.stack.push(Value::List(Vec::new()));
                self.advance_ip();
            }

            OpCode::ListPush => {
                let value = self.pop()?;
                let list = self.pop()?;
                match list {
                    Value::List(mut v) => {
                        v.push(value);
                        self.stack.push(Value::List(v));
                        self.advance_ip();
                    }
                    _ => return Err("ListPush requires list".to_string()),
                }
            }

            OpCode::NewMap => {
                self.stack.push(Value::Map(HashMap::new()));
                self.advance_ip();
            }

            OpCode::MapInsert => {
                let value = self.pop()?;
                let key = self.pop()?;
                let map = self.pop()?;
                match (map, key) {
                    (Value::Map(mut m), Value::String(k)) => {
                        m.insert(k, value);
                        self.stack.push(Value::Map(m));
                        self.advance_ip();
                    }
                    _ => return Err("MapInsert requires map and string key".to_string()),
                }
            }

            OpCode::Index => {
                let index = self.pop()?;
                let collection = self.pop()?;
                let result = match (&collection, &index) {
                    (Value::List(list), Value::Int(i)) => {
                        if *i < 0 || *i >= list.len() as i64 {
                            return Err(format!(
                                "Index {} out of bounds for list of length {}",
                                i,
                                list.len()
                            ));
                        }
                        list[*i as usize].clone()
                    }
                    (Value::Map(map), Value::String(key)) => {
                        map.get(key).cloned().unwrap_or(Value::None)
                    }
                    (Value::String(s), Value::Int(i)) => {
                        let chars: Vec<char> = s.chars().collect();
                        if *i < 0 || *i >= chars.len() as i64 {
                            return Err(format!(
                                "Index {} out of bounds for string of length {}",
                                i,
                                chars.len()
                            ));
                        }
                        Value::String(chars[*i as usize].to_string())
                    }
                    _ => return Err(format!("Cannot index {:?} with {:?}", collection, index)),
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Field => {
                let field_idx = inst.arg.ok_or("Field requires argument")?;
                let field_name = match &self.program.constants[field_idx as usize] {
                    Constant::String(s) => s.clone(),
                    _ => return Err("Field name must be a string".to_string()),
                };
                let object = self.pop()?;
                let result = match &object {
                    Value::Map(map) => map.get(&field_name).cloned().unwrap_or(Value::None),
                    Value::String(s) => {
                        let full_name = format!("{}.{}", s, field_name);
                        if self.globals.contains_key(&full_name) {
                            self.globals[&full_name].clone()
                        } else {
                            Value::None
                        }
                    }
                    _ => {
                        return Err(format!(
                            "Cannot access field '{}' on {:?}",
                            field_name, object
                        ))
                    }
                };
                self.stack.push(result);
                self.advance_ip();
            }

            OpCode::Halt => {
                self.running = false;
            }

            _ => {
                return Err(format!("Unimplemented opcode: {:?}", inst.op));
            }
        }

        Ok(())
    }

    /// Advance instruction pointer
    fn advance_ip(&mut self) {
        if let Some(frame) = self.call_stack.last_mut() {
            frame.ip += 1;
        }
    }

    /// Pop value from stack
    fn pop(&mut self) -> Result<Value, String> {
        self.stack.pop().ok_or("Stack underflow".to_string())
    }

    /// Peek at top of stack
    fn peek(&self) -> Result<&Value, String> {
        self.stack.last().ok_or("Stack underflow".to_string())
    }

    /// Check if two values are equal
    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::None, Value::None) => true,
            _ => false,
        }
    }

    /// Compare two values (-1, 0, 1)
    fn compare_values(&self, a: &Value, b: &Value) -> Result<i32, String> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(if a < b {
                -1
            } else if a > b {
                1
            } else {
                0
            }),
            (Value::Float(a), Value::Float(b)) => Ok(if a < b {
                -1
            } else if a > b {
                1
            } else {
                0
            }),
            (Value::Int(a), Value::Float(b)) => {
                let fa = *a as f64;
                Ok(if fa < *b { -1 } else if fa > *b { 1 } else { 0 })
            }
            (Value::Float(a), Value::Int(b)) => {
                let fb = *b as f64;
                Ok(if *a < fb { -1 } else if *a > fb { 1 } else { 0 })
            }
            (Value::String(a), Value::String(b)) => Ok(a.cmp(b) as i32),
            _ => Err(format!("Cannot compare {:?} and {:?}", a, b)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{Constant, Function, Instruction, OpCode, Program};

    fn make_program(instructions: Vec<Instruction>, constants: Vec<Constant>) -> Program {
        Program {
            version: 1,
            constants,
            functions: vec![Function {
                name: "main".to_string(),
                arity: 0,
                locals: 0,
                instructions,
                effects: vec![],
            }],
            entry_point: 0,
        }
    }

    #[test]
    fn test_simple_print() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                },
                Instruction {
                    op: OpCode::Print,
                    arg: None,
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::String("Hello, World!".to_string())],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
    }

    #[test]
    fn test_print_multi_args() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                },
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                },
                Instruction {
                    op: OpCode::Print,
                    arg: Some(2),
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::String("hello".to_string()), Constant::Int(42)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn test_print_zero_args() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Print,
                    arg: Some(0),
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn test_arithmetic() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // 10
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 20
                Instruction {
                    op: OpCode::Add,
                    arg: None,
                    line: 1,
                }, // 30
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::Int(10), Constant::Int(20)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Int(30));
    }

    #[test]
    fn test_comparison() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // 5
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 10
                Instruction {
                    op: OpCode::Lt,
                    arg: None,
                    line: 1,
                }, // 5 < 10 = true
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::Int(5), Constant::Int(10)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::Bool(true));
    }

    #[test]
    fn test_mixed_comparison() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // 5 (Int)
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 5.0 (Float)
                Instruction {
                    op: OpCode::Eq,
                    arg: None,
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::Int(5), Constant::Float(5.0)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::Bool(true));
    }

    #[test]
    fn test_mixed_div() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // 10 (Int)
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 4.0 (Float)
                Instruction {
                    op: OpCode::Div,
                    arg: None,
                    line: 1,
                }, // 10 / 4.0 = 2.5
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::Int(10), Constant::Float(4.0)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::Float(2.5));
    }

    #[test]
    fn test_mixed_mod() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // 10 (Int)
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 3.0 (Float)
                Instruction {
                    op: OpCode::Mod,
                    arg: None,
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::Int(10), Constant::Float(3.0)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::Float(1.0));
    }

    #[test]
    fn test_string_indexing() {
        let program = make_program(
            vec![
                Instruction {
                    op: OpCode::Push,
                    arg: Some(0),
                    line: 1,
                }, // "hello"
                Instruction {
                    op: OpCode::Push,
                    arg: Some(1),
                    line: 1,
                }, // 1
                Instruction {
                    op: OpCode::Index,
                    arg: None,
                    line: 1,
                },
                Instruction {
                    op: OpCode::Halt,
                    arg: None,
                    line: 1,
                },
            ],
            vec![Constant::String("hello".to_string()), Constant::Int(1)],
        );

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::String("e".to_string()));
    }

    #[test]
    fn test_user_function_call() {
        let program = Program {
            version: 1,
            constants: vec![Constant::String("double".to_string()), Constant::Int(21)],
            functions: vec![
                Function {
                    name: "double".to_string(),
                    arity: 1,
                    locals: 1,
                    instructions: vec![
                        Instruction {
                            op: OpCode::Load,
                            arg: Some(0),
                            line: 1,
                        },
                        Instruction {
                            op: OpCode::Dup,
                            arg: None,
                            line: 1,
                        },
                        Instruction {
                            op: OpCode::Add,
                            arg: None,
                            line: 1,
                        },
                        Instruction {
                            op: OpCode::Return,
                            arg: None,
                            line: 1,
                        },
                    ],
                    effects: vec![],
                },
                Function {
                    name: "main".to_string(),
                    arity: 0,
                    locals: 0,
                    instructions: vec![
                        Instruction {
                            op: OpCode::LoadGlobal,
                            arg: Some(0), // "double"
                            line: 2,
                        },
                        Instruction {
                            op: OpCode::Push,
                            arg: Some(1), // 21
                            line: 2,
                        },
                        Instruction {
                            op: OpCode::Call,
                            arg: Some(1),
                            line: 2,
                        },
                        Instruction {
                            op: OpCode::Halt,
                            arg: None,
                            line: 2,
                        },
                    ],
                    effects: vec![],
                },
            ],
            entry_point: 1,
        };

        let mut vm = VM::new(program);
        assert!(vm.run().is_ok());
        assert_eq!(vm.stack[0], Value::Int(42));
    }
}
