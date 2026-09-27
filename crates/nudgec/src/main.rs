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
//!   nudgec trace-diff <a.jsonl> <b.jsonl> [--fail-on-regression]  compare two traces; with the flag, exit 1 on regression (v1.2)

mod a2a;
mod ast;
mod check;
mod codegen;
mod codegen_ts;
mod cost;
mod dap;
mod fuzz;
mod init;
mod json;
mod learn;
mod lexer;
mod lint;
mod lsp;
mod parser;
mod policysweep;
mod tracecheck;
mod tracediff;
mod traceview;

use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("nudgec {} — the Nudge compiler", env!("CARGO_PKG_VERSION"));
    eprintln!("usage:");
    eprintln!("  nudgec init  <name> [--template <t>] [--force]  scaffold a project from a template (`--list` to browse)");
    eprintln!(
        "  nudgec learn [lesson]     the language in six terminal lessons (run bare for the index)"
    );
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
    eprintln!("  nudgec trace-diff <a.jsonl> <b.jsonl> [--fail-on-regression]  compare traces; gate CI on regression");
    eprintln!("  nudgec policy-sweep <trace.jsonl> --question <q> [--metric confidence|p] [--thresholds 0.5,0.8]");
    eprintln!("  nudgec debug <t.jsonl>    step through a trace over DAP (Debug Adapter Protocol)");
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
    // `--version` / `-V`: print and exit 0 (was: usage banner + exit 64,
    // which broke version detection in scripts)
    if args.len() == 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("nudgec {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    // `init` scaffolds a project — no source file involved
    if args.len() >= 2 && args[1] == "init" {
        if let Err(e) = init::run(&args[2..]) {
            eprintln!("error: {e}");
            process::exit(1);
        }
        return;
    }
    // `learn` prints tutorial lessons — no source file involved
    if args.len() >= 2 && args[1] == "learn" {
        if let Err(e) = learn::run(&args[2..]) {
            eprintln!("error: {e}");
            process::exit(1);
        }
        return;
    }
    // `lsp` takes no file argument — it serves JSON-RPC over stdio
    if args.len() == 2 && args[1] == "lsp" {
        lsp::run();
        return;
    }
    // `trace-diff` takes two trace files; `--fail-on-regression` turns the
    // report into a CI gate (exit 1 when the candidate regresses)
    if args.len() >= 4 && args[1] == "trace-diff" {
        let a = read_src(&args[2]);
        let b = read_src(&args[3]);
        print!("{}", tracediff::diff(&a, &b));
        if args.len() >= 5 && args[4] == "--fail-on-regression" {
            let why = tracediff::regressions(&a, &b);
            if !why.is_empty() {
                for w in &why {
                    eprintln!("regression: {w}");
                }
                process::exit(1);
            }
        }
        return;
    }
    // `debug` speaks DAP over stdio over a recorded trace (no file arg parsing)
    if args.len() == 3 && args[1] == "debug" {
        let src = read_src(&args[2]);
        dap::run(&src);
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
                    port = args.get(i).and_then(|p| p.parse().ok()).unwrap_or_else(|| {
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
    // policy-sweep takes the trace plus flags (len > 3); everything else
    // is exactly <cmd> <file>
    if args.len() != 3 && args[1] != "policy-sweep" {
        usage();
    }
    // `resume` takes a run_id, not a source file
    let src = if args[1] == "resume" {
        String::new()
    } else {
        read_src(&args[2])
    };

    /// 1-based line:col of a byte offset, for human-facing diagnostics.
    fn line_col(src: &str, at: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for ch in src[..at.min(src.len())].chars() {
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

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
                        match e.span {
                            Some(sp) => {
                                let (l, c) = line_col(&src, sp.start);
                                eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                            }
                            None => eprintln!("error[{}]: {}", e.code, e.msg),
                        }
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
            Ok(items) => {
                // run the checker first: duplicate fn names would silently
                // overwrite each other's entries in the cost map, and
                // type-invalid programs shouldn't produce a cost report
                let errs = check::check(&items);
                if !errs.is_empty() {
                    for e in &errs {
                        eprintln!("error[{}]: {}", e.code, e.msg);
                    }
                    process::exit(1);
                }
                print!("{}", cost::report(&items))
            }
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
                        match e.span {
                            Some(sp) => {
                                let (l, c) = line_col(&src, sp.start);
                                eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                            }
                            None => eprintln!("error[{}]: {}", e.code, e.msg),
                        }
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
                        match e.span {
                            Some(sp) => {
                                let (l, c) = line_col(&src, sp.start);
                                eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                            }
                            None => eprintln!("error[{}]: {}", e.code, e.msg),
                        }
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
                        match e.span {
                            Some(sp) => {
                                let (l, c) = line_col(&src, sp.start);
                                eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                            }
                            None => eprintln!("error[{}]: {}", e.code, e.msg),
                        }
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
        // v1.4: policy sweep — re-cut decision thresholds over recorded
        // distributions; zero model calls (design §11.5)
        "policy-sweep" => {
            let src = read_src(&args[2]);
            let mut question = String::new();
            let mut metric = "confidence".to_string();
            let mut thresholds: Vec<f64> = vec![0.5, 0.8, 0.95];
            let mut i = 3; // args[2] is the trace file
            while i < args.len() {
                match args[i].as_str() {
                    "--question" | "-q" => {
                        i += 1;
                        question = args.get(i).cloned().unwrap_or_default();
                    }
                    "--metric" | "-m" => {
                        i += 1;
                        metric = args.get(i).cloned().unwrap_or(metric);
                    }
                    "--thresholds" | "-t" => {
                        i += 1;
                        thresholds = args
                            .get(i)
                            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
                            .unwrap_or(thresholds);
                    }
                    other => {
                        eprintln!("unknown policy-sweep flag '{other}' (use --question, --metric, --thresholds)");
                        process::exit(64);
                    }
                }
                i += 1;
            }
            if question.is_empty() {
                eprintln!("usage: nudgec policy-sweep <trace.jsonl> --question <name> [--metric confidence|p] [--thresholds 0.5,0.8]");
                process::exit(64);
            }
            print!(
                "{}",
                policysweep::sweep(&src, &question, &metric, &thresholds)
            );
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
