//! nudgec — the Nudge compiler driver.
//!   nudgec lex   <file.ndg>   dump token stream
//!   nudgec parse <file.ndg>   dump AST
//!   nudgec check <file.ndg>   type-check (E0101–E0302)
//!   nudgec build <file.ndg>   check, then emit Python to out/<name>.py
//!   nudgec build-ts <file.ndg> check, then emit TypeScript to out/<name>.ts (v0.3c)
//!   nudgec cost  <file.ndg>   static cost report per fn (v0.4, design §13)
//!   nudgec test  <file.ndg>   check, emit, then run every nudge_test_* fn
//!   nudgec resume <run_id>    continue a crashed run from its last checkpoint (design §7)

mod ast;
mod check;
mod codegen;
mod codegen_ts;
mod cost;
mod lexer;
mod parser;

use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("nudgec 0.1.0 — the Nudge compiler");
    eprintln!("usage:");
    eprintln!("  nudgec lex   <file.ndg>   dump token stream");
    eprintln!("  nudgec parse <file.ndg>   dump AST");
    eprintln!("  nudgec check <file.ndg>   type-check (E0101–E0302)");
    eprintln!("  nudgec build <file.ndg>   check, then emit Python to out/<name>.py");
    eprintln!("  nudgec build-ts <file.ndg> check, then emit TypeScript to out/<name>.ts");
    eprintln!("  nudgec cost  <file.ndg>   static cost report per fn");
    eprintln!("  nudgec test  <file.ndg>   check, emit, then run every nudge_test_* fn");
    eprintln!("  nudgec resume <run_id>    continue a crashed run from its last checkpoint");
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
    // `resume` takes a run_id, not a source file
    let src = if args[1] == "resume" { String::new() } else { read_src(&args[2]) };

    let compile = |src: &str| -> Result<Vec<ast::Item>, String> {
        lexer::lex(src).map_err(|e| e.msg).and_then(|t| parser::parse(t).map_err(|e| e.msg))
    };

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
        "parse" => match compile(&src) {
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
        "check" => match compile(&src) {
            Ok(items) => {
                let errs = check::check(&items);
                if errs.is_empty() {
                    eprintln!("-- checked {} item(s): OK", items.len());
                } else {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
            }
            Err(msg) => {
                eprintln!("error[E0002]: {msg}");
                process::exit(1);
            }
        },
        // design §13: static cost report (v0.4) — parse, count llm call
        // sites per fn under flat fake pricing
        "cost" => match compile(&src) {
            Ok(items) => print!("{}", cost::report(&items)),
            Err(msg) => {
                eprintln!("error[E0002]: {msg}");
                process::exit(1);
            }
        },
        // design §14: TypeScript backend (v0.3c) — same pipeline, TS emit
        "build-ts" => match compile(&src) {
            Ok(items) => {
                let errs = check::check(&items);
                if !errs.is_empty() {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
                let ts = codegen_ts::emit_ts(&items);
                let stem = std::path::Path::new(&args[2])
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("out");
                if let Err(e) = fs::create_dir_all("out") {
                    eprintln!("error: cannot create out/: {e}");
                    process::exit(1);
                }
                let path = std::path::Path::new("out").join(format!("{stem}.ts"));
                match fs::write(&path, ts) {
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
        "build" => match compile(&src) {
            Ok(items) => {
                let errs = check::check(&items);
                if !errs.is_empty() {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
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
        "test" => match compile(&src) {
            Ok(items) => {
                let errs = check::check(&items);
                if !errs.is_empty() {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
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
                if let Err(e) = fs::write(&path, &py) {
                    eprintln!("error: cannot write {}: {e}", path.display());
                    process::exit(1);
                }
                let abs = match path.canonicalize() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("error: cannot resolve {}: {e}", path.display());
                        process::exit(1);
                    }
                };
                let driver = std::path::Path::new("out").join(format!("{stem}_nudge_tests.py"));
                let driver_src =
                    codegen::TEST_DRIVER_PY.replace("__MODULE__", &abs.to_string_lossy());
                if let Err(e) = fs::write(&driver, driver_src) {
                    eprintln!("error: cannot write {}: {e}", driver.display());
                    process::exit(1);
                }
                // cwd stays the user's: relative trace paths in tests resolve
                // exactly like they will under `nudge test`
                match process::Command::new("python3").arg(&driver).status() {
                    Ok(status) => process::exit(status.code().unwrap_or(1)),
                    Err(e) => {
                        eprintln!("error: cannot run python3: {e}");
                        process::exit(1);
                    }
                }
            }
            Err(msg) => {
                eprintln!("error[E0002]: {msg}");
                process::exit(1);
            }
        },
        // design §7: re-execute the registered program replaying the run's
        // recorded trace; once the recorded prefix is exhausted the runtime
        // goes live and appends to the same trace. State writes from the
        // replayed prefix are suppressed — the checkpoint reflects them.
        "resume" => {
            let run = &args[2];
            let dir = std::path::Path::new(".nudge").join("runs").join(run);
            let read = |name: &str| -> String {
                match fs::read_to_string(dir.join(name)) {
                    Ok(s) => s,
                    Err(_) => {
                        eprintln!("error: unknown run_id '{run}' (no {} in {})", name, dir.display());
                        process::exit(1);
                    }
                }
            };
            let program = read("program");
            let trace = read("trace");
            match process::Command::new("python3")
                .arg(program.trim())
                .env("NUDGE_PROVIDER", "fake")
                .env("NUDGE_RUN_ID", run)
                .env("NUDGE_REPLAY", trace.trim())
                .env("NUDGE_RESUME", "1")
                .env("NUDGE_TRACE", trace.trim())
                .status()
            {
                Ok(status) => process::exit(status.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("error: cannot run python3: {e}");
                    process::exit(1);
                }
            }
        }
        _ => usage(),
    }
}
