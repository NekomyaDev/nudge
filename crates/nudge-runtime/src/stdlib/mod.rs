//! Standard library modules
//!
//! Provides built-in functions for I/O, math, string operations, and more.

pub mod computer;
pub mod env;
pub mod io;
pub mod llm;
pub mod math;
pub mod os;
pub mod store;
pub mod string;
pub mod time;
pub mod workspace;

#[cfg(test)]
mod tests;

use crate::value::Value;
use std::collections::HashMap;

/// Register all standard library functions
pub fn register_all() -> HashMap<String, Value> {
    let mut functions = HashMap::new();

    // Register io functions
    functions.extend(io::register());

    // Register workspace functions (safe, sandboxed filesystem)
    functions.extend(workspace::register());

    // Register store functions (crash-safe persistent app data store)
    functions.extend(store::register());

    // Register math functions
    functions.extend(math::register());

    // Register string functions
    functions.extend(string::register());

    // Register computer-use functions (v1.5, docs/computer-use.md)
    functions.extend(computer::register());

    // Register os functions
    functions.extend(os::register());

    // Register time functions
    functions.extend(time::register());

    // Register environment variable functions
    functions.extend(env::register());

    // Register LLM functions
    functions.extend(llm::register());

    functions
}

/// Execute a standard library function
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    if name.starts_with("io.") {
        io::execute(name, args)
    } else if name.starts_with("workspace.") {
        workspace::execute(name, args)
    } else if name.starts_with("math.") {
        math::execute(name, args)
    } else if name.starts_with("str.") {
        string::execute(name, args)
    } else if name.starts_with("computer.") {
        computer::execute(name, args)
    } else if name.starts_with("os.") {
        os::execute(name, args)
    } else if name.starts_with("time.") {
        time::execute(name, args)
    } else if name.starts_with("env.") {
        env::execute(name, args)
    } else if name.starts_with("llm.") {
        llm::execute(name, args)
    } else {
        Err(format!("Unknown standard library function: {}", name))
    }
}
