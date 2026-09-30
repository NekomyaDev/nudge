//! Nudge Runtime
//!
//! Standalone bytecode interpreter for the Nudge programming language.
//!
//! Usage:
//!   nudge run <file.ndg>     Run a compiled Nudge program
//!   nudge build <file.ndg>   Compile Nudge to bytecode
//!   nudge check <file.ndg>   Type-check a Nudge program
//!   nudge --version          Show version

mod bytecode;
mod codegen;
mod parser;
mod stdlib;
mod vm;

use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("nudge {} — Nudge runtime", env!("CARGO_PKG_VERSION"));
    eprintln!("usage:");
    eprintln!("  nudge run <file.ndg>     Run a compiled Nudge program");
    eprintln!("  nudge build <file.ndg>   Compile Nudge to bytecode");
    eprintln!("  nudge check <file.ndg>   Type-check a Nudge program");
    eprintln!("  nudge --version          Show version");
    process::exit(64);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        usage();
    }

    match args[1].as_str() {
        "--version" | "-v" => {
            println!("nudge {}", env!("CARGO_PKG_VERSION"));
        }

        "--help" | "-h" => {
            println!("nudge {} — Nudge runtime", env!("CARGO_PKG_VERSION"));
            println!("usage:");
            println!("  nudge run <file.ndg>     Run a compiled Nudge program");
            println!("  nudge build <file.ndg>   Compile Nudge to bytecode");
            println!("  nudge check <file.ndg>   Type-check a Nudge program");
            println!("  nudge --version          Show version");
        }

        "run" => {
            if args.len() != 3 {
                eprintln!("error: nudge run requires a file argument");
                process::exit(1);
            }
            let path = &args[2];
            let data = fs::read(path).unwrap_or_else(|e| {
                eprintln!("error: cannot read {}: {}", path, e);
                process::exit(1);
            });

            let program = bytecode::Program::from_bytes(&data).unwrap_or_else(|e| {
                eprintln!("error: invalid bytecode: {}", e);
                process::exit(1);
            });

            let mut runtime = vm::VM::new(program);
            if env::var("NUDGE_TRACE").is_ok() {
                runtime.enable_trace();
            }

            if let Err(e) = runtime.run() {
                eprintln!("runtime error: {}", e);
                process::exit(1);
            }
        }

        "build" => {
            if args.len() != 3 {
                eprintln!("error: nudge build requires a file argument");
                process::exit(1);
            }
            let path = &args[2];
            let source = fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("error: cannot read {}: {}", path, e);
                process::exit(1);
            });

            // Parse source
            let program_def = parser::parse(&source).unwrap_or_else(|e| {
                eprintln!("error: parse error at line {}: {}", e.line, e.message);
                process::exit(1);
            });

            // Compile to bytecode
            let mut compiler = codegen::Compiler::new();
            let program = compiler.compile_program(&program_def);

            // Dump if requested
            if env::var("NUDGE_DUMP").is_ok() {
                for func in program.functions.iter() {
                    eprintln!(
                        "fn {} (arity={}, locals={}):",
                        func.name, func.arity, func.locals
                    );
                    for (j, inst) in func.instructions.iter().enumerate() {
                        eprintln!("  {:3}: {:?} arg={:?}", j, inst.op, inst.arg);
                    }
                }
                eprintln!("\nConstants:");
                for (i, c) in program.constants.iter().enumerate() {
                    eprintln!("  {}: {:?}", i, c);
                }
            }

            // Serialize
            let bytes = program.to_bytes();

            // Write output
            let stem = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out");
            let out_path = format!("{}.ndgb", stem);
            fs::write(&out_path, &bytes).unwrap_or_else(|e| {
                eprintln!("error: cannot write {}: {}", out_path, e);
                process::exit(1);
            });

            println!("wrote {}", out_path);
        }

        "check" => {
            if args.len() != 3 {
                eprintln!("error: nudge check requires a file argument");
                process::exit(1);
            }
            // TODO: Implement type checking
            eprintln!("error: nudge check not yet implemented");
            process::exit(1);
        }

        _ => {
            eprintln!("error: unknown command '{}'", args[1]);
            usage();
        }
    }
}
