//! The JSON of GraphQL over HTTP and WebSocket: a value keeps the order of its
//! object keys, so a response lists its fields in the order they were asked.

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json> {
        let mut reader = Reader {
            bytes: text.as_bytes(),
            at: 0,
        };
        let value = reader.value(0)?;
        reader.space();
        if reader.at != reader.bytes.len() {
            bail!("Unexpected text after JSON at byte {}.", reader.at);
        }
        Ok(value)
    }

    pub fn str(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }

    pub fn object(pairs: Vec<(&str, Json)>) -> Json {
        Json::Object(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(value) => Some(*value),
            Json::Float(value) if value.fract() == 0.0 => Some(*value as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(value) => Some(*value as f64),
            Json::Float(value) => Some(*value),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    /// One object or a list of them, as a list: GraphQL coerces a single
    /// value to a list of one, and DeepClient relies on it.
    pub fn as_list(&self) -> Vec<&Json> {
        match self {
            Json::Array(items) => items.iter().collect(),
            Json::Null => Vec::new(),
            other => vec![other],
        }
    }

    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Int(value) => out.push_str(&value.to_string()),
            Json::Float(value) if value.is_finite() => out.push_str(&value.to_string()),
            Json::Float(_) => out.push_str("null"),
            Json::String(text) => write_string(text, out),
            Json::Array(items) => {
                out.push('[');
                for (position, item) in items.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Json::Object(pairs) => {
                out.push('{');
                for (position, (key, value)) in pairs.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32))
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Deep enough for any request a client means; a deeper one is refused rather
/// than overflowing the stack.
const MAX_DEPTH: usize = 128;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, text: &str, value: Json) -> Result<Json> {
        if self.bytes[self.at..].starts_with(text.as_bytes()) {
            self.at += text.len();
            Ok(value)
        } else {
            bail!("Invalid JSON at byte {}.", self.at)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json> {
        if depth > MAX_DEPTH {
            bail!("JSON is nested too deeply.");
        }
        self.space();
        match self.bytes.get(self.at) {
            Some(b'n') => self.literal("null", Json::Null),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.bytes.get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Json::Array(items));
                        }
                        _ => bail!("Invalid JSON array at byte {}.", self.at),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut pairs = Vec::new();
                self.space();
                if self.bytes.get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Ok(Json::Object(pairs));
                }
                loop {
                    self.space();
                    if self.bytes.get(self.at) != Some(&b'"') {
                        bail!("Invalid JSON object at byte {}.", self.at);
                    }
                    let key = self.string()?;
                    self.space();
                    if self.bytes.get(self.at) != Some(&b':') {
                        bail!("Invalid JSON object at byte {}.", self.at);
                    }
                    self.at += 1;
                    pairs.push((key, self.value(depth + 1)?));
                    self.space();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Json::Object(pairs));
                        }
                        _ => bail!("Invalid JSON object at byte {}.", self.at),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => bail!("Invalid JSON at byte {}.", self.at),
        }
    }

    fn number(&mut self) -> Result<Json> {
        let start = self.at;
        while matches!(
            self.bytes.get(self.at),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])?;
        if let Ok(value) = text.parse::<i64>() {
            return Ok(Json::Int(value));
        }
        match text.parse::<f64>() {
            Ok(value) => Ok(Json::Float(value)),
            Err(_) => bail!("Invalid JSON number at byte {start}."),
        }
    }

    fn hex4(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u32::from_str_radix(digits, 16).ok());
        match digits {
            Some(value) => {
                self.at += 4;
                Ok(value)
            }
            None => bail!("Invalid JSON escape at byte {}.", self.at),
        }
    }

    fn string(&mut self) -> Result<String> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let Some(&byte) = self.bytes.get(self.at) else {
                bail!("Unterminated JSON string.");
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(String::from_utf8(out)?),
                b'\\' => {
                    let Some(&escape) = self.bytes.get(self.at) else {
                        bail!("Unterminated JSON string.");
                    };
                    self.at += 1;
                    let character = match escape {
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'/' => '/',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                code = 0x10000
                                    + ((code - 0xD800) << 10)
                                    + (low.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => bail!("Invalid JSON escape at byte {}.", self.at),
                    };
                    let mut buffer = [0; 4];
                    out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
                other => out.push(other),
            }
        }
    }
}
