//! Optional physical compression of complete engine revisions. Own one writer
//! and reopen through this decorator; the inner stream can contain delta1 rows.
use super::{error, journal::checksum};
use crate::{LinkError, TransitionLogStore};

#[derive(PartialEq, Eq)]
enum Kind {
    Snapshot,
    Changes,
    Checkpoint,
}
struct Frame {
    kind: Kind,
    revision: u64,
    next: String,
    rows: String,
    hash: String,
}
fn decimal(text: &str, max: u128) -> Result<u128, LinkError> {
    if text.is_empty()
        || text.len() > 39
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error("invalid delta journal decimal"));
    }
    text.parse::<u128>()
        .ok()
        .filter(|value| *value <= max)
        .ok_or_else(|| error("delta journal decimal out of range"))
}
fn checked(text: &str) -> String {
    format!("{text}|{:016x}", checksum(text))
}
fn verified(entry: &str) -> Result<Vec<&str>, LinkError> {
    if !entry.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(error("delta journal requires printable ASCII records"));
    }
    let (text, hash) = entry
        .rsplit_once('|')
        .ok_or_else(|| error("missing delta journal checksum"))?;
    if hash.len() != 16 || hash != format!("{:016x}", checksum(text)) {
        return Err(error("delta journal checksum mismatch"));
    }
    Ok(entry.split('|').collect())
}
fn frame(entry: &str) -> Result<Frame, LinkError> {
    let fields = verified(entry)?;
    let kind = match (fields[0], fields.len()) {
        ("engine1", 5) => Kind::Snapshot,
        ("engine2", 6) => Kind::Changes,
        ("engine3", 5) => Kind::Checkpoint,
        _ => return Err(error("expected an engine revision")),
    };
    let offset = usize::from(kind == Kind::Changes);
    let revision = decimal(fields[1], u128::from(u64::MAX))? as u64;
    let next = decimal(fields[2 + offset], u128::MAX)?;
    if next == 0 || (revision == 0 && kind != Kind::Checkpoint) {
        return Err(error("invalid delta journal revision or allocator"));
    }
    if revision == 0 && (next != 1 || !fields[3].is_empty()) {
        return Err(error("invalid revision-zero checkpoint"));
    }
    if kind == Kind::Changes
        && decimal(fields[2], u128::from(u64::MAX))? != u128::from(revision - 1)
    {
        return Err(error("invalid logical change base"));
    }
    Ok(Frame {
        kind,
        revision,
        next: fields[2 + offset].into(),
        rows: fields[3 + offset].into(),
        hash: fields[4 + offset].into(),
    })
}
fn contiguous(before: Option<&Frame>, after: &Frame) -> Result<(), LinkError> {
    if after.kind == Kind::Checkpoint {
        return if before.is_none() {
            Ok(())
        } else {
            Err(error("checkpoint must be the first delta journal record"))
        };
    }
    let expected = match before {
        Some(before) => before.revision.checked_add(1),
        None => Some(1),
    };
    if expected != Some(after.revision) {
        return Err(error("delta journal revisions are not contiguous"));
    }
    Ok(())
}
fn expand(before: Option<&Frame>, entry: &str) -> Result<String, LinkError> {
    if !entry.starts_with("delta1|") {
        return Ok(entry.into());
    }
    let fields = verified(entry)?;
    if fields.len() != 9 {
        return Err(error("invalid delta journal envelope"));
    }
    let before = before.ok_or_else(|| error("delta journal base is missing"))?;
    if before.kind == Kind::Changes {
        return Err(error("delta requires a complete snapshot base"));
    }
    if fields[1] != before.hash {
        return Err(error("delta journal base checksum mismatch"));
    }
    let prefix = decimal(fields[4], u128::from(u32::MAX))? as usize;
    let suffix = decimal(fields[5], u128::from(u32::MAX))? as usize;
    if prefix > before.rows.len() || suffix > before.rows.len() - prefix {
        return Err(error("delta journal span outside base"));
    }
    Ok(format!(
        "engine1|{}|{}|{}{}{}|{}",
        fields[2],
        fields[3],
        &before.rows[..prefix],
        fields[6],
        &before.rows[before.rows.len() - suffix..],
        fields[7]
    ))
}
fn compress(a: &Frame, b: &Frame, full: &str) -> String {
    let prefix = a
        .rows
        .bytes()
        .zip(b.rows.bytes())
        .take(u32::MAX as usize)
        .take_while(|(x, y)| x == y)
        .count();
    let suffix = a.rows.as_bytes()[prefix..]
        .iter()
        .rev()
        .zip(b.rows.as_bytes()[prefix..].iter().rev())
        .take(u32::MAX as usize)
        .take_while(|(x, y)| x == y)
        .count();
    let delta = checked(&format!(
        "delta1|{}|{}|{}|{prefix}|{suffix}|{}|{}",
        a.hash,
        b.revision,
        b.next,
        &b.rows[prefix..b.rows.len() - suffix],
        b.hash
    ));
    if delta.len() < full.len() {
        delta
    } else {
        full.into()
    }
}

