//! WASI replay runner for Nudge programs.
//!
//! Executes a compiled `StandaloneBundle` offline against a recorded NTF `.jsonl` trace
//! inside WebAssembly with zero network and zero child processes.

use nudge_agent::{load_bundle, parse_replay_args};
use std::path::Path;
use std::process;

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let replay_args = match parse_replay_args(&raw_args) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    let bundle = match load_bundle(Path::new(&replay_args.bundle_path)) {
        Ok(b) => b,
        Err(e) => {
            if e.starts_with("error") {
                eprintln!("{e}");
            } else {
                eprintln!("error: {e}");
            }
            process::exit(1);
        }
    };

    if replay_args.trace_path != "--live"
        && replay_args.trace_path != "none"
        && !replay_args.trace_path.is_empty()
    {
        std::env::set_var("NUDGE_REPLAY", &replay_args.trace_path);
    }

    let cli_args: Vec<nudgec::eval::NudgeVal> = replay_args
        .program_args
        .iter()
        .map(|a| nudgec::eval::parse_cli_arg(a))
        .collect();

    match nudgec::eval::run_program(&bundle.items, &bundle.entry, &cli_args) {
        Ok(val) => {
            if val != nudgec::eval::NudgeVal::Unit {
                println!("{val}");
            }
        }
        Err(e) => {
            if e.starts_with("error") {
                eprintln!("{e}");
            } else {
                eprintln!("error: {e}");
            }
            process::exit(1);
        }
    }
}
