//! Nudge Bytecode Format
//!
//! Binary format for efficient execution:
//! - Header: magic bytes + version
//! - Constants pool: strings, numbers, types
//! - Code section: bytecode instructions
//! - Debug info: source maps (optional)

/// Magic bytes: "NDGE"
pub const MAGIC: [u8; 4] = *b"NDGE";

/// Current bytecode version
#[allow(dead_code)] // reserved for future bytecode versioning
pub const VERSION: u32 = 1;

/// Bytecode instruction set
#[derive(Debug, Clone, PartialEq)]
pub enum OpCode {
    // Stack operations
    Push, // Push constant onto stack
    Pop,  // Pop top of stack
    Dup,  // Duplicate top of stack
    Swap, // Swap top two stack values

    // Arithmetic
    Add, // a + b
    Sub, // a - b
    Mul, // a * b
    Div, // a / b
    Mod, // a % b
    Neg, // -a

    // Comparison
    Eq, // a == b
    Ne, // a != b
    Lt, // a < b
    Le, // a <= b
    Gt, // a > b
    Ge, // a >= b

    // Logical
    And, // a && b
    Or,  // a || b
    Not, // !a

    // Variables
    Load,        // Load local variable
    Store,       // Store local variable
    LoadGlobal,  // Load global variable
    StoreGlobal, // Store global variable

    // Control flow
    Jump,      // Unconditional jump
    JumpIf,    // Jump if top of stack is true
    JumpIfNot, // Jump if top of stack is false
    Call,      // Call function
    Return,    // Return from function

    // Data structures
    NewList,   // Create new list
    ListPush,  // Push to list
    NewMap,    // Create new map
    MapInsert, // Insert into map
    Index,     // Index access (list[i] or map[key])
    Field,     // Field access (obj.field)

    // LLM operations
    LlmCall,   // Call LLM with schema
    LlmStream, // Stream LLM response

    // I/O operations
    Print,     // Print to stdout
    ReadLine,  // Read line from stdin
    ReadFile,  // Read file
    WriteFile, // Write file

    // Type operations
    TypeCheck, // Runtime type check
    Cast,      // Type cast

    // Effects
    CheckEffect, // Check if effect is allowed

    // Debug
    Trace,      // Emit trace record
    Breakpoint, // Debugger breakpoint

    // Halt
    Halt, // Stop execution
}

/// A single bytecode instruction
#[derive(Debug, Clone)]
pub struct Instruction {
    pub op: OpCode,
    pub arg: Option<u32>, // Optional argument (constant index, jump offset, etc.)
    pub line: u32,        // Source line number (for debugging)
}

/// Constant pool entry
#[derive(Debug, Clone)]
pub enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    None,
}

/// Bytecode function
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub arity: u32,  // Number of parameters
    pub locals: u32, // Number of local variables
    pub instructions: Vec<Instruction>,
    pub effects: Vec<String>, // Required effects
}

/// Complete bytecode program
#[derive(Debug, Clone)]
pub struct Program {
    pub version: u32,
    pub constants: Vec<Constant>,
    pub functions: Vec<Function>,
    pub entry_point: u32, // Index of main function
}

