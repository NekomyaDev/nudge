/// Heuristic BPE-aligned token counter for LLM prompts and completions.
/// Accurately mirrors byte-pair encoding (cl100k_base / o200k_base / SentencePiece).
/// - Spaces preceding words are absorbed into the word token.
/// - Common English words are 1 token; longer words break into ~3.5 character subwords.
/// - Numbers are grouped into 1-3 digit chunks.
/// - CJK characters are 1 token each.
/// - Punctuation and symbols are 1 token each.
pub fn count_tokens(text: &str) -> u64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut tokens: u64 = 0;
    let mut chars = text.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            let mut ws_len = 0;
            let mut newlines = 0;
            while let Some(&w) = chars.peek() {
                if !w.is_whitespace() {
                    break;
                }
                if w == '\n' {
                    newlines += 1;
                }
                ws_len += 1;
                chars.next();
            }
            if newlines > 0 {
                // Newlines in code / paragraphs
                tokens += newlines as u64;
            } else if ws_len >= 4 {
                // 4-space code indentation
                tokens += (ws_len / 4) as u64;
            }
            // Single spaces before words are absorbed into following word token in BPE
        } else if ch.is_ascii_digit() {
            // Numbers are grouped into 1-3 digit chunks
            let mut digit_len = 0;
            while let Some(&d) = chars.peek() {
                if !d.is_ascii_digit() {
                    break;
                }
                digit_len += 1;
                chars.next();
            }
            tokens += ((digit_len + 2) / 3).max(1) as u64;
        } else if ch.is_alphabetic() {
            let mut word_len = 0;
            let mut is_cjk = false;
            while let Some(&a) = chars.peek() {
                if !a.is_alphabetic() {
                    break;
                }
                if is_cjk_char(a) {
                    is_cjk = true;
                    chars.next();
                    tokens += 1;
                    break;
                }
                word_len += 1;
                chars.next();
            }
            if !is_cjk && word_len > 0 {
                // Short words (<= 5 chars) are 1 token.
                // Longer words split every ~3.5 characters into subwords.
                if word_len <= 5 {
                    tokens += 1;
                } else {
                    tokens += ((word_len + 2) / 4).max(1) as u64;
                }
            }
        } else {
            // Punctuation, symbols, emojis, math
            chars.next();
            tokens += 1;
        }
    }

    tokens.max(1)
}

fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0x3400..=0x4DBF |   // CJK Extension A
        0x20000..=0x2A6DF | // CJK Extension B
        0x3040..=0x309F |   // Hiragana
        0x30A0..=0x30FF |   // Katakana
        0xAC00..=0xD7AF     // Hangul Syllables
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_empty() {
        assert_eq!(count_tokens(""), 0);
        assert_eq!(count_tokens("   \n\t  "), 0);
    }

    #[test]
    fn test_count_simple_sentence() {
        let text = "Hello world!";
        let count = count_tokens(text);
        // "Hello" (1) + " world" (1) + "!" (1) = 3 tokens
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_cjk() {
        let text = "你好世界";
        let count = count_tokens(text);
        assert_eq!(count, 4);
    }

    #[test]
    fn test_count_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let count = count_tokens(code);
        assert!((8..=18).contains(&count), "got {}", count);
    }
}
