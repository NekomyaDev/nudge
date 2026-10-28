//! Fuzz smoke tests (v1.6): deterministic, dependency-free mutation fuzzing
//! of the lex -> parse -> check -> emit pipeline and the JSON parser.
//!
//! The harness drives mutated/derived inputs through the full
//! frontend and asserts exactly one thing: **no panics**. Clean errors are
//! the correct outcome for garbage input; a panic is a bug. Seeds are fixed
//! so failures reproduce byte-for-byte.

use crate::{check, codegen, codegen_ts, lexer, parser};

/// xorshift64* deterministic PRNG so fuzz runs are reproducible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x5EED_0001_DEAD_BEEF
        } else {
            seed
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

pub const SEEDS: &[&str] = &[
    include_str!("../../../examples/research_agent.ndg"),
    include_str!("../../../examples/checkpoint_agent.ndg"),
    include_str!("../../../examples/hello_llm.ndg"),
    "fn f() -> int { 1 }",
    "agent A {\n    state {\n        n: int = 0,\n    }\n    fn step() -> int uses LLM {\n        llm\"\"\"go {state.n}\"\"\" with { model: \"m\", budget: 0.01 USD }\n    }\n}",
];

pub const DEFAULT_SEEDS: &[&str] = SEEDS;

/// Returns the built-in seed programs as owned Strings.
pub fn default_seeds() -> Vec<String> {
    SEEDS.iter().map(|s| s.to_string()).collect()
}

/// Interesting single-token fragments the mutator splices in.
pub const FRAGMENTS: &[&str] = &[
    "\"\"\"",
    "llm\"\"\"",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "|",
    "->",
    "==",
    "!=",
    "<=",
    ">=",
    "+=",
    "-=",
    "=",
    ":",
    ",",
    ".",
    "@range(0, 1)",
    "state.",
    "0.02 USD",
    "\"",
    "\\",
    "\u{00e9}",
    "\u{1f600}",
    "par map",
    "merge",
    "zip",
    "route{",
    "when",
    "otherwise",
    "\n\n\n",
    "\t",
    "fn",
    "agent",
    "stream let",
    "with {",
    "budget:",
    "schema:",
    "999999999999999999999",
    "-",
    "0x",
    "1.2.3",
];

pub fn mutate(rng: &mut Rng, base: &str) -> String {
    let mut bytes = base.as_bytes().to_vec();
    let edits = 1 + rng.below(8);
    for _ in 0..edits {
        match rng.below(5) {
            // byte flip
            0 if !bytes.is_empty() => {
                let i = rng.below(bytes.len());
                bytes[i] = rng.next() as u8;
            }
            // splice a fragment
            1 => {
                let f = FRAGMENTS[rng.below(FRAGMENTS.len())].as_bytes();
                let i = rng.below(bytes.len() + 1);
                bytes.splice(i..i, f.iter().copied());
            }
            // delete a span
            2 if !bytes.is_empty() => {
                let i = rng.below(bytes.len());
                let len = (1 + rng.below(16)).min(bytes.len() - i);
                bytes.drain(i..i + len);
            }
            // duplicate a span
            3 if !bytes.is_empty() => {
                let i = rng.below(bytes.len());
                let len = (1 + rng.below(16)).min(bytes.len() - i);
                let chunk: Vec<u8> = bytes[i..i + len].to_vec();
                bytes.splice(i..i, chunk);
            }
            // truncate
            _ => {
                let keep = rng.below(bytes.len() + 1);
                bytes.truncate(keep);
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn random_string(rng: &mut Rng, len: usize) -> String {
    // printable ASCII + newlines + a few unicode curves
    (0..len)
        .map(|_| match rng.below(40) {
            0 => '\n',
            1 => '\u{1f600}',
            2 => '\u{00e9}',
            3 => '\\',
            n => (b' ' + ((n * 7) % 95) as u8) as char,
        })
        .collect()
}

/// One fuzz case must never panic: any Result is acceptable.
/// Runs through lex -> parse -> check -> codegen (Python) / codegen_ts (TypeScript).
pub fn fuzz_one(src: &str) -> Result<(), String> {
    let tokens = match lexer::lex(src) {
        Ok(t) => t,
        Err(e) => return Err(format!("lex error: {}", e.msg)),
    };
    let items = match parser::parse(tokens) {
        Ok(it) => it,
        Err(e) => return Err(format!("parse error at {}: {}", e.at, e.msg)),
    };
    let check_errs = check::check(&items);
    // codegen on unchecked garbage must stay panic-free too (the CLI
    // always checks first, but emit must not rely on that)
    let _ = codegen::emit(&items);
    let _ = codegen_ts::emit_ts(&items);

    if !check_errs.is_empty() {
        return Err(format!("check error: {}", check_errs[0].msg));
    }
    Ok(())
}

/// Backwards-compatible alias for driving one input through the compiler.
pub fn drive(src: &str) {
    let _ = fuzz_one(src);
}

use serde::{Deserialize, Serialize};

/// Information recorded when a fuzzed input causes an unexpected panic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzPanic {
    pub seed: u64,
    pub reproducing_seed: u64,
    pub snippet: String,
    pub mutated_snippet: String,
    pub message: String,
}

impl std::fmt::Display for FuzzPanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "panic with reproducing seed {}: {}\nsnippet:\n{}",
            self.seed, self.message, self.snippet
        )
    }
}

/// Summary report returned by `run_fuzzer`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzReport {
    pub total_iterations: usize,
    pub iterations: usize,
    pub mutations_tested: usize,
    pub errors_caught: usize,
    pub clean_errors: usize,
    pub clean_completions: usize,
    pub clean_rejections: usize,
    pub clean_rejections_completions: usize,
    pub panics: Vec<FuzzPanic>,
}

