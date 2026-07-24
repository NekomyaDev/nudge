//! Minimal dependency-free JSON value, parser, and writer — shared by the
//! A2A exporter, the LSP server, and the frozen-trace validator (v1.0).

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn idx(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(xs) => xs.get(i),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn is_obj(&self) -> bool {
        matches!(self, Json::Obj(_))
    }

    pub fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }
}

/// Escape a string's contents for embedding in a JSON string literal.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn dumps(v: &Json) -> String {
    match v {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Json::Str(s) => format!("\"{}\"", escape(s)),
        Json::Arr(xs) => {
            format!("[{}]", xs.iter().map(dumps).collect::<Vec<_>>().join(", "))
        }
        Json::Obj(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("\"{}\": {}", escape(k), dumps(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    struct P<'a> {
        b: &'a [u8],
        i: usize,
    }
    impl<'a> P<'a> {
        fn ws(&mut self) {
            while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
        }
        fn peek(&self) -> Option<u8> {
            self.b.get(self.i).copied()
        }
        fn expect(&mut self, c: u8) -> Result<(), String> {
            self.ws();
            if self.peek() == Some(c) {
                self.i += 1;
                Ok(())
            } else {
                Err(format!("expected '{}' at byte {}", c as char, self.i))
            }
        }
        fn value(&mut self) -> Result<Json, String> {
            self.ws();
            match self.peek() {
                Some(b'{') => self.obj(),
                Some(b'[') => self.arr(),
                Some(b'"') => Ok(Json::Str(self.string()?)),
                Some(b't') => self.lit("true", Json::Bool(true)),
                Some(b'f') => self.lit("false", Json::Bool(false)),
                Some(b'n') => self.lit("null", Json::Null),
                Some(_) => self.num(),
                None => Err("unexpected end of input".into()),
            }
        }
        fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
            if self.b[self.i..].starts_with(word.as_bytes()) {
                self.i += word.len();
                Ok(v)
            } else {
                Err(format!("invalid literal at byte {}", self.i))
            }
        }
        fn num(&mut self) -> Result<Json, String> {
            let start = self.i;
            while matches!(self.peek(), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')) {
                self.i += 1;
            }
            let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
            s.parse::<f64>()
                .map(Json::Num)
                .map_err(|_| format!("invalid number '{s}' at byte {start}"))
        }
        fn string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut out = String::new();
            loop {
                match self.peek() {
                    None => return Err("unterminated string".into()),
                    Some(b'"') => {
                        self.i += 1;
                        return Ok(out);
                    }
                    Some(b'\\') => {
                        self.i += 1;
                        match self.peek() {
                            Some(b'"') => out.push('"'),
                            Some(b'\\') => out.push('\\'),
                            Some(b'/') => out.push('/'),
                            Some(b'n') => out.push('\n'),
                            Some(b'r') => out.push('\r'),
                            Some(b't') => out.push('\t'),
                            Some(b'b') => out.push('\u{0008}'),
                            Some(b'f') => out.push('\u{000C}'),
                            Some(b'u') => {
                                let hex = self
                                    .b
                                    .get(self.i + 1..self.i + 5)
                                    .and_then(|h| std::str::from_utf8(h).ok())
                                    .ok_or("bad \\u escape")?;
                                let cp = u32::from_str_radix(hex, 16).map_err(|_| "bad \\u escape")?;
                                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                                self.i += 4;
                            }
                            _ => return Err(format!("bad escape at byte {}", self.i)),
                        }
                        self.i += 1;
                    }
                    Some(_) => {
                        // consume one UTF-8 code point
                        let rest = &self.b[self.i..];
                        let s = std::str::from_utf8(rest).map_err(|e| e.to_string())?;
                        let ch = s.chars().next().ok_or("unexpected end")?;
                        out.push(ch);
                        self.i += ch.len_utf8();
                    }
                }
            }
        }
        fn arr(&mut self) -> Result<Json, String> {
            self.expect(b'[')?;
            let mut xs = Vec::new();
            self.ws();
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(Json::Arr(xs));
            }
            loop {
                xs.push(self.value()?);
                self.ws();
                match self.peek() {
                    Some(b',') => self.i += 1,
                    Some(b']') => {
                        self.i += 1;
                        return Ok(Json::Arr(xs));
                    }
                    _ => return Err(format!("expected ',' or ']' at byte {}", self.i)),
                }
            }
        }
        fn obj(&mut self) -> Result<Json, String> {
            self.expect(b'{')?;
            let mut m = Vec::new();
            self.ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Ok(Json::Obj(m));
            }
            loop {
                self.ws();
                let k = self.string()?;
                self.expect(b':')?;
                let v = self.value()?;
                m.push((k, v));
                self.ws();
                match self.peek() {
                    Some(b',') => self.i += 1,
                    Some(b'}') => {
                        self.i += 1;
                        return Ok(Json::Obj(m));
                    }
                    _ => return Err(format!("expected ',' or '}}' at byte {}", self.i)),
                }
            }
        }
    }
    let mut p = P { b: text.as_bytes(), i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != text.len() {
        return Err(format!("trailing data at byte {}", p.i));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let v = parse(r#"{"a": [1, 2.5, "x\ny", true, null], "b": {"c": "é"}}"#).unwrap();
        assert_eq!(v.get("a").and_then(|a| a.idx(0)).and_then(Json::as_num), Some(1.0));
        assert_eq!(v.get("a").and_then(|a| a.idx(2)).and_then(Json::as_str), Some("x\ny"));
        assert_eq!(v.get("b").and_then(|b| b.get("c")).and_then(Json::as_str), Some("é"));
        let s = dumps(&v);
        let v2 = parse(&s).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn json_numbers_print_int_when_whole() {
        assert_eq!(dumps(&Json::Num(3.0)), "3");
        assert_eq!(dumps(&Json::Num(0.001)), "0.001");
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("[1,]").is_err());
    }
}
