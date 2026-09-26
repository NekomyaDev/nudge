//! Simple parser for Nudge source code
//!
//! Parses basic Nudge syntax into AST for bytecode compilation.

use crate::codegen::{BinaryOp, Expr, FunctionDef, ProgramDef, Stmt, UnaryOp};

/// Parse error
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
}

/// Simple parser
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    line: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // lexer vocabulary: not all tokens reach the parser yet
enum Token {
    // Literals
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),

    // Identifiers and keywords
    Ident(String),
    Fn,
    Let,
    If,
    Else,
    While,
    Return,
    Print,
    True,
    False,
    None,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Not,
    Assign,

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    Arrow,

    // Special
    Newline,
    Eof,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let tokens = Self::tokenize(source);
        Parser {
            tokens,
            pos: 0,
            line: 1,
        }
    }

    /// Simple tokenizer
    fn tokenize(source: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = source.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                ' ' | '\t' | '\r' => {
                    i += 1;
                }
                '\n' => {
                    tokens.push(Token::Newline);
                    i += 1;
                }
                '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                    // Comment - skip to end of line
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                }
                '0'..='9' => {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i < chars.len() && chars[i] == '.' {
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                        let s: String = chars[start..i].iter().collect();
                        tokens.push(Token::Float(s.parse().unwrap()));
                    } else {
                        let s: String = chars[start..i].iter().collect();
                        tokens.push(Token::Int(s.parse().unwrap()));
                    }
                }
                '"' => {
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != '"' {
                        if chars[i] == '\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    let s: String = chars[start..i].iter().collect();
                    i += 1; // skip closing quote
                    tokens.push(Token::String(s));
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    match word.as_str() {
                        "fn" => tokens.push(Token::Fn),
                        "let" => tokens.push(Token::Let),
                        "if" => tokens.push(Token::If),
                        "else" => tokens.push(Token::Else),
                        "while" => tokens.push(Token::While),
                        "return" => tokens.push(Token::Return),
                        "print" => tokens.push(Token::Print),
                        "true" => tokens.push(Token::True),
                        "false" => tokens.push(Token::False),
                        "none" => tokens.push(Token::None),
                        "and" => tokens.push(Token::And),
                        "or" => tokens.push(Token::Or),
                        "not" => tokens.push(Token::Not),
                        _ => tokens.push(Token::Ident(word)),
                    }
                }
                '+' => {
                    tokens.push(Token::Plus);
                    i += 1;
                }
                '-' => {
                    if i + 1 < chars.len() && chars[i + 1] == '>' {
                        tokens.push(Token::Arrow);
                        i += 2;
                    } else {
                        tokens.push(Token::Minus);
                        i += 1;
                    }
                }
                '*' => {
                    tokens.push(Token::Star);
                    i += 1;
                }
                '/' => {
                    tokens.push(Token::Slash);
                    i += 1;
                }
                '%' => {
                    tokens.push(Token::Percent);
                    i += 1;
                }
                '=' => {
                    if i + 1 < chars.len() && chars[i + 1] == '=' {
                        tokens.push(Token::EqEq);
                        i += 2;
                    } else {
                        tokens.push(Token::Assign);
                        i += 1;
                    }
                }
                '!' => {
                    if i + 1 < chars.len() && chars[i + 1] == '=' {
                        tokens.push(Token::NotEq);
                        i += 2;
                    } else {
                        tokens.push(Token::Not);
                        i += 1;
                    }
                }
                '<' => {
                    if i + 1 < chars.len() && chars[i + 1] == '=' {
                        tokens.push(Token::LtEq);
                        i += 2;
                    } else {
                        tokens.push(Token::Lt);
                        i += 1;
                    }
                }
                '>' => {
                    if i + 1 < chars.len() && chars[i + 1] == '=' {
                        tokens.push(Token::GtEq);
                        i += 2;
                    } else {
                        tokens.push(Token::Gt);
                        i += 1;
                    }
                }
                '(' => {
                    tokens.push(Token::LParen);
                    i += 1;
                }
                ')' => {
                    tokens.push(Token::RParen);
                    i += 1;
                }
                '{' => {
                    tokens.push(Token::LBrace);
                    i += 1;
                }
                '}' => {
                    tokens.push(Token::RBrace);
                    i += 1;
                }
                '[' => {
                    tokens.push(Token::LBracket);
                    i += 1;
                }
                ']' => {
                    tokens.push(Token::RBracket);
                    i += 1;
                }
                ',' => {
                    tokens.push(Token::Comma);
                    i += 1;
                }
                ':' => {
                    tokens.push(Token::Colon);
                    i += 1;
                }
                ';' => {
                    tokens.push(Token::Semicolon);
                    i += 1;
                }
                '.' => {
                    tokens.push(Token::Dot);
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        tokens.push(Token::Eof);
        tokens
    }

    /// Peek at current token
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    /// Peek at next-next token
    fn peek2(&self) -> &Token {
        self.tokens.get(self.pos + 1).unwrap_or(&Token::Eof)
    }

    /// Advance to next token
    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    /// Expect a specific token
    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, found {:?}", expected, self.peek()),
                line: self.line,
            })
        }
    }

    /// Skip newlines
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    /// Parse a complete program
    pub fn parse_program(&mut self) -> Result<ProgramDef, ParseError> {
        let mut functions = Vec::new();
        let mut body = Vec::new();

        self.skip_newlines();

        while !matches!(self.peek(), Token::Eof) {
            if matches!(self.peek(), Token::Fn) {
                functions.push(self.parse_function()?);
            } else {
                body.push(self.parse_stmt()?);
            }
            self.skip_newlines();
        }

        Ok(ProgramDef { functions, body })
    }

    /// Parse a function definition
    fn parse_function(&mut self) -> Result<FunctionDef, ParseError> {
        self.expect(&Token::Fn)?;
        let name = match self.peek().clone() {
            Token::Ident(n) => {
                self.advance();
                n
            }
            _ => {
                return Err(ParseError {
                    message: "Expected function name".to_string(),
                    line: self.line,
                })
            }
        };

        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::RParen) {
            if let Token::Ident(p) = self.peek().clone() {
                params.push(p);
                self.advance();
            }
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RParen)?;

        // Skip return type annotation
        if matches!(self.peek(), Token::Arrow) {
            self.advance();
            // Skip type name
            if matches!(self.peek(), Token::Ident(_)) {
                self.advance();
            }
        }

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Token::RBrace) {
            body.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;

        Ok(FunctionDef {
            name,
            params,
            body,
            effects: Vec::new(),
        })
    }

    /// Parse a statement
    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.skip_newlines();

        match self.peek().clone() {
            Token::Let => self.parse_let(),
            Token::If => self.parse_if(),
            Token::While => self.parse_while(),
            Token::Return => self.parse_return(),
            Token::Print => self.parse_print(),
            Token::Ident(_) if matches!(self.peek2(), Token::Assign) => {
                // Reassignment: name = expr
                let name = match self.peek().clone() {
                    Token::Ident(n) => {
                        self.advance();
                        n
                    }
                    _ => unreachable!(),
                };
                self.advance(); // consume Assign
                let value = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::Assign { name, value, line })
            }
            _ => {
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::ExprStmt { expr, line })
            }
        }
    }

    /// Parse let statement
    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.expect(&Token::Let)?;

        let name = match self.peek().clone() {
            Token::Ident(n) => {
                self.advance();
                n
            }
            _ => {
                return Err(ParseError {
                    message: "Expected variable name".to_string(),
                    line,
                })
            }
        };

        // Skip type annotation
        if matches!(self.peek(), Token::Colon) {
            self.advance();
            if matches!(self.peek(), Token::Ident(_)) {
                self.advance();
            }
        }

        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        self.skip_newlines();

        Ok(Stmt::Let { name, value, line })
    }

    /// Parse if statement
    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.expect(&Token::If)?;
        let condition = self.parse_expr()?;
        self.expect(&Token::LBrace)?;

        let mut body = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Token::RBrace) {
            body.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;

        let mut else_body = None;
        if matches!(self.peek(), Token::Else) {
            self.advance();
            self.expect(&Token::LBrace)?;
            let mut else_stmts = Vec::new();
            self.skip_newlines();
            while !matches!(self.peek(), Token::RBrace) {
                else_stmts.push(self.parse_stmt()?);
                self.skip_newlines();
            }
            self.expect(&Token::RBrace)?;
            else_body = Some(else_stmts);
        }

        Ok(Stmt::If {
            condition,
            body,
            else_body,
            line,
        })
    }

    /// Parse while statement
    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.expect(&Token::While)?;
        let condition = self.parse_expr()?;
        self.expect(&Token::LBrace)?;

        let mut body = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Token::RBrace) {
            body.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;

        Ok(Stmt::While {
            condition,
            body,
            line,
        })
    }

    /// Parse return statement
    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.expect(&Token::Return)?;

        let mut value = None;
        if !matches!(self.peek(), Token::Newline | Token::RBrace | Token::Eof) {
            value = Some(self.parse_expr()?);
        }
        self.skip_newlines();

        Ok(Stmt::Return { value, line })
    }

    /// Parse print statement
    fn parse_print(&mut self) -> Result<Stmt, ParseError> {
        let line = self.line;
        self.expect(&Token::Print)?;
        self.expect(&Token::LParen)?;

        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen) {
            args.push(self.parse_expr()?);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RParen)?;
        self.skip_newlines();

        Ok(Stmt::Print { args, line })
    }

    /// Parse expression
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    /// Parse or expression
    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse and expression
    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse equality expression
    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinaryOp::Eq,
                Token::NotEq => BinaryOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse comparison expression
    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinaryOp::Lt,
                Token::LtEq => BinaryOp::LtEq,
                Token::Gt => BinaryOp::Gt,
                Token::GtEq => BinaryOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse addition expression
    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse multiplication expression
    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse unary expression
    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    /// Parse postfix expression (function calls, field access)
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                Token::LParen => {
                    // Function call
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Token::RParen) {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(&Token::RParen)?;
                    expr = Expr::Call {
                        func: Box::new(expr),
                        args,
                    };
                }
                Token::Dot => {
                    // Field access
                    self.advance();
                    let field = match self.peek().clone() {
                        Token::Ident(name) => {
                            self.advance();
                            name
                        }
                        _ => {
                            return Err(ParseError {
                                message: "Expected field name after '.'".to_string(),
                                line: self.line,
                            })
                        }
                    };
                    expr = Expr::Field {
                        object: Box::new(expr),
                        field,
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse primary expression
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Int(v) => {
                self.advance();
                Ok(Expr::IntLiteral(v))
            }
            Token::Float(v) => {
                self.advance();
                Ok(Expr::FloatLiteral(v))
            }
            Token::String(v) => {
                self.advance();
                Ok(Expr::StringLiteral(v))
            }
            Token::True => {
                self.advance();
                Ok(Expr::BoolLiteral(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::BoolLiteral(false))
            }
            Token::None => {
                self.advance();
                Ok(Expr::NoneLiteral)
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Identifier(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), Token::RBracket) {
                    items.push(self.parse_expr()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::ListLiteral(items))
            }
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", self.peek()),
                line: self.line,
            }),
        }
    }
}

