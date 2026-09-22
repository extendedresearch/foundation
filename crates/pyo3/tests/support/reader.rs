//! A reader for the shared-rule vector, so consuming it adds no dependency.
//!
//! It accepts the subset `vectors/*.json` is written in — objects, arrays and
//! strings, with no number, `true`, `false` or `null` anywhere — and keeps
//! object members in file order. Every integer in a vector is a decimal string,
//! which is the encoding `crates/clock/vectors` established, so there is
//! nothing here that reads a number.
//!
//! `extendedresearch-clock` carries a fuller reader for its own vectors. This
//! one is separate rather than shared: a crate reaching into another crate's
//! test tree is the kind of undeclared seam
//! `docs/specs/boundary-enforcement-plan.md` exists to catch, and a copy of
//! that file would be a finding of `ecosystem/check.py duplication`.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    List(Vec<Value>),
    /// Members in file order.
    Map(Vec<(String, Value)>),
}

impl Value {
    /// The member `key`, or `None` when this is not a map or has no such
    /// member.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The member `key`, which the caller requires.
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

    /// The string member `key`.
    pub fn text_at(&self, key: &str) -> &str {
        self.at(key).text()
    }

    pub fn list(&self) -> &[Value] {
        match self {
            Value::List(items) => items,
            other => panic!("{other:?} is not an array"),
        }
    }

    pub fn members(&self) -> &[(String, Value)] {
        match self {
            Value::Map(members) => members,
            other => panic!("{other:?} is not an object"),
        }
    }
}

/// Read one vector file. Panics with the byte offset on anything outside the
/// subset above, because a vector that does not parse is a broken file rather
/// than a case to handle.
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
                match bytes.get(*at) {
                    Some(b',') => *at += 1,
                    _ => break,
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
                space(bytes, at);
                let key = string(bytes, at);
                expect(bytes, at, b':');
                members.push((key, value(bytes, at)));
                space(bytes, at);
                match bytes.get(*at) {
                    Some(b',') => *at += 1,
                    _ => break,
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
            // The vectors carry no escape, and one arriving silently would
            // become a different string than the file shows.
            b'\\' => panic!("byte {at}: this reader takes no escape sequence"),
            _ => *at += 1,
        }
    }
    panic!("byte {at}: the file ended inside a string")
}
