//! The flat JSON objects of the rendezvous files: string and non-negative
//! integer members; members of other types are skipped.

use std::collections::BTreeMap;
use std::io;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    Number(u64),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Object(BTreeMap<String, Value>);

impl Object {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.0.insert(name.to_string(), value.into());
        self
    }

    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        match self.0.get(name) {
            Some(Value::Text(text)) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn number(&self, name: &str) -> Option<u64> {
        match self.0.get(name) {
            Some(Value::Number(number)) => Some(*number),
            _ => None,
        }
    }

    #[cfg_attr(not(any(unix, windows)), allow(dead_code))] // used by the OS-specific kinds
    pub(crate) fn require_number(&self, name: &str) -> io::Result<u64> {
        self.number(name)
            .ok_or_else(|| invalid(format!("the handshake lacks the number `{name}`")))
    }

    pub(crate) fn to_json(&self) -> String {
        let mut out = String::from("{");
        for (index, (name, value)) in self.0.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write_string(&mut out, name);
            out.push(':');
            match value {
                Value::Text(text) => write_string(&mut out, text),
                Value::Number(number) => out.push_str(&number.to_string()),
            }
        }
        out.push('}');
        out
    }

    pub(crate) fn parse(text: &str) -> io::Result<Self> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            at: 0,
        };
        parser.space();
        let object = parser.object()?;
        parser.space();
        if parser.at != parser.bytes.len() {
            return Err(invalid("trailing characters after the JSON object".into()));
        }
        Ok(object)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Value::Text(text.to_string())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Value::Text(text)
    }
}

impl From<u64> for Value {
    fn from(number: u64) -> Self {
        Value::Number(number)
    }
}

impl From<u32> for Value {
    fn from(number: u32) -> Self {
        Value::Number(number.into())
    }
}

/// Parses a JSON array of strings, such as the argument vector of `exec:`.
pub(crate) fn parse_strings(text: &str) -> io::Result<Vec<String>> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.expect(b'[')?;
    let mut strings = Vec::new();
    parser.space();
    if parser.peek() == Some(b']') {
        parser.at += 1;
    } else {
        loop {
            parser.space();
            strings.push(parser.string()?);
            parser.space();
            match parser.peek() {
                Some(b',') => parser.at += 1,
                Some(b']') => {
                    parser.at += 1;
                    break;
                }
                _ => return Err(invalid("expected `,` or `]` in a JSON array".into())),
            }
        }
    }
    parser.space();
    if parser.at != parser.bytes.len() {
        return Err(invalid("trailing characters after the JSON array".into()));
    }
    Ok(strings)
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

enum Parsed {
    Keep(Value),
    Skip,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> io::Result<()> {
        self.space();
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(invalid(format!(
                "expected `{}` at offset {} of the JSON object",
                byte as char, self.at
            )))
        }
    }

    fn object(&mut self) -> io::Result<Object> {
        self.expect(b'{')?;
        let mut members = BTreeMap::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Object(members));
        }
        loop {
            self.space();
            let name = self.string()?;
            self.expect(b':')?;
            if let Parsed::Keep(value) = self.value(0)? {
                members.insert(name, value);
            }
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Object(members));
                }
                _ => return Err(invalid("expected `,` or `}` in the JSON object".into())),
            }
        }
    }

    fn value(&mut self, depth: usize) -> io::Result<Parsed> {
        if depth > 32 {
            return Err(invalid("the JSON object nests too deeply".into()));
        }
        self.space();
        match self.peek() {
            Some(b'"') => Ok(Parsed::Keep(Value::Text(self.string()?))),
            Some(b'0'..=b'9') => {
                let start = self.at;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
                if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
                    self.number_tail();
                    return Ok(Parsed::Skip);
                }
                let digits = std::str::from_utf8(&self.bytes[start..self.at]).expect("digits");
                Ok(match digits.parse() {
                    Ok(number) => Parsed::Keep(Value::Number(number)),
                    Err(_) => Parsed::Skip,
                })
            }
            Some(b'-') => {
                self.at += 1;
                self.number_tail();
                Ok(Parsed::Skip)
            }
            Some(b't') => self.word("true"),
            Some(b'f') => self.word("false"),
            Some(b'n') => self.word("null"),
            Some(b'[') => {
                self.at += 1;
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Parsed::Skip);
                }
                loop {
                    self.value(depth + 1)?;
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Parsed::Skip);
                        }
                        _ => return Err(invalid("expected `,` or `]` in a JSON array".into())),
                    }
                }
            }
            Some(b'{') => {
                self.object_skip(depth + 1)?;
                Ok(Parsed::Skip)
            }
            _ => Err(invalid(format!("unexpected JSON at offset {}", self.at))),
        }
    }

    fn object_skip(&mut self, depth: usize) -> io::Result<()> {
        self.expect(b'{')?;
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            self.string()?;
            self.expect(b':')?;
            self.value(depth)?;
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err(invalid("expected `,` or `}` in a JSON object".into())),
            }
        }
    }

    fn number_tail(&mut self) {
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        ) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &str) -> io::Result<Parsed> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(Parsed::Skip)
        } else {
            Err(invalid(format!("unexpected JSON at offset {}", self.at)))
        }
    }

    fn string(&mut self) -> io::Result<String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            while !matches!(self.peek(), Some(b'"' | b'\\') | None) {
                self.at += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.bytes[start..self.at])
                    .map_err(|_| invalid("a JSON string is not UTF-8".into()))?,
            );
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self
                        .peek()
                        .ok_or_else(|| invalid("unended escape".into()))?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                0x10000
                                    + ((high - 0xD800) << 10)
                                    + (low.wrapping_sub(0xDC00) & 0x3ff)
                            } else {
                                high
                            };
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(invalid("unknown JSON escape".into())),
                    }
                }
                _ => return Err(invalid("unended JSON string".into())),
            }
        }
    }

    fn hex4(&mut self) -> io::Result<u32> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .ok_or_else(|| invalid("bad `\\u` escape".into()))?;
        self.at += 4;
        Ok(digits)
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_skips_other_members() {
        let object = Object::new()
            .with("kind", "shm")
            .with("pid", 42u32)
            .with("path", "C:\\dir \"x\"");
        let parsed = Object::parse(&object.to_json()).unwrap();
        assert_eq!(parsed, object);
        let other = Object::parse(
            r#" {"v":1, "x":[1,{"a":null}], "b":true, "f":-1.5e3, "s":"\u00e9\ud83d\ude00"} "#,
        )
        .unwrap();
        assert_eq!(other.number("v"), Some(1));
        assert_eq!(other.text("s"), Some("é😀"));
        assert_eq!(other.number("f"), None);
        assert!(Object::parse("{\"a\":1} x").is_err());
        assert_eq!(
            parse_strings(r#" ["clink", "--serve", "a b"] "#).unwrap(),
            ["clink", "--serve", "a b"]
        );
        assert!(parse_strings("[1]").is_err());
    }
}
