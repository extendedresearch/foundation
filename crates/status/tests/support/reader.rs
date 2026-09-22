//! A reader for the shared-rule vector, so consuming it adds no dependency —
//! this crate has none, and a vector is not a reason to gain one.
//!
//! It accepts what `vectors/*.json` is written in and nothing else: objects,
//! arrays and strings, nested, with no number, `true`, `false` or `null`
//! anywhere, and no escape sequence inside a string. Every integer in a vector
//! is a decimal string, the encoding `crates/clock/vectors` established, so
//! there is nothing here that reads a number. A file outside that subset panics
//! with the byte offset rather than being half read.
//!
//! Two other readers exist in this repository, in `crates/clock` and
//! `crates/pyo3`, each for the subset its own vectors use. They are not one
//! file shared between crates: a crate reaching into another crate's test tree
//! is the undeclared seam `docs/specs/boundary-enforcement-plan.md` exists to
//! catch, and a byte-identical copy is a finding of
//! `ecosystem/check.py duplication`.

/// A string, an array, or members in file order.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn at(&self, key: &str) -> &Value {
        self.get(key)
            .unwrap_or_else(|| panic!("no member {key:?} in {self:?}"))
    }

    pub fn text(&self) -> &str {
        match self {
            Value::Text(s) => s,
            other => panic!("{other:?} is not a string"),
        }
    }

    pub fn text_at(&self, key: &str) -> &str {
        self.at(key).text()
    }

    pub fn members(&self) -> &[(String, Value)] {
        match self {
            Value::Map(members) => members,
            other => panic!("{other:?} is not an object"),
        }
    }
}

pub fn read(text: &str) -> Value {
    let bytes = text.as_bytes();
    let mut at = 0;
    let value = value(bytes, &mut at);
    space(bytes, &mut at);
    assert_eq!(at, bytes.len(), "trailing bytes at {at}");
    value
}

fn space(bytes: &[u8], at: &mut usize) {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
}

fn expect(bytes: &[u8], at: &mut usize, byte: u8) {
    space(bytes, at);
    assert_eq!(
        bytes.get(*at).copied(),
        Some(byte),
        "byte {at}: expected {:?}",
        byte as char
    );
    *at += 1;
}

fn value(bytes: &[u8], at: &mut usize) -> Value {
    space(bytes, at);
    match bytes.get(*at) {
        Some(b'"') => Value::Text(string(bytes, at)),
        Some(b'[') => {
            *at += 1;
            let mut items = Vec::new();
            space(bytes, at);
            if bytes.get(*at) == Some(&b']') {
                *at += 1;
                return Value::List(items);
            }
            loop {
                items.push(value(bytes, at));
                space(bytes, at);
                if bytes.get(*at) == Some(&b',') {
                    *at += 1;
                } else {
                    break;
                }
            }
            expect(bytes, at, b']');
            Value::List(items)
        }
        Some(b'{') => {
            *at += 1;
            let mut members = Vec::new();
            space(bytes, at);
            if bytes.get(*at) == Some(&b'}') {
                *at += 1;
                return Value::Map(members);
            }
            loop {
                let key = string(bytes, at);
                expect(bytes, at, b':');
                members.push((key, value(bytes, at)));
                space(bytes, at);
                if bytes.get(*at) == Some(&b',') {
                    *at += 1;
                } else {
                    break;
                }
            }
            expect(bytes, at, b'}');
            Value::Map(members)
        }
        other => panic!("byte {at}: {other:?} begins no value this reader accepts"),
    }
}

fn string(bytes: &[u8], at: &mut usize) -> String {
    expect(bytes, at, b'"');
    let start = *at;
    while let Some(byte) = bytes.get(*at) {
        match byte {
            b'"' => {
                let text = std::str::from_utf8(&bytes[start..*at]).expect("utf-8");
                *at += 1;
                return text.to_owned();
            }
            // The vectors carry no escape, and one read as its own bytes would
            // be a different string than the file shows.
            b'\\' => panic!("byte {at}: this reader takes no escape sequence"),
            _ => *at += 1,
        }
    }
    panic!("byte {at}: the file ended inside a string")
}