/// Single-span delta encoding with at most interval-1 consecutive deltas.
/// Compress complete snapshots; logical change records pass through unchanged.
/// Inner resources/fencing remain caller-owned. Mutation errors require reopen.
pub struct DeltaLog<L> {
    inner: L,
    codec: Codec,
}
// Keep one codec implementation across transport types. Only the three physical
// I/O calls use dynamic dispatch; the encoded format and recovery state are shared.
struct Codec {
    interval: u32,
    previous: Option<Frame>,
    streak: u64,
    ready: bool,
    poisoned: bool,
}
impl<L: TransitionLogStore> DeltaLog<L> {
    pub fn new(inner: L, checkpoint_interval: u32) -> Result<Self, LinkError> {
        Ok(Self {
            inner,
            codec: Codec::new(checkpoint_interval)?,
        })
    }
    pub fn into_inner(self) -> L {
        self.inner
    }
}
impl Codec {
    fn new(checkpoint_interval: u32) -> Result<Self, LinkError> {
        if checkpoint_interval == 0 {
            return Err(error("checkpoint interval must be positive"));
        }
        Ok(Self {
            interval: checkpoint_interval,
            previous: None,
            streak: 0,
            ready: false,
            poisoned: false,
        })
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.poisoned {
            Err(error(
                "indeterminate delta journal outcome; reopen the decorator",
            ))
        } else {
            Ok(())
        }
    }
    fn read(&mut self, inner: &mut dyn TransitionLogStore) -> Result<Vec<String>, LinkError> {
        self.check()?;
        self.ready = false;
        let mut logical = Vec::<String>::new();
        let mut previous = None;
        let mut streak = 0_u64;
        for entry in inner.read_log_entries()? {
            let full = expand(previous.as_ref(), &entry)?;
            let parsed = frame(&full)?;
            contiguous(previous.as_ref(), &parsed)?;
            previous = Some(parsed);
            streak = if entry.starts_with("delta1|") {
                streak + 1
            } else {
                0
            };
            logical.push(full);
        }
        self.previous = previous;
        self.streak = streak;
        self.ready = true;
        Ok(logical)
    }
    fn append(&mut self, inner: &mut dyn TransitionLogStore, entry: &str) -> Result<(), LinkError> {
        self.check()?;
        if !self.ready {
            self.read(inner)?;
        }
        let parsed = frame(entry)?;
        contiguous(self.previous.as_ref(), &parsed)?;
        let physical = match &self.previous {
            Some(before)
                if before.kind != Kind::Changes
                    && parsed.kind != Kind::Changes
                    && self.streak + 1 < u64::from(self.interval) =>
            {
                compress(before, &parsed, entry)
            }
            _ => entry.into(),
        };
        if let Err(cause) = inner.append_log_entry(&physical) {
            self.poisoned = true;
            return Err(cause);
        }
        self.streak = if physical.starts_with("delta1|") {
            self.streak + 1
        } else {
            0
        };
        self.previous = Some(parsed);
        Ok(())
    }
    fn flush(&mut self, inner: &mut dyn TransitionLogStore) -> Result<(), LinkError> {
        self.check()?;
        if let Err(cause) = inner.flush_log() {
            self.poisoned = true;
            return Err(cause);
        }
        Ok(())
    }
}
impl<L: TransitionLogStore> TransitionLogStore for DeltaLog<L> {
    fn supports_durable_flush(&self) -> bool {
        self.inner.supports_durable_flush()
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.codec.read(&mut self.inner)
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.codec.append(&mut self.inner, entry)
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.codec.flush(&mut self.inner)
    }
}
