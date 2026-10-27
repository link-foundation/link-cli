//! Selected bounded local causal ownership; no global atomicity claim.
use super::{
    causal::{CausalDot, CausalPayload, CausalVersion},
    cluster::{AtomicJournalStore, JournalVersion},
};
use crate::LinkError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn invalid() -> LinkError {
    LinkError::InvalidFormat("invalid causal replica state".into())
}
fn identity(value: &str) -> Result<(), LinkError> {
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
fn raw(value: &CausalVersion) -> Result<Value, LinkError> {
    serde_json::from_str(&value.encode()?).map_err(|_| invalid())
}
fn parsed(value: &Value) -> Result<CausalVersion, LinkError> {
    CausalVersion::decode(&value.to_string())
}
fn values(value: &Value) -> Result<Vec<CausalVersion>, LinkError> {
    value
        .as_array()
        .filter(|v| v.len() <= 64)
        .ok_or_else(invalid)?
        .iter()
        .map(parsed)
        .collect()
}
fn compare(a: &CausalVersion, b: &CausalVersion) -> std::cmp::Ordering {
    a.address.cmp(&b.address).then_with(|| a.dot.cmp(&b.dot))
}
fn evidence(values: &[CausalVersion]) -> Result<Vec<CausalVersion>, LinkError> {
    CausalVersion::merge(values, &[])?;
    let mut result = values.to_vec();
    result.sort_by(compare);
    result.dedup();
    Ok(result)
}
fn descriptor(
    address: u64,
    context: &[CausalVersion],
    payload: Option<CausalPayload>,
) -> Result<String, LinkError> {
    let dummy =
        CausalVersion::advance(address, "00000000000000000000000000000001", 0, &[], payload)?;
    Ok(json!([
        address.to_string(),
        context.iter().map(raw).collect::<Result<Vec<_>, _>>()?,
        raw(&dummy)?[4]
    ])
    .to_string())
}

#[derive(Clone, Copy, Debug)]
pub struct CausalReplicaOptions {
    pub max_entries: usize,
    pub max_bytes: usize,
    pub require_durable_flush: bool,
}
impl Default for CausalReplicaOptions {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_bytes: 8_388_608,
            require_durable_flush: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalSnapshot {
    pub author: String,
    pub last_counter: u64,
    pub versions: Vec<CausalVersion>,
}
struct State {
    author: String,
    counter: u64,
    history: BTreeMap<CausalDot, (String, CausalVersion)>,
    rows: BTreeMap<u64, Vec<CausalVersion>>,
    tokens: BTreeMap<String, (String, CausalVersion)>,
}
impl State {
    fn new(author: &str) -> Self {
        Self {
            author: author.into(),
            counter: 0,
            history: BTreeMap::new(),
            rows: BTreeMap::new(),
            tokens: BTreeMap::new(),
        }
    }
    fn remember(
        &mut self,
        items: &[CausalVersion],
        local: bool,
        update: bool,
    ) -> Result<(), LinkError> {
        let mut groups: BTreeMap<u64, Vec<CausalVersion>> = BTreeMap::new();
        for value in items {
            let wire = value.encode()?;
            if let Some((prior, _)) = self.history.get(&value.dot) {
                if prior != &wire {
                    return Err(invalid());
                }
            } else if !local && value.dot.author == self.author {
                return Err(invalid());
            }
            if value
                .clock
                .iter()
                .any(|v| v.author == self.author && v.counter > self.counter)
            {
                return Err(invalid());
            }
            for (prior, previous) in self.history.values() {
                if previous.address == value.address
                    && previous.clock == value.clock
                    && prior != &wire
                {
                    return Err(invalid());
                }
            }
            self.history
                .insert(value.dot.clone(), (wire, value.clone()));
            groups.entry(value.address).or_default().push(value.clone());
        }
        if update {
            for (address, group) in groups {
                let merged = CausalVersion::merge(
                    self.rows.get(&address).map_or(&[], Vec::as_slice),
                    &group,
                )?;
                self.rows.insert(address, merged);
            }
        }
        Ok(())
    }
    fn apply(&mut self, entry: &Value) -> Result<(), LinkError> {
        let entry = entry.as_array().ok_or_else(invalid)?;
        if entry.first() == Some(&json!("causal-receive1")) {
            if entry.len() != 2 {
                return Err(invalid());
            }
            let received = values(&entry[1])?;
            if received.is_empty()
                || received
                    .windows(2)
                    .any(|v| compare(&v[0], &v[1]) != std::cmp::Ordering::Less)
            {
                return Err(invalid());
            }
            return self.remember(&received, false, true);
        }
        if entry.len() != 4 || entry[0] != "causal-write1" {
            return Err(invalid());
        }
        let token = entry[1].as_str().ok_or_else(invalid)?;
        identity(token)?;
        if self.tokens.contains_key(token) {
            return Err(invalid());
        }
        let seen = evidence(&values(&entry[2])?)?;
        if json!(seen.iter().map(raw).collect::<Result<Vec<_>, _>>()?) != entry[2] {
            return Err(invalid());
        }
        let mutation = parsed(&entry[3])?;
        let context = CausalVersion::merge(&seen, &[])?;
        self.remember(&seen, false, false)?;
        let expected = CausalVersion::advance(
            mutation.address,
            &self.author,
            self.counter,
            &context,
            mutation.payload.clone(),
        )?;
        if expected != mutation {
            return Err(invalid());
        }
        self.counter = mutation.dot.counter;
        self.remember(std::slice::from_ref(&mutation), true, true)?;
        self.tokens.insert(
            token.into(),
            (
                descriptor(mutation.address, &context, mutation.payload.clone())?,
                mutation,
            ),
        );
        Ok(())
    }
    fn snapshot(&self) -> CausalSnapshot {
        let mut versions: Vec<_> = self.rows.values().flatten().cloned().collect();
        versions.sort_by(compare);
        CausalSnapshot {
            author: self.author.clone(),
            last_counter: self.counter,
            versions,
        }
    }
}

pub struct CausalReplica<S> {
    store: S,
    key: String,
    author: String,
    options: CausalReplicaOptions,
}
impl<S: AtomicJournalStore> CausalReplica<S> {
    pub fn new(
        store: S,
        key: &str,
        author: &str,
        options: CausalReplicaOptions,
    ) -> Result<Self, LinkError> {
        identity(author)?;
        if key.is_empty()
            || key.len() > 128
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || !(2..=4096).contains(&options.max_entries)
            || !(1..=8_388_608).contains(&options.max_bytes)
            || (options.require_durable_flush && !store.supports_durable_flush())
        {
            return Err(invalid());
        }
        Ok(Self {
            store,
            key: key.into(),
            author: author.into(),
            options,
        })
    }
    pub fn into_store(self) -> S {
        self.store
    }
    fn admit(&self, entries: &[String]) -> Result<(), LinkError> {
        if entries.len() > self.options.max_entries {
            return Err(invalid());
        }
        let mut bytes: usize = 0;
        for entry in entries {
            bytes = bytes.checked_add(entry.len()).ok_or_else(invalid)?;
            if entry.len() > 1_048_576 || bytes > self.options.max_bytes {
                return Err(invalid());
            }
            let (mut depth, mut separators, mut quoted) = (0i32, 0usize, false);
            // Canonical fields have no escapes; bound shape before JSON allocation.
            for item in entry.bytes() {
                if !(b' '..=b'~').contains(&item) || item == b'\\' {
                    return Err(invalid());
                }
                if item == b'"' {
                    quoted = !quoted;
                    continue;
                }
                if quoted {
                    continue;
                }
                match item {
                    b'{' | b'}' => return Err(invalid()),
                    b'[' => {
                        depth += 1;
                        if depth > 5 {
                            return Err(invalid());
                        }
                    }
                    b']' => {
                        depth -= 1;
                        if depth < 0 {
                            return Err(invalid());
                        }
                    }
                    b',' => {
                        separators += 1;
                        if separators > 32768 {
                            return Err(invalid());
                        }
                    }
                    _ => {}
                }
            }
            if quoted || depth != 0 {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn load(&mut self) -> Result<(JournalVersion, State), LinkError> {
        let mut result = self.store.read(std::slice::from_ref(&self.key))?;
        if result.len() != 1 {
            return Err(invalid());
        }
        let expected = result.remove(0);
        self.admit(&expected.entries)?;
        let mut state = State::new(&self.author);
        if expected.generation == 0 {
            if !expected.entries.is_empty() {
                return Err(invalid());
            }
            return Ok((expected, state));
        }
        if expected.entries.first() != Some(&json!(["causal-replica1", self.author]).to_string()) {
            return Err(invalid());
        }
        for entry in &expected.entries[1..] {
            let value: Value = serde_json::from_str(entry).map_err(|_| invalid())?;
            let canonical = value.to_string();
            if canonical != *entry {
                return Err(invalid());
            }
            state.apply(&value)?;
        }
        Ok((expected, state))
    }
    fn publish(&mut self, expected: JournalVersion, entry: Value) -> Result<(), LinkError> {
        let mut entries = expected.entries.clone();
        if entries.is_empty() {
            entries.push(json!(["causal-replica1", self.author]).to_string());
        }
        entries.push(entry.to_string());
        self.admit(&entries)?;
        match self.store.compare_exchange(std::slice::from_ref(&self.key), &[expected], &[Some(entries)]) {
            Ok(true) => Ok(()),
            Ok(false) => Err(LinkError::Transaction("conflict: causal state changed; reread and retry the same token".into())),
            Err(cause) => Err(LinkError::Transaction(format!("indeterminate causal publication; retry the same token after a fresh read: {cause}"))),
        }
    }
    pub fn snapshot(&mut self) -> Result<CausalSnapshot, LinkError> {
        Ok(self.load()?.1.snapshot())
    }
    pub fn read(&mut self, address: u64) -> Result<Vec<CausalVersion>, LinkError> {
        if address == 0 {
            return Err(invalid());
        }
        Ok(self.load()?.1.rows.remove(&address).unwrap_or_default())
    }
    pub fn write(
        &mut self,
        token: &str,
        address: u64,
        context: &[CausalVersion],
        payload: Option<CausalPayload>,
    ) -> Result<CausalVersion, LinkError> {
        identity(token)?;
        let admitted = evidence(context)?;
        let context = CausalVersion::merge(&admitted, &[])?;
        let input = descriptor(address, &context, payload.clone())?;
        let (expected, mut state) = self.load()?;
        state.remember(&admitted, false, false)?;
        if let Some((prior, value)) = state.tokens.get(token) {
            if *prior != input {
                return Err(invalid());
            }
            return Ok(value.clone());
        }
        let mutation =
            CausalVersion::advance(address, &self.author, state.counter, &context, payload)?;
        let entry = json!([
            "causal-write1",
            token,
            admitted.iter().map(raw).collect::<Result<Vec<_>, _>>()?,
            raw(&mutation)?
        ]);
        state.apply(&entry)?;
        self.publish(expected, entry)?;
        Ok(mutation)
    }
    pub fn receive(&mut self, incoming: &[CausalVersion]) -> Result<bool, LinkError> {
        if incoming.len() > 64 {
            return Err(invalid());
        }
        for value in incoming {
            value.encode()?;
        }
        let (expected, mut state) = self.load()?;
        let mut new_values = BTreeMap::new();
        for value in incoming {
            if !state.history.contains_key(&value.dot) {
                new_values.insert(value.dot.clone(), value.clone());
            }
        }
        state.remember(incoming, false, true)?;
        if new_values.is_empty() {
            return Ok(false);
        }
        let mut new_values: Vec<_> = new_values.into_values().collect();
        new_values.sort_by(compare);
        self.publish(
            expected,
            json!([
                "causal-receive1",
                new_values.iter().map(raw).collect::<Result<Vec<_>, _>>()?
            ]),
        )?;
        Ok(true)
    }
}