impl FuzzReport {
    pub fn is_success(&self) -> bool {
        self.panics.is_empty()
    }

    pub fn is_ok(&self) -> bool {
        self.panics.is_empty()
    }

    pub fn clean_rejections_and_completions(&self) -> usize {
        self.clean_rejections_completions
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl std::fmt::Display for FuzzReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "fuzzed {} inputs across lex/parse/check/codegen: {} panics, {} clean rejections/completions",
            self.mutations_tested,
            self.panics.len(),
            self.clean_rejections_completions
        )
    }
}

/// Runs the mutation fuzzer for the given number of iterations using deterministic PRNG seeds.
pub fn run_fuzzer(
    seed_programs: &[String],
    iterations: usize,
    seed: u64,
) -> Result<FuzzReport, String> {
    if iterations == 0 {
        return Err("iterations must be at least 1".to_string());
    }

    let effective_seeds: Vec<String> = if seed_programs.is_empty() {
        default_seeds()
    } else {
        seed_programs.to_vec()
    };

    if effective_seeds.is_empty() {
        return Err("no seed programs available".to_string());
    }

    let mut master_rng = Rng::new(seed);
    let mut report = FuzzReport {
        total_iterations: iterations,
        iterations,
        ..Default::default()
    };

    let mut current_seed = seed;
    for _ in 0..iterations {
        let iter_seed = current_seed;
        current_seed = master_rng.next();

        let mut iter_rng = Rng::new(iter_seed);
        let base = &effective_seeds[iter_rng.below(effective_seeds.len())];
        let src = if iter_rng.below(20) == 0 && seed_programs.is_empty() {
            let len = 32 + iter_rng.below(256);
            random_string(&mut iter_rng, len)
        } else {
            mutate(&mut iter_rng, base)
        };

        report.mutations_tested += 1;

        let panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fuzz_one(&src)));

        match panic_res {
            Ok(outcome) => match outcome {
                Ok(()) => {
                    report.clean_completions += 1;
                }
                Err(_) => {
                    report.errors_caught += 1;
                    report.clean_errors += 1;
                    report.clean_rejections += 1;
                }
            },
            Err(err) => {
                let msg = if let Some(s) = err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                report.panics.push(FuzzPanic {
                    seed: iter_seed,
                    reproducing_seed: iter_seed,
                    snippet: src.clone(),
                    mutated_snippet: src,
                    message: msg,
                });
            }
        }
    }

    report.clean_rejections_completions = report.errors_caught + report.clean_completions;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn fuzz_mutated_real_programs_never_panic() {
        let mut rng = Rng(0x5EED_0001);
        for round in 0..6000 {
            let base = SEEDS[rng.below(SEEDS.len())];
            let src = mutate(&mut rng, base);
            drive(&src);
            if round % 1000 == 999 {
                // mix in fully synthetic input too
                let len = 64 + rng.below(256);
                drive(&random_string(&mut rng, len));
            }
        }
    }

    #[test]
    fn test_fuzz_unicode_multibyte_string_slicing() {
        let mut rng = Rng(0x5EED_0005);
        let sample_chars = [
            'a', 'z', '0', '9', ' ', 'ç', 'ğ', 'ı', 'ö', 'ş', 'ü', '世', '界', '👋', '🦀', '🌍',
        ];
        for _ in 0..1000 {
            let len = 1 + rng.below(64);
            let mut s = String::new();
            for _ in 0..len {
                let idx = rng.below(sample_chars.len());
                s.push(sample_chars[idx]);
            }
            // Exercise compiler lexical parser with randomized multibyte inputs
            drive(&format!("fn f() -> string {{ \"{s}\" }}"));
        }
    }

    #[test]
    fn fuzz_random_strings_never_panic() {
        let mut rng = Rng(0x5EED_0002);
        for _ in 0..2000 {
            let len = 1 + rng.below(128);
            drive(&random_string(&mut rng, len));
        }
    }

    #[test]
    fn fuzz_json_parser_never_panics() {
        let mut rng = Rng(0x5EED_0003);
        let json_fragments = [
            "{",
            "}",
            "[",
            "]",
            ":",
            ",",
            "\"",
            "\\u",
            "\\uD83D",
            "\\uDE00",
            "null",
            "true",
            "false",
            "-",
            ".",
            "e+",
            "e-",
            "123456789012345678901234567890",
            "\"\\\"",
            "\u{1f600}",
        ];
        for _ in 0..4000 {
            let mut s = String::new();
            for _ in 0..rng.below(24) {
                s.push_str(json_fragments[rng.below(json_fragments.len())]);
            }
            let _ = json::parse(&s);
            if let Ok(v) = json::parse(&s) {
                // dumps(dumps^-1) round-trip must not panic either
                let _ = json::dumps(&v);
            }
        }
    }

    #[test]
    fn test_run_fuzzer_default_seeds() {
        let report =
            run_fuzzer(&[], 100, 0x1234).expect("fuzzer with default seeds should succeed");
        assert_eq!(report.iterations, 100);
        assert_eq!(report.total_iterations, 100);
        assert_eq!(report.mutations_tested, 100);
        assert!(report.panics.is_empty());
        assert_eq!(report.clean_rejections_completions, 100);
        assert_eq!(
            report.clean_rejections_and_completions(),
            report.errors_caught + report.clean_completions
        );
        assert!(report.is_success());
    }

    #[test]
    fn test_run_fuzzer_custom_seeds() {
        let custom = vec!["fn answer() -> int { 42 }".to_string()];
        let report =
            run_fuzzer(&custom, 50, 0xABCD).expect("fuzzer with custom seed should succeed");
        assert_eq!(report.iterations, 50);
        assert_eq!(report.mutations_tested, 50);
        assert!(report.panics.is_empty());
        assert_eq!(report.clean_rejections_completions, 50);
    }

    #[test]
    fn test_run_fuzzer_zero_iterations() {
        let err = run_fuzzer(&[], 0, 42).expect_err("zero iterations should fail");
        assert!(err.contains("iterations must be at least 1"));
    }

    #[test]
    fn test_run_fuzzer_reproducibility() {
        let custom = vec!["agent Bot { state { count: int = 0, } }".to_string()];
        let report1 = run_fuzzer(&custom, 30, 0x9999).unwrap();
        let report2 = run_fuzzer(&custom, 30, 0x9999).unwrap();
        assert_eq!(report1, report2);
    }

    #[test]
    fn test_fuzz_panic_report_display() {
        let panic_entry = FuzzPanic {
            seed: 0x5EED,
            reproducing_seed: 0x5EED,
            snippet: "agent { broken".to_string(),
            mutated_snippet: "agent { broken".to_string(),
            message: "unexpected panic in test".to_string(),
        };
        assert!(
            panic_entry.to_string().contains("0x5EED") || panic_entry.to_string().contains("24301")
        );
        assert!(panic_entry.to_string().contains("broken"));

        let report = FuzzReport {
            total_iterations: 10,
            iterations: 10,
            mutations_tested: 10,
            errors_caught: 7,
            clean_errors: 7,
            clean_completions: 2,
            clean_rejections: 7,
            clean_rejections_completions: 9,
            panics: vec![panic_entry],
        };
        assert!(!report.is_success());
        assert_eq!(
            report.to_string(),
            "fuzzed 10 inputs across lex/parse/check/codegen: 1 panics, 9 clean rejections/completions"
        );
    }

    #[test]
    fn test_fuzz_report_to_json_validity() {
        let seeds = vec!["fn main() -> int { 42 }".to_string()];
        let report = run_fuzzer(&seeds, 5, 12345).expect("run_fuzzer");
        let json_str = report.to_json();
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("fuzz report to_json must produce valid JSON");
        assert_eq!(parsed["total_iterations"], 5);
        assert!(parsed["panics"].as_array().is_some());
    }
}
