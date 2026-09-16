//! A JSON reader for the conformance vectors, so reading them adds no
//! dependency to the crate.
//!
//! It accepts RFC 8259 JSON and keeps what the vector format needs: object keys
//! in file order, and numbers as their source text, since every value the
//! vectors compare is an integer or a decimal string.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The number's text as written.
    Number(String),
    String(String),
    Array(Vec<Json>),
    /// Members in file order.
    Object(Vec<(String, Json)>),
}

#[derive(Debug)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "byte {}: {}", self.offset, self.message)
    }
}

pub fn parse(text: &str) -> Result<Json, ParseError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.whitespace();
    let value = parser.value(0)?;
    parser.whitespace();
    if parser.at != parser.bytes.len() {
        return Err(parser.error("trailing characters after the value"));
    }
    Ok(value)
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(members) => Some(members),
            _ => None,
        }
    }

    /// A JSON number that is a non-negative integer.
    pub fn as_integer(&self) -> Option<u64> {
        match self {
            Json::Number(text) => text.parse().ok(),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> ParseError {
        ParseError {
            offset: self.at,
            message: message.to_owned(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.at += 1;
        Some(b)
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.bump() == Some(byte) {
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, ParseError> {
        if self.bytes.get(self.at..self.at + word.len()) == Some(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            if members.iter().any(|(k, _)| *k == key) {
                return Err(self.error(&format!("duplicate key {key:?}")));
            }
            self.whitespace();
            self.expect(b':')?;
            self.whitespace();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.whitespace();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => return Ok(Json::Object(members)),
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.whitespace();
            items.push(self.value(depth + 1)?);
            self.whitespace();
            match self.bump() {
                Some(b',') => continue,
                Some(b']') => return Ok(Json::Array(items)),
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(self.error("expected a digit")),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.error("number is not UTF-8"))?;
        Ok(Json::Number(text.to_owned()))
    }

    fn digits(&mut self) -> Result<(), ParseError> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(self.error("expected a digit"));
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.bump() {
                Some(b @ b'0'..=b'9') => b - b'0',
                Some(b @ b'a'..=b'f') => b - b'a' + 10,
                Some(b @ b'A'..=b'F') => b - b'A' + 10,
                _ => return Err(self.error("expected four hex digits")),
            };
            value = value * 16 + u32::from(digit);
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            while matches!(self.peek(), Some(b) if b != b'"' && b != b'\\' && b >= 0x20) {
                self.at += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.bytes[start..self.at])
                    .map_err(|_| self.error("string is not UTF-8"))?,
            );
            match self.bump() {
                Some(b'"') => return Ok(out),
                Some(b'\\') => {
                    let c = match self.bump() {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high) {
                                if self.bump() != Some(b'\\') || self.bump() != Some(b'u') {
                                    return Err(self.error("unpaired surrogate"));
                                }
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error("unpaired surrogate"));
                                }
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            };
                            char::from_u32(code).ok_or_else(|| self.error("invalid code point"))?
                        }
                        _ => return Err(self.error("invalid escape")),
                    };
                    out.push(c);
                }
                _ => return Err(self.error("unterminated string or control character")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_vectors_use() {
        let v = parse(r#" { "a": "x\u00e9\"", "b": [1, -2, 3.5e2, true, false, null], "c": {} } "#)
            .unwrap();
        assert_eq!(v.get("a").and_then(Json::as_str), Some("x\u{e9}\""));
        let b = v.get("b").and_then(Json::as_array).unwrap();
        assert_eq!(b[0].as_integer(), Some(1));
        assert_eq!(b[1], Json::Number("-2".to_owned()));
        assert_eq!(b[1].as_integer(), None);
        assert_eq!(b[2], Json::Number("3.5e2".to_owned()));
        assert_eq!(&b[3..], &[Json::Bool(true), Json::Bool(false), Json::Null]);
        assert_eq!(
            v.get("c").and_then(Json::as_object).map(<[_]>::len),
            Some(0)
        );
        assert_eq!(
            parse(r#""\ud83d\ude00""#).unwrap(),
            Json::String("\u{1f600}".to_owned())
        );
    }

    #[test]
    fn refuses_what_is_not_json() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\":1,}",
            "01",
            "1.",
            "\"\\x\"",
            "\"a",
            "{\"a\":1 \"b\":2}",
            "{\"a\":1,\"a\":2}",
            "\"\\ud800\"",
            "nul",
            "1 2",
            "\"\t\"",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }
}
