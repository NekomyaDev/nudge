//! `nudgec` compiler and evaluation library.
#![allow(clippy::new_without_default)]

pub mod a2a;
pub mod ast;
pub mod check;
pub mod codegen;
pub mod codegen_ts;
pub mod compile;
pub mod cost;
pub mod dap;
pub mod eval;
pub mod evalcmd;
pub mod fmt;
pub mod fuzz;
pub mod hints;
pub mod init;
pub mod json;
pub mod learn;
pub mod lexer;
pub mod lint;
pub mod lsp;
pub mod mcpserver;
pub mod parser;
pub mod policysweep;
pub mod recipes;
pub mod runs;
pub mod serve;
pub mod standalone;
pub mod tracecheck;
pub mod tracediff;
pub mod traceexplain;
pub mod traceview;
pub mod watch;

pub use compile::compile;
pub use nudge_runtime;
