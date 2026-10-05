//! Standard library modules
//!
//! Provides built-in functions for I/O, math, string operations, and more.

pub mod computer;
pub mod io;
pub mod math;
pub mod string;

#[cfg(test)]
mod tests;

use crate::vm::Value;
use std::collections::HashMap;

/// Register all standard library functions
pub fn register_all() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    // Register io functions
    functions.extend(io::register());

    // Register math functions
    functions.extend(math::register());

    // Register string functions
    functions.extend(string::register());

    // Register computer-use functions (v1.5, docs/computer-use.md)
    functions.extend(computer::register());

    functions
}

/// Execute a standard library function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    if name.starts_with("io.") {
        io::execute(name, args)
    } else if name.starts_with("math.") {
        math::execute(name, args)
    } else if name.starts_with("str.") {
        string::execute(name, args)
    } else if name.starts_with("computer.") {
        computer::execute(name, args)
    } else {
        Err(format!("Unknown standard library function: {}", name))
    }
}
