//! nudgec — the Nudge compiler driver.
//!   nudgec lex   <file.ndg>   dump token stream
//!   nudgec parse <file.ndg>   dump AST
//!   nudgec check <file.ndg>   type-check (E0101–E0302)
//!   nudgec build <file.ndg>   check, then emit Python to out/<name>.py
//!   nudgec build-ts <file.ndg> check, then emit TypeScript to out/<name>.ts (v0.3c)
//!   nudgec cost  <file.ndg>   static cost report per fn (v0.4, design §13)
//!   nudgec test  <file.ndg>   check, emit, then run every nudge_test_* fn
//!   nudgec resume <run_id>    continue a crashed run from its last checkpoint (design §7)
//!   nudgec trace-check <t.jsonl> validate a trace against the frozen v1 schema (v1.0, design §6)
//!   nudgec a2a   <file.ndg>   emit A2A agent card(s) to out/<name>.agent.json (v1.0, design §9)
//!   nudgec lsp                serve the Language Server Protocol over stdio (v1.0, design §10)
//!   nudgec trace-view <t.jsonl> [--port N] [--no-open]  local web UI for a trace (v1.2)

mod a2a;
mod ast;
mod check;
mod codegen;
mod codegen_ts;
mod cost;
mod fuzz;
mod json;
mod lexer;
mod lint;
mod lsp;
mod parser;
mod tracecheck;
mod traceview;

use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("nudgec 1.0.0 — the Nudge compiler");
    eprintln!("usage:");
    eprintln!("  nudgec lex   <file.ndg>   dump token stream");
    eprintln!("  nudgec parse <file.ndg>   dump AST");
    eprintln!("  nudgec check <file.ndg>   type-check (E0101–E0302)");
    eprintln!("  nudgec build <file.ndg>   check, then emit Python to out/<name>.py");
    eprintln!("  nudgec build-ts <file.ndg> check, then emit TypeScript to out/<name>.ts");
    eprintln!("  nudgec cost  <file.ndg>   static cost report per fn");
    eprintln!("  nudgec test  <file.ndg>   check, emit, then run every nudge_test_* fn");
    eprintln!("  nudgec resume <run_id>    continue a crashed run from its last checkpoint");
    eprintln!("  nudgec trace-check <t.jsonl> validate a trace against the frozen v1 schema");
    eprintln!("  nudgec a2a   <file.ndg>   emit A2A agent card(s) to out/<name>.agent.json");
    eprintln!("  nudgec lsp                serve the Language Server Protocol over stdio");
    eprintln!("  nudgec trace-view <t.jsonl> [--port N] [--no-open]  local web UI for a trace");
    process::exit(64);
}

fn read_src(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // `lsp` takes no file argument — it serves JSON-RPC over stdio
    if args.len() == 2 && args[1] == "lsp" {
        lsp::run();
        return;
    }
    // `trace-view` takes a trace file plus optional --port/--no-open flags
    if args.len() >= 3 && args[1] == "trace-view" {
        let mut port = traceview::DEFAULT_PORT;
        let mut no_open = false;
        let mut i = 3;
        while i < args.len() {
            match args[i].as_str() {
                "--no-open" => no_open = true,
                "--port" => {
                    i += 1;
                    port = args
                        .get(i)
                        .and_then(|p| p.parse().ok())
                        .unwrap_or_else(|| {
                            eprintln!("error: --port requires a number");
                            process::exit(64);
                        });
                }
                other => {
                    eprintln!("error: unknown flag '{other}'");
                    process::exit(64);
                }
            }
            i += 1;
        }
        let src = read_src(&args[2]);
        traceview::run(&args[2], &src, port, no_open);
    }
    if args.len() != 3 {
        usage();
    }
    // `resume` takes a run_id, not a source file
    let src = if args[1] == "resume" {
        String::new()
    } else {
        read_src(&args[2])
    };

    fn print_lints(items: &[ast::Item]) {
        for l in lint::lint_items(items) {
            eprintln!("warning[{}]: {}", l.code, l.msg);
        }
    }
    let compile = |src: &str| -> Result<Vec<ast::Item>, (String, usize)> {
        lexer::lex(src)
            .map_err(|e| (e.msg, e.at))
            .and_then(|t| parser::parse(t).map_err(|e| (e.msg, e.at)))
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
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
                process::exit(1);
            }
        },
        "check" => match compile(&src) {
            Ok(items) => {
                let errs = check::check(&items);
                if errs.is_empty() {
                    print_lints(&items);
                    eprintln!("-- checked {} item(s): OK", items.len());
                } else {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
            }
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
                process::exit(1);
            }
        },
        // design §13: static cost report (v0.4) — parse, count llm call
        // sites per fn under flat fake pricing
        "cost" => match compile(&src) {
            Ok(items) => print!("{}", cost::report(&items)),
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
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
                print_lints(&items);
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
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
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
                print_lints(&items);
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
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
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
                print_lints(&items);
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
                // exactly like they will under `nudge test`. NUDGE_PROGRAM
                // points agent-state registration at the emitted module,
                // not this driver (resume correctness, design §7).
                match process::Command::new("python3")
                    .arg(&driver)
                    .env("NUDGE_PROGRAM", &abs)
                    .status()
                {
                    Ok(status) => process::exit(status.code().unwrap_or(1)),
                    Err(e) => {
                        eprintln!("error: cannot run python3: {e}");
                        process::exit(1);
                    }
                }
            }
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
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
                        eprintln!(
                            "error: unknown run_id '{run}' (no {} in {})",
                            name,
                            dir.display()
                        );
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
        // design §6 (v1.0): validate a trace against the frozen v1 schema
        "trace-check" => {
            let problems = tracecheck::validate(&src);
            if problems.is_empty() {
                let n = src.lines().filter(|l| !l.trim().is_empty()).count();
                eprintln!("-- trace OK ({n} record(s), frozen v1 schema)");
            } else {
                for p in &problems {
                    eprintln!("error: {p}");
                }
                process::exit(1);
            }
        }
        // design §9 (v1.0): A2A agent-card export — one card per agent
        // block, or a single card wrapping the file's top-level fns
        "a2a" => match compile(&src) {
            Ok(items) => {
                let stem = std::path::Path::new(&args[2])
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("agent");
                if let Err(e) = fs::create_dir_all("out") {
                    eprintln!("error: cannot create out/: {e}");
                    process::exit(1);
                }
                for (name, card) in a2a::cards(&items, stem) {
                    let path = std::path::Path::new("out").join(format!("{name}.agent.json"));
                    match fs::write(&path, json::dumps(&card) + "\n") {
                        Ok(()) => println!("wrote {}", path.display()),
                        Err(e) => {
                            eprintln!("error: cannot write {}: {e}", path.display());
                            process::exit(1);
                        }
                    }
                }
            }
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
                process::exit(1);
            }
        },
        _ => usage(),
    }
}