/// Parse Nudge source code
pub fn parse(source: &str) -> Result<ProgramDef, ParseError> {
    let mut parser = Parser::new(source);
    parser.parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_let() {
        let source = "let x = 42";
        let program = parse(source).unwrap();
        assert_eq!(program.body.len(), 1);
    }

    #[test]
    fn test_parse_function() {
        let source = r#"
fn add(a, b) {
    return a + b
}
"#;
        let program = parse(source).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "add");
    }

    #[test]
    fn test_parse_if() {
        let source = r#"
if x > 0 {
    print(x)
}
"#;
        let program = parse(source).unwrap();
        assert_eq!(program.body.len(), 1);
    }

    #[test]
    fn test_parse_while() {
        let source = r#"
let x = 0
while x < 10 {
    let x = x + 1
}
"#;
        let program = parse(source).unwrap();
        assert_eq!(program.body.len(), 2);
    }

    #[test]
    fn test_parse_list() {
        let source = "let list = [1, 2, 3]";
        let program = parse(source).unwrap();
        assert_eq!(program.body.len(), 1);
    }

    #[test]
    fn test_parse_expressions() {
        let source = "let x = 1 + 2 * 3";
        let program = parse(source).unwrap();
        assert_eq!(program.body.len(), 1);
    }
}
