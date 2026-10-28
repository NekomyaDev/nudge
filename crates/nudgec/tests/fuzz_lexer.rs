//! Fuzz test for Nudge lexer / tokenizer.
//! Feeds random byte sequences and generated tokens to ensure `lexer::lex` never panics.

use nudgec::lexer::lex;

struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest.iter_mut() {
            *b = (self.next_u32() & 0xFF) as u8;
        }
    }
}

#[test]
fn test_fuzz_lexer_random_bytes_no_panic() {
    let mut rng = SimpleRng::new(0xABCD_1234);

    for _ in 0..10_000 {
        let len = (rng.next_u32() % 256) as usize;
        let mut buf = vec![0u8; len];
        rng.fill_bytes(&mut buf);

        if let Ok(valid_str) = std::str::from_utf8(&buf) {
            let _ = lex(valid_str);
        }
    }
}

#[test]
fn test_fuzz_lexer_nested_parentheses() {
    let open = "(".repeat(500);
    let close = ")".repeat(500);
    let src = format!("fn main() -> int {{ {open} 42 {close} }}");
    let res = lex(&src);
    assert!(
        res.is_ok(),
        "lexer must handle deeply nested parentheses without panic"
    );
}
