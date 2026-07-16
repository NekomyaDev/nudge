//! nudgec — the Nudge compiler driver (MVP day 1–3).
//!   nudgec lex   <file.ndg>   dump token stream
//!   nudgec parse <file.ndg>   dump AST
//!   nudgec build <file.ndg>   emit Python to out/<name>.py

mod ast;
mod codegen;
mod lexer;
mod parser;

use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("nudgec 0.1.0 — the Nudge compiler");
    eprintln!("usage:");
    eprintln!("  nudgec lex   <file.ndg>   dump token stream");
    eprintln!("  nudgec parse <file.ndg>   dump AST");
    eprintln!("  nudgec build <file.ndg>   emit Python to out/<name>.py");
    process::exit(64);
}

fn read_src(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { eprintln!("error: cannot read {path}: {e}"); process::exit(1); }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        usage();
    }
    let src = read_src(&args[2]);

    match args[1].as_str() {
        "lex" => match lexer::lex(&src) {
            Ok(tokens) => {
                for t in &tokens {
                    println!("{:>6}..{:<6} {:?}", t.start, t.end, t.tok);
                }
            }
            Err(e) => {
                eprintln!("error[E0001]: {} at byte {}", e.msg, e.at);
                process::exit(1);
            }
        },
        "parse" => match lexer::lex(&src).map_err(|e| e.msg).and_then(|t| parser::parse(t).map_err(|e| e.msg)) {
            Ok(items) => {
                for item in &items {
                    println!("{item:#?}");
                }
                eprintln!("-- parsed {} item(s) OK", items.len());
            }
            Err(msg) => {
                eprintln!("error[E0002]: {msg}");
                process::exit(1);
            }
        },
        "build" => match lexer::lex(&src).map_err(|e| e.msg).and_then(|t| parser::parse(t).map_err(|e| e.msg)) {
            Ok(items) => {
                let py = codegen::emit(&items);
                let stem = std::path::Path::new(&args[2])
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("out");
                if let Err(e) = fs::create_dir_all("out") {
                    eprintln!("error: cannot create out/: {e}");
                    process::exit(1);
                }
                let path = std::path::Path::new("out").join(format!("{stem}.py"));
                match fs::write(&path, py) {
                    Ok(()) => println!("wrote {}", path.display()),
                    Err(e) => {
                        eprintln!("error: cannot write {}: {e}", path.display());
                        process::exit(1);
                    }
                }
            }
            Err(msg) => {
                eprintln!("error[E0002]: {msg}");
                process::exit(1);
            }
        },
        _ => usage(),
    }
}
