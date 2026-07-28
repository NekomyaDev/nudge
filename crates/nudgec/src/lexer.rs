//! Nudge lexer — day 1–3 MVP deliverable (design doc §12 grammar summary).
//! Zero-dependency, hand-rolled, UTF-8 safe; emits tokens with byte spans.
//!
//! Lexical rules (design §12): source is UTF-8; identifiers are ASCII-only;
//! string literals may contain arbitrary UTF-8; `//` comments run to EOL.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // literals
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),        // "..." — no interpolation
    Prompt(String),     // llm""" ... """ — raw body, interpolation parsed later
    Money(f64, String), // 0.02 USD — unit kept for E0501 (USD only in v0.1)

    // reserved keywords (design §12: contextual keywords like `schema`, `retry`,
    // `impl`, `replay` intentionally lex as Ident — the parser recognizes them
    // by string in their grammatical positions, so they stay usable as names)
    Fn,
    Let,
    Type,
    Tool,
    Agent,
    State,
    Uses,
    With,
    Par,
    Map,
    All,
    Race,
    For,
    In,
    If,
    Else,
    Return,
    Test,
    Assert,
    Export,
    Use,
    And,
    Or,
    True,
    False,
    None,

    // punctuation & operators
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Semi,
    At,
    Arrow, // ->
    Pipe,  // |>
    Bar,   // |
    Assign,
    PlusEq,
    MinusEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub msg: String,
    pub at: usize,
}

const KEYWORDS: &[(&str, Tok)] = &[
    ("fn", Tok::Fn),
    ("let", Tok::Let),
    ("type", Tok::Type),
    ("tool", Tok::Tool),
    ("agent", Tok::Agent),
    ("state", Tok::State),
    ("uses", Tok::Uses),
    ("with", Tok::With),
    ("par", Tok::Par),
    ("map", Tok::Map),
    ("all", Tok::All),
    ("race", Tok::Race),
    ("for", Tok::For),
    ("in", Tok::In),
    ("if", Tok::If),
    ("else", Tok::Else),
    ("return", Tok::Return),
    ("test", Tok::Test),
    ("assert", Tok::Assert),
    ("export", Tok::Export),
    ("use", Tok::Use),
    ("and", Tok::And),
    ("or", Tok::Or),
    ("true", Tok::True),
    ("false", Tok::False),
    ("none", Tok::None),
];

pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < b.len() {
        let c = b[i] as char;
        match c {
            c if c.is_whitespace() => i += 1,
            '/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                // ASCII-only identifiers: non-ASCII bytes stop the word and
                // surface later as a proper "unexpected character" error.
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &src[start..i];

                // llm""" prompt literal
                if word == "llm" && src[i..].starts_with("\"\"\"") {
                    i += 3;
                    let body_start = i;
                    match src[i..].find("\"\"\"") {
                        Some(rel) => {
                            let body = &src[i..i + rel];
                            i += rel + 3;
                            out.push(Spanned {
                                tok: Tok::Prompt(body.to_string()),
                                start: body_start,
                                end: i,
                            });
                            continue;
                        }
                        None => {
                            return Err(LexError {
                                msg: "unterminated llm\"\"\" prompt".into(),
                                at: start,
                            })
                        }
                    }
                }

                let tok = KEYWORDS
                    .iter()
                    .find(|(k, _)| *k == word)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_else(|| Tok::Ident(word.to_string()));
                out.push(Spanned { tok, start, end: i });
            }
            '0'..='9' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let mut is_float = false;
                if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    is_float = true;
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text = &src[start..i];
                // money literal: <number> <UPPERCASE-UNIT> — spaces/tabs
                // between amount and unit are tolerated; the checker
                // validates the unit itself (E0501; USD only in v0.1)
                let rest = &src[i..];
                let ws = rest
                    .bytes()
                    .take_while(|c| matches!(c, b' ' | b'\t'))
                    .count();
                let unit_len = rest[ws..]
                    .bytes()
                    .take_while(|c| c.is_ascii_uppercase())
                    .count();
                if unit_len >= 2 {
                    let unit = &src[i + ws..i + ws + unit_len];
                    i += ws + unit_len;
                    let v: f64 = text.parse().map_err(|_| LexError {
                        msg: "bad money literal".into(),
                        at: start,
                    })?;
                    out.push(Spanned {
                        tok: Tok::Money(v, unit.to_string()),
                        start,
                        end: i,
                    });
                } else if is_float {
                    let v: f64 = text.parse().map_err(|_| LexError {
                        msg: "bad float literal".into(),
                        at: start,
                    })?;
                    out.push(Spanned {
                        tok: Tok::Float(v),
                        start,
                        end: i,
                    });
                } else {
                    let v: i64 = text.parse().map_err(|_| LexError {
                        msg: "integer literal out of range".into(),
                        at: start,
                    })?;
                    out.push(Spanned {
                        tok: Tok::Int(v),
                        start,
                        end: i,
                    });
                }
            }
            '"' => {
                let start = i;
                i += 1;
                let mut s = String::new();
                loop {
                    match b.get(i) {
                        None => {
                            return Err(LexError {
                                msg: "unterminated string".into(),
                                at: start,
                            })
                        }
                        Some(&b'"') => {
                            i += 1;
                            break;
                        }
                        Some(&b'\\') => {
                            i += 1;
                            match b.get(i) {
                                Some(&b'n') => s.push('\n'),
                                Some(&b't') => s.push('\t'),
                                Some(&b'"') => s.push('"'),
                                Some(&b'\\') => s.push('\\'),
                                _ => {
                                    return Err(LexError {
                                        msg: "bad escape".into(),
                                        at: i,
                                    })
                                }
                            }
                            i += 1;
                        }
                        // UTF-8: ASCII fast path, multibyte sequences copied as a slice.
                        Some(&ch) if ch < 0x80 => {
                            s.push(ch as char);
                            i += 1;
                        }
                        Some(&ch) => {
                            let len = if ch >= 0xF0 {
                                4
                            } else if ch >= 0xE0 {
                                3
                            } else {
                                2
                            };
                            if i + len > b.len() {
                                return Err(LexError {
                                    msg: "invalid UTF-8 in string".into(),
                                    at: i,
                                });
                            }
                            s.push_str(&src[i..i + len]);
                            i += len;
                        }
                    }
                }
                out.push(Spanned {
                    tok: Tok::Str(s),
                    start,
                    end: i,
                });
            }
            _ => {
                let start = i;
                // Two-byte operators: only peek when both bytes are ASCII,
                // otherwise the slice could split a UTF-8 sequence.
                let two = if i + 1 < b.len() && b[i].is_ascii() && b[i + 1].is_ascii() {
                    &src[i..i + 2]
                } else {
                    ""
                };
                let (tok, len) = match two {
                    "->" => (Tok::Arrow, 2),
                    "|>" => (Tok::Pipe, 2),
                    "+=" => (Tok::PlusEq, 2),
                    "-=" => (Tok::MinusEq, 2),
                    "==" => (Tok::EqEq, 2),
                    "!=" => (Tok::NotEq, 2),
                    "<=" => (Tok::LtEq, 2),
                    ">=" => (Tok::GtEq, 2),
                    _ => match c {
                        '(' => (Tok::LParen, 1),
                        ')' => (Tok::RParen, 1),
                        '{' => (Tok::LBrace, 1),
                        '}' => (Tok::RBrace, 1),
                        '[' => (Tok::LBracket, 1),
                        ']' => (Tok::RBracket, 1),
                        ',' => (Tok::Comma, 1),
                        ':' => (Tok::Colon, 1),
                        '.' => (Tok::Dot, 1),
                        ';' => (Tok::Semi, 1),
                        '@' => (Tok::At, 1),
                        '|' => (Tok::Bar, 1),
                        '=' => (Tok::Assign, 1),
                        '+' => (Tok::Plus, 1),
                        '-' => (Tok::Minus, 1),
                        '*' => (Tok::Star, 1),
                        '/' => (Tok::Slash, 1),
                        '%' => (Tok::Percent, 1),
                        '!' => (Tok::Bang, 1),
                        '<' => (Tok::Lt, 1),
                        '>' => (Tok::Gt, 1),
                        _ => {
                            return Err(LexError {
                                msg: format!("unexpected character '{c}'"),
                                at: start,
                            })
                        }
                    },
                };
                i += len;
                out.push(Spanned { tok, start, end: i });
            }
        }
    }
    out.push(Spanned {
        tok: Tok::Eof,
        start: i,
        end: i,
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|s| s.tok).collect()
    }

    #[test]
    fn fn_signature_with_effects() {
        let t = toks("fn research(q: string) -> Report uses LLM, Tool {}");
        assert_eq!(
            t[..8],
            [
                Tok::Fn,
                Tok::Ident("research".into()),
                Tok::LParen,
                Tok::Ident("q".into()),
                Tok::Colon,
                Tok::Ident("string".into()),
                Tok::RParen,
                Tok::Arrow,
            ]
        );
        assert!(t.contains(&Tok::Uses));
    }

    #[test]
    fn money_literal() {
        let t = toks("budget: 0.02 USD");
        assert_eq!(
            t,
            vec![
                Tok::Ident("budget".into()),
                Tok::Colon,
                Tok::Money(0.02, "USD".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn money_keeps_any_unit_for_e0501() {
        let t = lex("5 EUR")
            .unwrap()
            .into_iter()
            .map(|s| s.tok)
            .collect::<Vec<_>>();
        assert_eq!(t, vec![Tok::Money(5.0, "EUR".into()), Tok::Eof]);
        // a lone lowercase suffix is not money
        let t = lex("5 apples")
            .unwrap()
            .into_iter()
            .map(|s| s.tok)
            .collect::<Vec<_>>();
        assert_eq!(t, vec![Tok::Int(5), Tok::Ident("apples".into()), Tok::Eof]);
    }

    #[test]
    fn prompt_literal_captures_body() {
        let t = toks("llm\"\"\"Break down: {question}\"\"\" with {}");
        assert_eq!(t[0], Tok::Prompt("Break down: {question}".into()));
        assert_eq!(t[1], Tok::With);
    }

    #[test]
    fn refinement_and_repair() {
        let t = toks("float @range(0, 1) retry: 2 with repair");
        assert!(t.contains(&Tok::At));
        assert!(t.contains(&Tok::With));
        // contextual keywords lex as plain identifiers (design §12)
        assert!(t.contains(&Tok::Ident("retry".into())));
        assert!(t.contains(&Tok::Ident("repair".into())));
    }

    #[test]
    fn par_map_fanout() {
        let t = toks("par map steps |s| -> execute(s)");
        assert_eq!(
            t[..4],
            [Tok::Par, Tok::Map, Tok::Ident("steps".into()), Tok::Bar]
        );
    }

    #[test]
    fn and_or_are_keywords() {
        let t = toks("a and b or c");
        assert_eq!(
            t,
            vec![
                Tok::Ident("a".into()),
                Tok::And,
                Tok::Ident("b".into()),
                Tok::Or,
                Tok::Ident("c".into()),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn utf8_string_contents_survive() {
        let t = toks("\"héllo dünya — 你好\"");
        assert_eq!(t[0], Tok::Str("héllo dünya — 你好".into()));
    }

    #[test]
    fn non_ascii_outside_strings_errors_not_panics() {
        // identifier path: non-ASCII stops the word, then errors cleanly
        assert!(lex("let café = 1").is_err());
        // operator peek must not split a UTF-8 sequence
        assert!(lex("-é").is_err());
    }

    #[test]
    fn unterminated_string_errors() {
        assert!(lex("\"oops").is_err());
    }

    #[test]
    fn money_tolerates_whitespace_before_the_unit() {
        let t = toks("budget: 0.02  USD");
        assert!(t.contains(&Tok::Money(0.02, "USD".into())), "{t:?}");
        let t = toks("budget: 0.02\tUSD");
        assert!(t.contains(&Tok::Money(0.02, "USD".into())), "{t:?}");
    }
}
