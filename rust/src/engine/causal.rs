//! Selected causal semantics. No I/O, owner fencing or replication.
use crate::LinkError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CausalDot {
    pub author: String,
    pub counter: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalPayload {
    pub source: u64,
    pub target: u64,
    pub name: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalVersion {
    pub address: u64,
    pub dot: CausalDot,
    pub clock: Vec<CausalDot>,
    pub payload: Option<CausalPayload>,
}
fn invalid() -> LinkError {
    super::error("invalid causal version")
}
fn author(value: &str) -> Result<(), LinkError> {
    if value.len() != 32
        || value.bytes().all(|b| b == b'0')
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(())
}
fn counter(value: &Value, positive: bool) -> Result<u64, LinkError> {
    let text = value.as_str().ok_or_else(invalid)?;
    if text.is_empty() || text.len() > 20 {
        return Err(invalid());
    }
    let result = text.parse::<u64>().map_err(|_| invalid())?;
    if result.to_string() != text || (positive && result == 0) {
        return Err(invalid());
    }
    Ok(result)
}
fn array(value: &Value, count: usize) -> Result<&[Value], LinkError> {
    value
        .as_array()
        .filter(|v| v.len() == count)
        .map(Vec::as_slice)
        .ok_or_else(invalid)
}
fn dot(value: &Value) -> Result<CausalDot, LinkError> {
    let pair = array(value, 2)?;
    Ok(CausalDot {
        author: pair[0].as_str().ok_or_else(invalid)?.to_owned(),
        counter: counter(&pair[1], true)?,
    })
}
fn hex(value: &str) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    value
        .bytes()
        .flat_map(|b| {
            [
                DIGITS[(b >> 4) as usize] as char,
                DIGITS[(b & 15) as usize] as char,
            ]
        })
        .collect()
}
fn name(value: &Value) -> Result<Option<String>, LinkError> {
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().ok_or_else(invalid)?;
    if text.len() > 8192
        || text.len() % 2 != 0
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map(Some).map_err(|_| invalid())
}
impl CausalVersion {
    pub fn encode(&self) -> Result<String, LinkError> {
        if self.address == 0
            || self.dot.counter == 0
            || self.clock.is_empty()
            || self.clock.len() > 64
        {
            return Err(invalid());
        }
        author(&self.dot.author)?;
        let mut previous = "";
        let mut found = false;
        for item in &self.clock {
            author(&item.author)?;
            if item.counter == 0 || item.author.as_str() <= previous {
                return Err(invalid());
            }
            previous = &item.author;
            if item.author == self.dot.author {
                if item.counter != self.dot.counter {
                    return Err(invalid());
                }
                found = true;
            }
        }
        if !found
            || self
                .payload
                .as_ref()
                .and_then(|v| v.name.as_ref())
                .is_some_and(|v| v.len() > 4096)
        {
            return Err(invalid());
        }
        let payload = self.payload.as_ref().map(|row| {
            json!([
                row.source.to_string(),
                row.target.to_string(),
                row.name.as_deref().map(hex)
            ])
        });
        Ok(json!([
            "causal-version1",
            self.address.to_string(),
            [self.dot.author, self.dot.counter.to_string()],
            self.clock
                .iter()
                .map(|d| json!([d.author, d.counter.to_string()]))
                .collect::<Vec<_>>(),
            payload
        ])
        .to_string())
    }
    pub fn decode(wire: &str) -> Result<Self, LinkError> {
        if wire.len() > 65536 || !wire.bytes().all(|b| (b' '..=b'~').contains(&b)) {
            return Err(invalid());
        }
        let parsed: Value = serde_json::from_str(wire).map_err(|_| invalid())?;
        let record = array(&parsed, 5)?;
        if record[0] != "causal-version1" {
            return Err(invalid());
        }
        let clock = record[3]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 64)
            .ok_or_else(invalid)?;
        let payload = if record[4].is_null() {
            None
        } else {
            let row = array(&record[4], 3)?;
            Some(CausalPayload {
                source: counter(&row[0], false)?,
                target: counter(&row[1], false)?,
                name: name(&row[2])?,
            })
        };
        let result = Self {
            address: counter(&record[1], true)?,
            dot: dot(&record[2])?,
            clock: clock.iter().map(dot).collect::<Result<_, _>>()?,
            payload,
        };
        if result.encode()? != wire {
            return Err(invalid());
        }
        Ok(result)
    }
    fn checked(values: &[Self]) -> Result<(), LinkError> {
        if values.len() > 64 {
            return Err(invalid());
        }
        for value in values {
            value.encode()?;
        }
        Ok(())
    }
    fn dominates(&self, other: &Self) -> bool {
        other.clock.iter().all(|item| {
            self.clock
                .iter()
                .any(|value| value.author == item.author && value.counter >= item.counter)
        })
    }
    pub fn merge(left: &[Self], right: &[Self]) -> Result<Vec<Self>, LinkError> {
        Self::checked(left)?;
        Self::checked(right)?;
        let mut unique: Vec<&Self> = Vec::new();
        let address = left.first().or_else(|| right.first()).map(|v| v.address);
        for value in left.iter().chain(right) {
            if Some(value.address) != address {
                return Err(invalid());
            }
            let mut duplicate = false;
            for previous in &unique {
                if previous.dot == value.dot || previous.clock == value.clock {
                    if **previous != *value {
                        return Err(invalid());
                    }
                    duplicate = true;
                }
            }
            if !duplicate {
                unique.push(value);
            }
        }
        let mut result: Vec<Self> = unique
            .iter()
            .filter(|value| {
                !unique
                    .iter()
                    .any(|other| other.dot != value.dot && other.dominates(value))
            })
            .map(|v| (**v).clone())
            .collect();
        if result.len() > 64 {
            return Err(invalid());
        }
        result.sort_by(|a, b| a.dot.cmp(&b.dot));
        Ok(result)
    }
    /// Construct only: caller must persist counter and mutation atomically and
    /// fence duplicate author owners before publishing the result.
    pub fn advance(
        address: u64,
        local_author: &str,
        last_counter: u64,
        observed: &[Self],
        payload: Option<CausalPayload>,
    ) -> Result<Self, LinkError> {
        if address == 0 {
            return Err(invalid());
        }
        author(local_author)?;
        Self::merge(observed, &[])?;
        let mut clock: BTreeMap<String, u64> = BTreeMap::new();
        for value in observed {
            if value.address != address {
                return Err(invalid());
            }
            for item in &value.clock {
                let count = clock.entry(item.author.clone()).or_default();
                *count = (*count).max(item.counter);
            }
        }
        let next = last_counter
            .max(clock.get(local_author).copied().unwrap_or_default())
            .checked_add(1)
            .ok_or_else(invalid)?;
        clock.insert(local_author.to_owned(), next);
        let result = Self {
            address,
            dot: CausalDot {
                author: local_author.to_owned(),
                counter: next,
            },
            clock: clock
                .into_iter()
                .map(|(author, counter)| CausalDot { author, counter })
                .collect(),
            payload,
        };
        result.encode()?;
        Ok(result)
    }
}
