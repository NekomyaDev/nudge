//! `nudgec fmt <file.ndg> [--check]` — safe formatting (B3): normalizes
//! indentation from the token stream's brace/bracket/paren depth, trims
//! trailing whitespace and collapses blank-line runs. It never reorders,
//! rewrites or re-flows code: every token stays on its own line, prompt
//! (`llm"""`) bodies are kept verbatim, and comment lines are only
//! re-indented. That keeps the transformation trivially reviewable and
//! idempotent.

use crate::lexer::{lex, Tok};

const INDENT: &str = "    ";

pub fn fmt(src: &str) -> Result<String, String> {
    let tokens = lex(src).map_err(|e| format!("cannot format: {} at byte {}", e.msg, e.at))?;

    // spans of llm""" prompts — lines strictly inside keep their text
    let prompts: Vec<(usize, usize)> = tokens
        .iter()
        .filter(|t| matches!(t.tok, Tok::Prompt(_)))
        .map(|t| (t.start, t.end))
        .collect();

    // (start, is_open, is_close) for depth tracking; prompts excluded
    let marks: Vec<(usize, bool, bool)> = tokens
        .iter()
        .filter(|t| {
            !matches!(t.tok, Tok::Prompt(_))
                && matches!(
                    t.tok,
                    Tok::LBrace
                        | Tok::RBrace
                        | Tok::LParen
                        | Tok::RParen
                        | Tok::LBracket
                        | Tok::RBracket
                )
        })
        .map(|t| {
            let open = matches!(t.tok, Tok::LBrace | Tok::LParen | Tok::LBracket);
            (t.start, open, !open)
        })
        .collect();

    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    let in_prompt = |line_start: usize, line_end: usize| {
        prompts
            .iter()
            .any(|(s, e)| line_start > *s && line_end <= *e)
    };

    let mut out = String::with_capacity(src.len() + 16);
    let mut depth = 0usize;
    let mut mark_i = 0usize;
    let mut blank_run = 0usize;
    for (li, start) in line_starts.iter().enumerate() {
        let end = line_starts.get(li + 1).copied().unwrap_or(src.len());
        let line = &src[*start..end];
        let line_trimmed = line.trim_end_matches(['\n', '\r']);

        if in_prompt(*start, end) {
            out.push_str(line_trimmed);
            out.push('\n');
            blank_run = 0;
            continue;
        }

        // depth at this line's start = all opener/closer marks before it
        while mark_i < marks.len() && marks[mark_i].0 < *start {
            if marks[mark_i].1 {
                depth += 1;
            } else {
                depth = depth.saturating_sub(1);
            }
            mark_i += 1;
        }

        if line_trimmed.trim().is_empty() {
            blank_run += 1;
            // skip blank lines at EOF entirely
            let trailing = src[end..].trim().is_empty();
            if blank_run <= 1 && !trailing {
                out.push('\n');
            }
            continue;
        }

        let content = line_trimmed.trim();
        let is_comment = content.starts_with("//");
        // closers that belong to this line hang at the opener's depth
        let mut lead_closers = 0usize;
        if !is_comment {
            let mut probe = mark_i;
            let mut first_on_line = true;
            while probe < marks.len() && marks[probe].0 < end {
                if first_on_line && marks[probe].2 {
                    lead_closers += 1;
                } else {
                    first_on_line = false;
                }
                probe += 1;
            }
        }
        let indent = depth.saturating_sub(lead_closers);
        for _ in 0..indent {
            out.push_str(INDENT);
        }
        out.push_str(content);
        out.push('\n');
        blank_run = 0;
    }
    Ok(out)
}

/// True when `fmt` would change the file — `fmt --check` reports this.
pub fn needs_formatting(src: &str) -> Result<bool, String> {
    Ok(fmt(src)? != src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn token_kinds(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn fmt_is_idempotent_and_preserves_the_token_stream() {
        let src = include_str!("../../../examples/classifier/classifier.ndg");
        let once = fmt(src).unwrap();
        let twice = fmt(&once).unwrap();
        assert_eq!(once, twice, "fmt must be idempotent");
        assert_eq!(token_kinds(src), token_kinds(&once), "tokens must survive");
        // the formatted program still parses
        parse(lex(&once).unwrap()).unwrap();
    }

    #[test]
    fn fmt_normalizes_indentation_and_keeps_prompts_verbatim() {
        let src = "fn f(x: string) -> string uses LLM {\nlet y = llm\"\"\"multi\n   line   prompt\n\"\"\"\n    with { model: x }\n}\n";
        let out = fmt(src).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "fn f(x: string) -> string uses LLM {");
        assert_eq!(lines[1], "    let y = llm\"\"\"multi");
        // prompt body line kept verbatim, including its odd spacing
        assert_eq!(lines[2], "   line   prompt");
        assert_eq!(lines[4], "    with { model: x }");
        assert_eq!(lines[5], "}");
    }

    #[test]
    fn fmt_trims_trailing_whitespace_and_collapses_blank_runs() {
        let src = "fn f() -> int {\n\n\n    1   \n\n\n}\n\n\n";
        let out = fmt(src).unwrap();
        assert_eq!(out, "fn f() -> int {\n\n    1\n\n}\n");
        assert!(!needs_formatting(&out).unwrap());
        assert!(needs_formatting(src).unwrap());
    }

    #[test]
    fn fmt_reindents_comment_lines_but_keeps_their_text() {
        let src = "fn f() -> int {\n// a comment\n1\n}\n";
        let out = fmt(src).unwrap();
        assert!(out.contains("    // a comment"), "{out}");
    }
}