impl Program {
    /// Serialize program to binary format
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&self.version.to_le_bytes());

        // Constants pool
        bytes.extend_from_slice(&(self.constants.len() as u32).to_le_bytes());
        for constant in &self.constants {
            match constant {
                Constant::Int(v) => {
                    bytes.push(0); // type tag
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                Constant::Float(v) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                Constant::String(s) => {
                    bytes.push(2);
                    bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(s.as_bytes());
                }
                Constant::Bool(v) => {
                    bytes.push(3);
                    bytes.push(if *v { 1 } else { 0 });
                }
                Constant::None => {
                    bytes.push(4);
                }
            }
        }

        // Functions
        bytes.extend_from_slice(&(self.functions.len() as u32).to_le_bytes());
        for func in &self.functions {
            // Function name
            bytes.extend_from_slice(&(func.name.len() as u32).to_le_bytes());
            bytes.extend_from_slice(func.name.as_bytes());

            // Arity and locals
            bytes.extend_from_slice(&func.arity.to_le_bytes());
            bytes.extend_from_slice(&func.locals.to_le_bytes());

            // Instructions
            bytes.extend_from_slice(&(func.instructions.len() as u32).to_le_bytes());
            for inst in &func.instructions {
                bytes.push(inst.op.clone() as u8);
                bytes.extend_from_slice(&inst.arg.unwrap_or(0).to_le_bytes());
                bytes.extend_from_slice(&inst.line.to_le_bytes());
            }

            // Effects
            bytes.extend_from_slice(&(func.effects.len() as u32).to_le_bytes());
            for effect in &func.effects {
                bytes.extend_from_slice(&(effect.len() as u32).to_le_bytes());
                bytes.extend_from_slice(effect.as_bytes());
            }
        }

        // Entry point
        bytes.extend_from_slice(&self.entry_point.to_le_bytes());

        bytes
    }

    /// Deserialize program from binary format
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 8 {
            return Err("Invalid bytecode: too short".to_string());
        }

        // Check magic
        if data[0..4] != MAGIC {
            return Err("Invalid bytecode: bad magic".to_string());
        }

        // Read version
        let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let mut pos = 8;

        // Read constants
        let num_constants = read_u32(data, &mut pos)?;
        let mut constants = Vec::new();
        for _ in 0..num_constants {
            let tag = read_u8(data, &mut pos)?;
            let constant = match tag {
                0 => {
                    let v = read_i64(data, &mut pos)?;
                    Constant::Int(v)
                }
                1 => {
                    let v = read_f64(data, &mut pos)?;
                    Constant::Float(v)
                }
                2 => {
                    let len = read_u32(data, &mut pos)? as usize;
                    let s = read_string(data, &mut pos, len)?;
                    Constant::String(s)
                }
                3 => {
                    let v = read_u8(data, &mut pos)?;
                    Constant::Bool(v != 0)
                }
                4 => Constant::None,
                _ => return Err(format!("Unknown constant type: {}", tag)),
            };
            constants.push(constant);
        }

        // Read functions
        let num_functions = read_u32(data, &mut pos)?;
        let mut functions = Vec::new();
        for _ in 0..num_functions {
            let name_len = read_u32(data, &mut pos)? as usize;
            let name = read_string(data, &mut pos, name_len)?;
            let arity = read_u32(data, &mut pos)?;
            let locals = read_u32(data, &mut pos)?;

            let num_instructions = read_u32(data, &mut pos)?;
            let mut instructions = Vec::new();
            for _ in 0..num_instructions {
                let op_byte = read_u8(data, &mut pos)?;
                let arg = read_u32(data, &mut pos)?;
                let line = read_u32(data, &mut pos)?;

                let op = match op_byte {
                    0 => OpCode::Push,
                    1 => OpCode::Pop,
                    2 => OpCode::Dup,
                    3 => OpCode::Swap,
                    4 => OpCode::Add,
                    5 => OpCode::Sub,
                    6 => OpCode::Mul,
                    7 => OpCode::Div,
                    8 => OpCode::Mod,
                    9 => OpCode::Neg,
                    10 => OpCode::Eq,
                    11 => OpCode::Ne,
                    12 => OpCode::Lt,
                    13 => OpCode::Le,
                    14 => OpCode::Gt,
                    15 => OpCode::Ge,
                    16 => OpCode::And,
                    17 => OpCode::Or,
                    18 => OpCode::Not,
                    19 => OpCode::Load,
                    20 => OpCode::Store,
                    21 => OpCode::LoadGlobal,
                    22 => OpCode::StoreGlobal,
                    23 => OpCode::Jump,
                    24 => OpCode::JumpIf,
                    25 => OpCode::JumpIfNot,
                    26 => OpCode::Call,
                    27 => OpCode::Return,
                    28 => OpCode::NewList,
                    29 => OpCode::ListPush,
                    30 => OpCode::NewMap,
                    31 => OpCode::MapInsert,
                    32 => OpCode::Index,
                    33 => OpCode::Field,
                    34 => OpCode::LlmCall,
                    35 => OpCode::LlmStream,
                    36 => OpCode::Print,
                    37 => OpCode::ReadLine,
                    38 => OpCode::ReadFile,
                    39 => OpCode::WriteFile,
                    40 => OpCode::TypeCheck,
                    41 => OpCode::Cast,
                    42 => OpCode::CheckEffect,
                    43 => OpCode::Trace,
                    44 => OpCode::Breakpoint,
                    45 => OpCode::Halt,
                    _ => return Err(format!("Unknown opcode: {}", op_byte)),
                };

                instructions.push(Instruction {
                    op,
                    arg: Some(arg),
                    line,
                });
            }

            let num_effects = read_u32(data, &mut pos)?;
            let mut effects = Vec::new();
            for _ in 0..num_effects {
                let eff_len = read_u32(data, &mut pos)? as usize;
                let eff = read_string(data, &mut pos, eff_len)?;
                effects.push(eff);
            }

            functions.push(Function {
                name,
                arity,
                locals,
                instructions,
                effects,
            });
        }

        // Read entry point
        let entry_point = read_u32(data, &mut pos)?;

        Ok(Program {
            version,
            constants,
            functions,
            entry_point,
        })
    }
}

