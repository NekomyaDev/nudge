//! Compiler pipeline entry point: lexing + parsing.

use crate::ast;
use crate::lexer;
use crate::parser;

/// Lex and parse Nudge source code into AST items.
pub fn compile(src: &str) -> Result<Vec<ast::Item>, (String, usize)> {
    lexer::lex(src)
        .map_err(|e| (e.msg, e.at))
        .and_then(|t| parser::parse(t).map_err(|e| (e.msg, e.at)))
}
