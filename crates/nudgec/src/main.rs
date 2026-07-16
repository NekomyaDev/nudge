//! nudgec — the Nudge compiler driver (MVP day 1–3).
//! Usage: nudgec lex <file.ndg>   (dump token stream; parser lands next)

mod lexer;

use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || args[1] != "lex" {
        eprintln!("nudgec 0.1.0 — the Nudge compiler");
        eprintln!("usage: nudgec lex <file.ndg>");
        process::exit(64);
    }

    let src = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => { eprintln!("error: cannot read {}: {e}", args[2]); process::exit(1); }
    };

    match lexer::lex(&src) {
        Ok(tokens) => {
            for t in &tokens {
                println!("{:>6}..{:<6} {:?}", t.start, t.end, t.tok);
            }
        }
        Err(e) => {
            eprintln!("error[E0001]: {} at byte {}", e.msg, e.at);
            process::exit(1);
        }
    }
}
