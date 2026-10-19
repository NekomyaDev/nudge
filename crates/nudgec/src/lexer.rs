//! Nudge lexer — day 1–3 MVP deliverable (design doc §12 grammar summary).
//! Zero-dependency, hand-rolled, UTF-8 safe; emits tokens with byte spans.
//!
//! Lexical rules (design §12): source is UTF-8; identifiers are ASCII-only;
//! string literals may contain arbitrary UTF-8; `//` comments run to EOL.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    Mut,
    Type,
    Tool,
    Agent,
    State,
    Uses,
    Costs,
    With,
    Par,
    Map,
    All,
    Race,
    For,
    ForAll,
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
    Spawn,

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
    Arrow,    // ->
    Pipe,     // |>
    Bar,      // |
    Question, // ?
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spanned {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
    pub newline_before: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexError {
    pub msg: String,
    pub at: usize,
}

const KEYWORDS: &[(&str, Tok)] = &[
    ("fn", Tok::Fn),
    ("let", Tok::Let),
    ("mut", Tok::Mut),
    ("type", Tok::Type),
    ("tool", Tok::Tool),
    ("agent", Tok::Agent),
    ("state", Tok::State),
    ("uses", Tok::Uses),
    ("costs", Tok::Costs),
    ("with", Tok::With),
    ("par", Tok::Par),
    ("map", Tok::Map),
    ("all", Tok::All),
    ("race", Tok::Race),
    ("for", Tok::For),
    ("for_all", Tok::ForAll),
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
    ("spawn", Tok::Spawn),
];

pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    let mut newline_before = false;

    while i < b.len() {
        let c = b[i] as char;
        match c {
            '\n' => {
                newline_before = true;
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
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
                    match src[i..].find("\"\"\"") {
                        Some(rel) => {
                            let body = &src[i..i + rel];
                            i += rel + 3;
                            out.push(Spanned {
                                tok: Tok::Prompt(body.to_string()),
                                start,
                                end: i,
                                newline_before: std::mem::take(&mut newline_before),
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
                out.push(Spanned {
                    tok,
                    start,
                    end: i,
                    newline_before: std::mem::take(&mut newline_before),
                });
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
                let unit_word = &rest[ws..ws + unit_len];
                let known_unit = matches!(
                    unit_word,
                    "USD" | "EUR" | "GBP" | "JPY" | "CHF" | "TRY" | "CAD" | "AUD" | "CNY" | "INR"
                );
                if unit_len >= 2 && known_unit {
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
                        newline_before: std::mem::take(&mut newline_before),
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
                        newline_before: std::mem::take(&mut newline_before),
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
                        newline_before: std::mem::take(&mut newline_before),
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
                            if (ch as char).is_control() {
                                return Err(LexError {
                                    msg: format!(
                                        "unescaped control character '\\x{:02x}' in string",
                                        ch
                                    ),
                                    at: i,
                                });
                            }
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
                    newline_before: std::mem::take(&mut newline_before),
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
                        '?' => (Tok::Question, 1),
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
                out.push(Spanned {
                    tok,
                    start,
                    end: i,
                    newline_before: std::mem::take(&mut newline_before),
                });
            }
        }
    }
    out.push(Spanned {
        tok: Tok::Eof,
        start: i,
        end: i,
        newline_before,
    });
    Ok(out)
}

impl Tok {
    /// Return the canonical identifier name for this token kind.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Tok::Ident(_) => "Ident",
            Tok::Int(_) => "Int",
            Tok::Float(_) => "Float",
            Tok::Str(_) => "Str",
            Tok::Prompt(_) => "Prompt",
            Tok::Money(_, _) => "Money",
            Tok::Fn => "Fn",
            Tok::Let => "Let",
            Tok::Mut => "Mut",
            Tok::Type => "Type",
            Tok::Tool => "Tool",
            Tok::Agent => "Agent",
            Tok::State => "State",
            Tok::Uses => "Uses",
            Tok::Costs => "Costs",
            Tok::With => "With",
            Tok::Par => "Par",
            Tok::Map => "Map",
            Tok::All => "All",
            Tok::Race => "Race",
            Tok::For => "For",
            Tok::ForAll => "ForAll",
            Tok::In => "In",
            Tok::If => "If",
            Tok::Else => "Else",
            Tok::Return => "Return",
            Tok::Test => "Test",
            Tok::Assert => "Assert",
            Tok::Export => "Export",
            Tok::Use => "Use",
            Tok::And => "And",
            Tok::Or => "Or",
            Tok::True => "True",
            Tok::False => "False",
            Tok::None => "None",
            Tok::Spawn => "Spawn",
            Tok::LParen => "LParen",
            Tok::RParen => "RParen",
            Tok::LBrace => "LBrace",
            Tok::RBrace => "RBrace",
            Tok::LBracket => "LBracket",
            Tok::RBracket => "RBracket",
            Tok::Comma => "Comma",
            Tok::Colon => "Colon",
            Tok::Dot => "Dot",
            Tok::Semi => "Semi",
            Tok::At => "At",
            Tok::Arrow => "Arrow",
            Tok::Pipe => "Pipe",
            Tok::Bar => "Bar",
            Tok::Question => "Question",
            Tok::Assign => "Assign",
            Tok::PlusEq => "PlusEq",
            Tok::MinusEq => "MinusEq",
            Tok::Plus => "Plus",
            Tok::Minus => "Minus",
            Tok::Star => "Star",
            Tok::Slash => "Slash",
            Tok::Percent => "Percent",
            Tok::Bang => "Bang",
            Tok::EqEq => "EqEq",
            Tok::NotEq => "NotEq",
            Tok::Lt => "Lt",
            Tok::LtEq => "LtEq",
            Tok::Gt => "Gt",
            Tok::GtEq => "GtEq",
            Tok::Eof => "Eof",
        }
    }
}

/// JSON representation of an individual token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexTokenJson {
    pub start: usize,
    pub end: usize,
    pub kind: String,
    pub text: String,
}

/// JSON error detail for lex errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexErrorJson {
    pub code: String,
    pub message: String,
    pub at: usize,
    pub line: usize,
    pub col: usize,
}

/// JSON response envelope for successful lexing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexOutputSuccessJson {
    pub ok: bool,
    pub tokens_count: usize,
    pub tokens: Vec<LexTokenJson>,
}

