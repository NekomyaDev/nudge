//! Fuzz smoke tests (v1.6): deterministic, dependency-free mutation fuzzing
//! of the lex → parse → check → emit pipeline and the JSON parser.
//!
//! The harness drives thousands of mutated/derived inputs through the full
//! frontend and asserts exactly one thing: **no panics**. Clean errors are
//! the correct outcome for garbage input; a panic is a bug. Seeds are fixed
//! so failures reproduce byte-for-byte.

#[cfg(test)]
mod tests {
    use crate::{check, codegen, codegen_ts, json, lexer, parser};

    /// xorshift64* — tiny deterministic PRNG so fuzz runs are reproducible.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    const SEEDS: &[&str] = &[
        include_str!("../../../examples/research_agent.ndg"),
        include_str!("../../../examples/checkpoint_agent.ndg"),
        include_str!("../../../examples/hello_llm.ndg"),
        "fn f() -> int { 1 }",
        "agent A {\n    state {\n        n: int = 0,\n    }\n    fn step() -> int uses LLM {\n        llm\"\"\"go {state.n}\"\"\" with { model: \"m\", budget: 0.01 USD }\n    }\n}",
    ];

    /// Interesting single-token fragments the mutator splices in.
    const FRAGMENTS: &[&str] = &[
        "\"\"\"", "llm\"\"\"", "{", "}", "(", ")", "[", "]", "|", "->", "==", "!=", "<=", ">=",
        "+=", "-=", "=", ":", ",", ".", "@range(0, 1)", "state.", "0.02 USD", "\"", "\\", "é", "😀",
        "par map", "merge", "zip", "route{", "when", "otherwise", "\n\n\n", "\t", "fn", "agent",
        "stream let", "with {", "budget:", "schema:", "999999999999999999999", "-", "0x", "1.2.3",
    ];

    fn mutate(rng: &mut Rng, base: &str) -> String {
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

    fn random_string(rng: &mut Rng, len: usize) -> String {
        // printable ASCII + newlines + a few unicode curves
        (0..len)
            .map(|_| match rng.below(40) {
                0 => '\n',
                1 => '😀',
                2 => 'é',
                3 => '\\',
                n => (b' ' + ((n * 7) % 95) as u8) as char,
            })
            .collect()
    }

    /// One fuzz case must never panic — any Result is acceptable.
    fn drive(src: &str) {
        let Ok(tokens) = lexer::lex(src) else { return };
        let Ok(items) = parser::parse(tokens) else { return };
        let _ = check::check(&items);
        // codegen on unchecked garbage must stay panic-free too (the CLI
        // always checks first, but emit must not rely on that)
        let _ = codegen::emit(&items);
        let _ = codegen_ts::emit_ts(&items);
    }

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
            "{", "}", "[", "]", ":", ",", "\"", "\\u", "\\uD83D", "\\uDE00", "null", "true",
            "false", "-", ".", "e+", "e-", "123456789012345678901234567890", "\"\\\"", "😀",
        ];
        for _ in 0..4000 {
            let mut s = String::new();
            for _ in 0..rng.below(24) {
                s.push_str(json_fragments[rng.below(json_fragments.len())]);
            }
            let _ = json::parse(&s);
            if let Ok(v) = json::parse(&s) {
                // dumps(dumps⁻¹) round-trip must not panic either
                let _ = json::dumps(&v);
            }
        }
    }
}