// Helper functions for deserialization
fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8, String> {
    if *pos >= data.len() {
        return Err("Unexpected end of data".to_string());
    }
    let val = data[*pos];
    *pos += 1;
    Ok(val)
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > data.len() {
        return Err("Unexpected end of data reading u32".to_string());
    }
    let val = u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos += 4;
    Ok(val)
}

fn read_i64(data: &[u8], pos: &mut usize) -> Result<i64, String> {
    if *pos + 8 > data.len() {
        return Err("Unexpected end of data reading i64".to_string());
    }
    let val = i64::from_le_bytes([
        data[*pos],
        data[*pos + 1],
        data[*pos + 2],
        data[*pos + 3],
        data[*pos + 4],
        data[*pos + 5],
        data[*pos + 6],
        data[*pos + 7],
    ]);
    *pos += 8;
    Ok(val)
}

fn read_f64(data: &[u8], pos: &mut usize) -> Result<f64, String> {
    if *pos + 8 > data.len() {
        return Err("Unexpected end of data reading f64".to_string());
    }
    let val = f64::from_le_bytes([
        data[*pos],
        data[*pos + 1],
        data[*pos + 2],
        data[*pos + 3],
        data[*pos + 4],
        data[*pos + 5],
        data[*pos + 6],
        data[*pos + 7],
    ]);
    *pos += 8;
    Ok(val)
}

fn read_string(data: &[u8], pos: &mut usize, len: usize) -> Result<String, String> {
    if *pos + len > data.len() {
        return Err("Unexpected end of data reading string".to_string());
    }
    let s = String::from_utf8(data[*pos..*pos + len].to_vec())
        .map_err(|e| format!("Invalid UTF-8: {}", e))?;
    *pos += len;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcodes() {
        // Ensure all opcodes are unique
        let opcodes = [
            OpCode::Push,
            OpCode::Pop,
            OpCode::Add,
            OpCode::Sub,
            OpCode::Mul,
            OpCode::Div,
            OpCode::Eq,
            OpCode::Ne,
            OpCode::Lt,
            OpCode::Gt,
            OpCode::And,
            OpCode::Or,
            OpCode::Not,
            OpCode::Jump,
            OpCode::Call,
            OpCode::Return,
            OpCode::Halt,
        ];

        for (i, a) in opcodes.iter().enumerate() {
            for (j, b) in opcodes.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_program_serialization() {
        let program = Program {
            version: VERSION,
            constants: vec![
                Constant::Int(42),
                Constant::Float(std::f64::consts::PI),
                Constant::String("hello".to_string()),
                Constant::Bool(true),
                Constant::None,
            ],
            functions: vec![Function {
                name: "main".to_string(),
                arity: 0,
                locals: 0,
                instructions: vec![
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
                effects: vec!["IO".to_string()],
            }],
            entry_point: 0,
        };

        let bytes = program.to_bytes();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], &MAGIC);
    }
}
