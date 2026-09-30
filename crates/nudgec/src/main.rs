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
mod eval;
mod fmt;
mod fuzz;
mod hints;
mod init;
mod json;
mod learn;
mod lexer;
mod lint;
mod lsp;
mod mcpserver;
mod parser;
mod policysweep;
mod recipes;
mod runs;
mod serve;
mod tracecheck;
mod tracediff;
mod traceexplain;
mod traceview;
mod watch;

use std::{env, fs, process};

fn print_usage_to(to_stderr: bool) {
    let msg = format!(
        "nudgec {} — the Nudge compiler\nusage:\n  nudgec fmt   <file.ndg> [--check]   normalize indentation, trim trailing ws, collapse blanks\n  nudgec init  <name> [--template <t>] [--force]  scaffold a project from a template (`--list` to browse)\n  nudgec learn [lesson]     the language in six terminal lessons (run bare for the index)\n  nudgec lex   <file.ndg>   dump token stream\n  nudgec parse <file.ndg>   dump AST\n  nudgec check <file.ndg>   type-check (E0101–E0302)\n  nudgec build <file.ndg>   check, then emit Python to out/<name>.py\n  nudgec build-ts <file.ndg> check, then emit TypeScript to out/<name>.ts\n  nudgec check <file.ndg> --watch   re-check on every file change (Ctrl-C to stop)\n  nudgec cost  <file.ndg>   static cost report per fn\n  nudgec test  <file.ndg>   check, emit, then run every nudge_test_* fn\n  nudgec resume <run_id>    continue a crashed run from its last checkpoint\n  nudgec trace-check <t.jsonl> validate a trace against the frozen v1 schema\n  nudgec a2a   <file.ndg>   emit A2A agent card(s) to out/<name>.agent.json\n  nudgec lsp                serve the Language Server Protocol over stdio\n  nudgec trace-html <t.jsonl> [--out file.html]  static single-file viewer (no server, works offline)\n  nudgec trace-view <t.jsonl> [--port N] [--no-open]  local web UI for a trace\n  nudgec explain <t.jsonl>    human report over a trace: totals, failures, low-confidence answers\n  nudgec trace-diff <a.jsonl> <b.jsonl> [--fail-on-regression]  compare traces; gate CI on regression\n  nudgec policy-sweep <trace.jsonl> --question <q> [--metric confidence|p] [--thresholds 0.5,0.8]\n  nudgec compare <results_a.jsonl> <results_b.jsonl>  two eval runs: which rows flipped\n  nudgec runs  [--run <run_id>]   list recorded agent runs / show one run's state and trace\n  nudgec mcp   <file.ndg> [--fns a,b]   expose program fns as MCP tools over stdio\n  nudgec serve <file.ndg> [--fn <name>] [--port N]   run a program fn as a local HTTP API (POST /run, GET /health)\n  nudgec eval  <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8]  score a program over a dataset\n  nudgec debug <t.jsonl>    step through a trace over DAP (Debug Adapter Protocol)",
        env!("CARGO_PKG_VERSION")
    );
    if to_stderr {
        eprintln!("{msg}");
    } else {
        println!("{msg}");
    }
}