/// JSON response envelope for failed lexing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexOutputErrorJson {
    pub ok: bool,
    pub error: LexErrorJson,
}

/// Serialize token stream to JSON string.
pub fn tokens_to_json(tokens: &[Spanned], src: &str) -> String {
    let tokens_json: Vec<LexTokenJson> = tokens
        .iter()
        .map(|t| LexTokenJson {
            start: t.start,
            end: t.end,
            kind: t.tok.kind_str().to_string(),
            text: src.get(t.start..t.end).unwrap_or("").to_string(),
        })
        .collect();
    let payload = LexOutputSuccessJson {
        ok: true,
        tokens_count: tokens_json.len(),
        tokens: tokens_json,
    };
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

/// Lex source code and return a formatted JSON string.
/// Returns Ok(json) on success, or Err(json) on lexical error.
pub fn lex_json(src: &str) -> Result<String, String> {
    match lex(src) {
        Ok(tokens) => Ok(tokens_to_json(&tokens, src)),
        Err(e) => {
            let (line, col) = crate::check::line_col(src, e.at);
            let payload = LexOutputErrorJson {
                ok: false,
                error: LexErrorJson {
                    code: "E0001".to_string(),
                    message: e.msg,
                    at: e.at,
                    line,
                    col,
                },
            };
            Err(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
    }
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

    #[test]
    fn string_escape_validation() {
        let t = toks("\"\\n\\t\\\"\\\\\"");
        assert_eq!(t[0], Tok::Str("\n\t\"\\".into()));
        assert!(lex("\"\\x\"").is_err());
    }

    #[test]
    fn unescaped_control_characters_rejected() {
        assert!(lex("\"hello\nworld\"").is_err());
        assert!(lex("\"hello\rworld\"").is_err());
        assert!(lex("\"hello\x00world\"").is_err());
        assert!(lex("\"hello\x1fworld\"").is_err());
    }

    #[test]
    fn test_tok_serde_round_trip() {
        let tok = Tok::Money(42.5, "USD".into());
        let json = serde_json::to_string(&tok).expect("serialize tok");
        let decoded: Tok = serde_json::from_str(&json).expect("deserialize tok");
        assert_eq!(tok, decoded);

        let spanned = Spanned {
            tok: Tok::Fn,
            start: 0,
            end: 2,
            newline_before: true,
        };
        let s_json = serde_json::to_string(&spanned).expect("serialize spanned");
        let s_decoded: Spanned = serde_json::from_str(&s_json).expect("deserialize spanned");
        assert_eq!(spanned, s_decoded);
    }

    #[test]
    fn test_lex_json_success() {
        let src = "fn add(a: int) -> int { a + 1 }";
        let res = lex_json(src).expect("lex_json ok");
        let val: serde_json::Value = serde_json::from_str(&res).expect("valid json");
        assert_eq!(val["ok"], true);
        assert!(val["tokens_count"].as_u64().unwrap() > 0);
        let tokens = val["tokens"].as_array().expect("tokens array");
        assert_eq!(tokens[0]["kind"], "Fn");
        assert_eq!(tokens[0]["text"], "fn");
        assert_eq!(tokens[0]["start"], 0);
        assert_eq!(tokens[0]["end"], 2);
    }

    #[test]
    fn test_lex_json_error() {
        let src = "let x = \"unterminated";
        let err_res = lex_json(src).expect_err("lex_json should fail");
        let val: serde_json::Value = serde_json::from_str(&err_res).expect("valid json");
        assert_eq!(val["ok"], false);
        assert_eq!(val["error"]["code"], "E0001");
        assert_eq!(val["error"]["line"], 1);
        assert_eq!(val["error"]["col"], 9);
        assert_eq!(val["error"]["at"], 8);
        assert!(val["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unterminated"));
    }

    #[test]
    fn test_lex_multibyte_utf8_identifiers_and_escapes() {
        // String with valid multibyte characters
        let src = "\"Übersetzung 日本語 🚀\"";
        let tokens = lex(src).expect("lex multibyte string");
        assert_eq!(tokens.len(), 2); // Tok::Str + Tok::Eof
        assert_eq!(tokens[0].tok, Tok::Str("Übersetzung 日本語 🚀".into()));
        assert_eq!(tokens[1].tok, Tok::Eof);

        // Valid escape sequences: \n, \t, \", \\
        let esc = "\"line1\\nline2\\tindent\\\\backslash\\\"quote\"";
        let esc_tokens = lex(esc).expect("lex escape sequences");
        assert_eq!(
            esc_tokens[0].tok,
            Tok::Str("line1\nline2\tindent\\backslash\"quote".into())
        );

        // Invalid escape sequence fails cleanly
        let bad_esc = "\"bad \\x escape\"";
        let err = lex(bad_esc).expect_err("bad escape should fail");
        assert!(err.msg.contains("bad escape"));

        // Unterminated prompt fails cleanly with byte offset
        let bad_prompt = "llm\"\"\"missing closing delimiter";
        let prompt_err = lex(bad_prompt).expect_err("unterminated prompt should fail");
        assert!(prompt_err.msg.contains("unterminated llm\"\"\" prompt"));
    }
}