fn usage() -> ! {
    print_usage_to(true);
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

fn print_hint(code: &str) {
    if let Some(h) = hints::hint(code) {
        eprintln!("  hint: {h}");
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
    // `--help` / `-h`: print usage and exit 0
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        print_usage_to(false);
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
    // `check --watch` re-runs the checker on every file change (B4)
    if args.len() >= 3 && args[1] == "check" && args.iter().any(|a| a == "--watch") {
        let file = match args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-') && a.as_str() != "check")
        {
            Some(f) => f.clone(),
            None => usage(),
        };
        watch::run(&file);
    }
    // `fmt` normalizes indentation; --check reports without writing
    if args.len() >= 3 && args[1] == "fmt" {
        let check = args.iter().any(|a| a == "--check");
        let file = match args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-') && a.as_str() != "fmt")
        {
            Some(f) => f.clone(),
            None => usage(),
        };
        let src = read_src(&file);
        let action = || -> Result<bool, String> {
            if check {
                crate::fmt::needs_formatting(&src)
            } else {
                crate::fmt::fmt(&src).map(|out| {
                    let changed = out != src;
                    if changed {
                        fs::write(&file, &out).map_err(|e| format!("cannot write {file}: {e}"))?;
                    }
                    Ok(changed)
                })?
            }
        };
        match action() {
            Err(e) => {
                eprintln!("error: {e}");
                process::exit(1);
            }
            Ok(true) => {
                if check {
                    println!("{file}: needs formatting");
                    process::exit(1);
                } else {
                    println!("{file}: formatted");
                }
            }
            Ok(false) => println!("{file}: already formatted"),
        }
        return;
    }
    // `trace-html` exports the viewer as a static single file
    if args.len() >= 3 && args[1] == "trace-html" {
        let src = read_src(&args[2]);
        let problems = tracecheck::validate(&src);
        if !problems.is_empty() {
            for p in &problems {
                eprintln!("error: {p}");
            }
            eprintln!("error: trace does not conform to the frozen v1 schema — run `nudgec trace-check` for details");
            process::exit(1);
        }
        let out = if args.len() == 5 && (args[3] == "--out" || args[3] == "-o") {
            args[4].clone()
        } else if args.len() == 3 {
            std::path::Path::new(&args[2])
                .file_stem()
                .map(|s| format!("{}.html", s.to_string_lossy()))
                .unwrap_or_else(|| "trace.html".to_string())
        } else {
            usage();
        };
        let html = traceview::static_html(&src, &args[2]);
        if let Err(e) = fs::write(&out, &html) {
            eprintln!("error: cannot write {out}: {e}");
            process::exit(1);
        }
        println!(
            "wrote {out} ({} record(s)) — open it in any browser",
            src.lines().filter(|l| !l.trim().is_empty()).count()
        );
        return;
    }
    // `explain` turns a recorded trace into a human report
    if args.len() == 3 && args[1] == "explain" {
        print!("{}", traceexplain::explain(&read_src(&args[2])));
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
    if args.len() != 3
        && args[1] != "policy-sweep"
        && args[1] != "serve"
        && args[1] != "mcp"
        && args[1] != "eval"
        && args[1] != "compare"
        && args[1] != "runs"
        && args[1] != "lint"
    {
        usage();
    }
    // `resume` takes a run_id, not a source file
    let src = if args[1] == "resume" || args[1] == "runs" {
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
                print_hint("E0001");
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
                print_hint("E0002");
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
                                {
                                    eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                                    print_hint(e.code);
                                }
                            }
                            None => {
                                eprintln!("error[{}]: {}", e.code, e.msg);
                                print_hint(e.code);
                            }
                        }
                    }
                    process::exit(1);
                }
            }
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
                print_hint("E0002");
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
                        {
                            eprintln!("error[{}]: {}", e.code, e.msg);
                            print_hint(e.code);
                        }
                    }
                    process::exit(1);
                }
                print!("{}", cost::report(&items))
            }
            Err((msg, at)) => {
                eprintln!("error[E0002]: {msg} at byte {at}");
                print_hint("E0002");
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
                                {
                                    eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                                    print_hint(e.code);
                                }
                            }
                            None => {
                                eprintln!("error[{}]: {}", e.code, e.msg);
                                print_hint(e.code);
                            }
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
                print_hint("E0002");
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
                                {
                                    eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                                    print_hint(e.code);
                                }
                            }
                            None => {
                                eprintln!("error[{}]: {}", e.code, e.msg);
                                print_hint(e.code);
                            }
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
                print_hint("E0002");
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
                                {
                                    eprintln!("error[{}] at {l}:{c}: {}", e.code, e.msg);
                                    print_hint(e.code);
                                }
                            }
                            None => {
                                eprintln!("error[{}]: {}", e.code, e.msg);
                                print_hint(e.code);
                            }
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
                print_hint("E0002");
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
        "eval" => {
            // C1: run a fn over a JSONL dataset, score it, optional CI gate
            if args.len() < 3 {
                eprintln!("usage: nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8]");
                process::exit(64);
            }
            let items = match compile(&src) {
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
                            print_hint(e.code);
                        }
                        process::exit(1);
                    }
                    print_lints(&items);
                    items
                }
                Err((msg, at)) => {
                    eprintln!("error[E0002]: {msg} at byte {at}");
                    print_hint("E0002");
                    process::exit(1);
                }
            };
            let mut dataset: Option<String> = None;
            let mut fname = String::from("main");
            let mut path = String::new();
            let mut min_acc: Option<f64> = None;
            let mut i = 3usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--dataset" | "-d" => {
                        i += 1;
                        dataset = args.get(i).cloned();
                    }
                    "--fn" | "-f" => {
                        i += 1;
                        fname = args.get(i).cloned().unwrap_or_default();
                    }
                    "--path" | "-p" => {
                        i += 1;
                        path = args.get(i).cloned().unwrap_or_default();
                    }
                    "--min-accuracy" | "-a" => {
                        i += 1;
                        min_acc = args.get(i).and_then(|v| v.parse::<f64>().ok());
                    }
                    other => {
                        eprintln!("error: unknown eval argument '{other}'");
                        process::exit(64);
                    }
                }
                i += 1;
            }
            let Some(dataset) = dataset else {
                eprintln!("error: eval needs --dataset <rows.jsonl>");
                process::exit(64);
            };
            let code = eval::run_cli(&items, &args[2], &dataset, &fname, &path, min_acc);
            process::exit(code);
        }
        "compare" => {
            // C2: two eval runs, row-level flips
            if args.len() != 4 {
                eprintln!("usage: nudgec compare <results_a.jsonl> <results_b.jsonl>");
                process::exit(64);
            }
            print!(
                "{}",
                eval::compare(&read_src(&args[2]), &read_src(&args[3]))
            );
        }
        "runs" => {
            // C8: inspect the local run store
            let detail = if args.len() == 4 && args[2] == "--run" {
                Some(args[3].as_str())
            } else if args.len() != 2 {
                eprintln!("usage: nudgec runs [--run <run_id>]");
                process::exit(64);
            } else {
                None
            };
            process::exit(runs::run(detail));
        }
        "serve" => {
            // D1: run a program fn as a local HTTP API (stdlib only)
            let mut fname = String::from("main");
            let mut port: u16 = 8080;
            let mut i = 3usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--fn" | "-f" => {
                        i += 1;
                        fname = args.get(i).cloned().unwrap_or_default();
                    }
                    "--port" | "-p" => {
                        i += 1;
                        port = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(8080);
                    }
                    other => {
                        eprintln!("error: unknown serve argument '{other}'");
                        process::exit(64);
                    }
                }
                i += 1;
            }
            let items = match compile(&src) {
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
                            print_hint(e.code);
                        }
                        process::exit(1);
                    }
                    print_lints(&items);
                    items
                }
                Err((msg, at)) => {
                    eprintln!("error[E0002]: {msg} at byte {at}");
                    print_hint("E0002");
                    process::exit(1);
                }
            };
            let stem = std::path::Path::new(&args[2])
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out");
            fs::create_dir_all("out").unwrap_or(());
            let module_path = std::path::Path::new("out").join(format!("{stem}.py"));
            if let Err(e) = fs::write(&module_path, codegen::emit(&items)) {
                eprintln!("error: cannot write {}: {e}", module_path.display());
                process::exit(1);
            }
            let abs = module_path.canonicalize().unwrap_or_else(|e| {
                eprintln!("error: cannot resolve {}: {e}", module_path.display());
                process::exit(1);
            });
            let server_path = std::path::Path::new("out").join(format!("{stem}_server.py"));
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let server_src = serve::SERVER_PY
                .replace("__MODULE__", &esc(&abs.to_string_lossy()))
                .replace("__FN__", &esc(&fname))
                .replace("__PORT__", &port.to_string());
            if let Err(e) = fs::write(&server_path, server_src) {
                eprintln!("error: cannot write {}: {e}", server_path.display());
                process::exit(1);
            }
            match process::Command::new("python3").arg(&server_path).status() {
                Ok(st) => process::exit(st.code().unwrap_or(0)),
                Err(e) => {
                    eprintln!("error: cannot run python3: {e}");
                    process::exit(1);
                }
            }
        }
        "mcp" => {
            // D2: expose the program's fns as MCP tools over stdio
            let mut fns = String::new();
            let mut i = 3usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--fns" | "-f" => {
                        i += 1;
                        fns = args.get(i).cloned().unwrap_or_default();
                    }
                    other => {
                        eprintln!("error: unknown mcp argument '{other}'");
                        process::exit(64);
                    }
                }
                i += 1;
            }
            let items = match compile(&src) {
                Ok(items) => items,
                Err((msg, at)) => {
                    eprintln!("error[E0002]: {msg} at byte {at}");
                    print_hint("E0002");
                    process::exit(1);
                }
            };
            let stem = std::path::Path::new(&args[2])
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out");
            fs::create_dir_all("out").unwrap_or(());
            let module_path = std::path::Path::new("out").join(format!("{stem}.py"));
            if let Err(e) = fs::write(&module_path, codegen::emit(&items)) {
                eprintln!("error: cannot write {}: {e}", module_path.display());
                process::exit(1);
            }
            let abs = module_path.canonicalize().unwrap_or_else(|e| {
                eprintln!("error: cannot resolve {}: {e}", module_path.display());
                process::exit(1);
            });
            let driver_path = std::path::Path::new("out").join(format!("{stem}_mcp.py"));
            let driver_src =
                mcpserver::driver(&abs.to_string_lossy(), env!("CARGO_PKG_VERSION"), &fns);
            if let Err(e) = fs::write(&driver_path, driver_src) {
                eprintln!("error: cannot write {}: {e}", driver_path.display());
                process::exit(1);
            }
            match process::Command::new("python3").arg(&driver_path).status() {
                Ok(st) => process::exit(st.code().unwrap_or(0)),
                Err(e) => {
                    eprintln!("error: cannot run python3: {e}");
                    process::exit(1);
                }
            }
        }
        "lint" => {
            // B5: lint with auto-fix
            let check = args.iter().any(|a| a == "--fix");
            let file = match args
                .iter()
                .skip(1)
                .find(|a| !a.starts_with('-') && a.as_str() != "lint")
            {
                Some(f) => f.clone(),
                None => usage(),
            };
            let items = match compile(&src) {
                Ok(items) => items,
                Err((msg, at)) => {
                    eprintln!("error[E0002]: {msg} at byte {at}");
                    print_hint("E0002");
                    process::exit(1);
                }
            };
            let lints = lint::lint_items(&items);
            if check {
                if lints.iter().any(|l| l.fix.is_some()) {
                    let fixed = lint::apply_fixes(&src, &lints);
                    if let Err(e) = fs::write(&file, &fixed) {
                        eprintln!("error: cannot write {file}: {e}");
                        process::exit(1);
                    }
                    println!(
                        "{file}: applied fixes — re-run nudgec lint {} to see what's left",
                        file
                    );
                    let remaining = lint::lint_items(&compile(&fixed).unwrap_or(items));
                    for l in &remaining {
                        eprintln!("warning[{}]: {}", l.code, l.msg);
                    }
                } else {
                    println!("{file}: nothing to auto-fix");
                }
            } else {
                for l in &lints {
                    eprintln!("warning[{}]: {}", l.code, l.msg);
                }
                println!(
                    "{} lint(s); run nudgec lint {} --fix to apply auto-fixes",
                    lints.len(),
                    file
                );
            }
        }
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
                print_hint("E0002");
                process::exit(1);
            }
        },
        _ => usage(),
    }
}
