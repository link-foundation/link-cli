//! Durable ownership of an unlogged checkout or branch switch.
use super::{
    decode_version_name, encode_version_name, GenericVersionControlDecorator, NamedTypeLinks,
};
use crate::transactions::{Transition, TransitionKind};
use anyhow::{bail, Result};
use std::collections::HashMap;

pub(super) const MOVEMENT_PREFIX: &str = "__vc:move:";

#[derive(Clone, Debug)]
pub(super) struct MovementIntent {
    pub from_branch: String,
    pub from: i64,
    pub to_branch: String,
    pub to: i64,
    pub undo: Vec<Transition>,
    pub redo: Vec<Transition>,
}

#[derive(Clone)]
pub(super) struct MovementState {
    intent: MovementIntent,
    link: u32,
    prepared: bool,
    rows_flushed: bool,
    published: bool,
}

fn checksum(body: &str) -> u64 {
    body.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

impl MovementIntent {
    pub fn encode(&self) -> String {
        let records = |records: &[Transition]| {
            records
                .iter()
                .map(|record| encode_version_name(&record.serialize()))
                .collect::<Vec<_>>()
                .join(",")
        };
        let body = format!(
            "1|{}|{}|{}|{}|{}|{}",
            encode_version_name(&self.from_branch),
            self.from,
            encode_version_name(&self.to_branch),
            self.to,
            records(&self.undo),
            records(&self.redo)
        );
        format!("{MOVEMENT_PREFIX}{body}|{:016x}", checksum(&body))
    }

    pub fn decode(marker: &str) -> Option<Self> {
        let text = marker.strip_prefix(MOVEMENT_PREFIX)?;
        let (body, hash) = text.rsplit_once('|')?;
        if hash.len() != 16 || u64::from_str_radix(hash, 16).ok()? != checksum(body) {
            return None;
        }
        let fields: Vec<_> = body.split('|').collect();
        if fields.len() != 7 || fields[0] != "1" {
            return None;
        }
        let records = |text: &str| -> Option<Vec<Transition>> {
            if text.is_empty() {
                return Some(Vec::new());
            }
            text.split(',')
                .map(|record| {
                    let text = decode_version_name(record)?;
                    let record = Transition::parse(&text).ok()?;
                    if record.serialize() != text
                        || record.transaction_id == 0
                        || record.sequence <= 0
                        || !(-62_135_596_800_000..=253_402_300_799_999)
                            .contains(&record.timestamp_ms)
                    {
                        return None;
                    }
                    let valid = match record.kind {
                        TransitionKind::Create => {
                            record.before.index == 0 && record.after.index != 0
                        }
                        TransitionKind::Update => {
                            record.before.index != 0 && record.before.index == record.after.index
                        }
                        TransitionKind::Delete => {
                            record.before.index != 0 && record.after.index == 0
                        }
                    };
                    valid.then_some(record)
                })
                .collect()
        };
        let intent = Self {
            from_branch: decode_version_name(fields[1])?,
            from: fields[2].parse().ok()?,
            to_branch: decode_version_name(fields[3])?,
            to: fields[4].parse().ok()?,
            undo: records(fields[5])?,
            redo: records(fields[6])?,
        };
        if intent.from < 0
            || intent.to < 0
            || intent.from_branch.trim().is_empty()
            || intent.to_branch.trim().is_empty()
            || intent.encode() != marker
        {
            return None;
        }
        Some(intent)
    }
}

impl<B: NamedTypeLinks> GenericVersionControlDecorator<B> {
    pub(super) fn start_movement(&mut self, target_path: Vec<i64>, new_branch: &str) -> Result<()> {
        let to = target_path.last().copied().unwrap_or(0);
        if new_branch == self.current_branch && to == self.current_applied {
            return Ok(());
        }
        let current_path: Vec<_> = self
            .build_branch_seqs(&self.current_branch)
            .into_iter()
            .filter(|s| *s <= self.current_applied)
            .collect();
        let common = current_path
            .iter()
            .zip(&target_path)
            .take_while(|(a, b)| a == b)
            .count();
        let available: HashMap<_, _> = self
            .transactions
            .log()
            .into_iter()
            .map(|transition| (transition.sequence, transition))
            .collect();
        let required = |seq: &i64| {
            available.get(seq).copied().ok_or_else(|| {
                anyhow::anyhow!(
            "Transition {seq} is no longer retained; checkout requires complete changed history.")
            })
        };
        // Resolve all changed history before claiming ownership or mutating rows.
        let undo = current_path[common..]
            .iter()
            .rev()
            .map(required)
            .collect::<Result<_>>()?;
        let redo = target_path[common..]
            .iter()
            .map(required)
            .collect::<Result<_>>()?;
        self.active_movement = Some(MovementState {
            intent: MovementIntent {
                from_branch: self.current_branch.clone(),
                from: self.current_applied,
                to_branch: new_branch.into(),
                to,
                undo,
                redo,
            },
            link: 0,
            prepared: false,
            rows_flushed: false,
            published: false,
        });
        self.finish_movement()
    }

    pub(super) fn finish_movement(&mut self) -> Result<()> {
        let Some(mut state) = self.active_movement.clone() else {
            return Ok(());
        };
        if !state.prepared {
            self.transactions.begin_version_replay()?;
            let marker = state.intent.encode();
            if state.link == 0 || !self.branches_store.exists(state.link) {
                state.link = match self.branches_store.get_by_name(&marker)? {
                    Some(link) => link,
                    None => self.branches_store.try_create(0, 0)?,
                };
                self.active_movement.as_mut().unwrap().link = state.link;
            }
            self.branches_store.set_name(state.link, &marker)?;
            self.branches_store.save()?;
            self.active_movement.as_mut().unwrap().prepared = true;
            self.trace("Prepared durable version movement.");
        }
        if !state.rows_flushed {
            for record in &state.intent.undo {
                self.transactions.replay_version_transition(record, true)?;
            }
            for record in &state.intent.redo {
                self.transactions.replay_version_transition(record, false)?;
            }
            self.transactions.flush_version_replay()?;
            self.active_movement.as_mut().unwrap().rows_flushed = true;
        }
        if !state.published {
            self.set_current_branch(&state.intent.to_branch)?;
            self.set_applied(state.intent.to)?;
            self.branches_store.save()?;
            self.current_branch = state.intent.to_branch.clone();
            self.current_applied = state.intent.to;
            self.active_movement.as_mut().unwrap().published = true;
        }
        self.clear_publication_intent(state.link)?;
        self.transactions.finish_version_replay();
        self.active_movement = None;
        self.trace("Completed durable version movement.");
        Ok(())
    }

    pub(super) fn recover_movements(&mut self, pending: Vec<(u32, MovementIntent)>) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        if pending.len() != 1 {
            bail!("Multiple version movement intents.");
        }
        let (link, intent) = pending.into_iter().next().unwrap();
        self.validate_movement_ancestry(&intent.from_branch)?;
        self.validate_movement_ancestry(&intent.to_branch)?;
        let from_path = self.build_branch_seqs(&intent.from_branch);
        let to_path = self.build_branch_seqs(&intent.to_branch);
        let reachable = |path: &[i64], seq| seq == 0 || path.contains(&seq);
        if !self.branches.contains_key(&intent.from_branch)
            || !self.branches.contains_key(&intent.to_branch)
            || !reachable(&from_path, intent.from)
            || !reachable(&to_path, intent.to)
            || (self.current_branch != intent.from_branch
                && self.current_branch != intent.to_branch)
            || (self.current_applied != intent.from && self.current_applied != intent.to)
            || (intent.from_branch != intent.to_branch
                && intent.from != intent.to
                && self.current_branch == intent.from_branch
                && self.current_applied == intent.to)
        {
            bail!("Version movement intent contradicts selected history.");
        }
        let from: Vec<_> = from_path
            .into_iter()
            .filter(|s| *s <= intent.from)
            .collect();
        let to: Vec<_> = to_path.into_iter().filter(|s| *s <= intent.to).collect();
        let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
        if !intent
            .undo
            .iter()
            .map(|t| t.sequence)
            .eq(from[common..].iter().rev().copied())
            || !intent
                .redo
                .iter()
                .map(|t| t.sequence)
                .eq(to[common..].iter().copied())
        {
            bail!("Version movement intent contradicts its transition paths.");
        }
        let available: HashMap<_, _> = self
            .transactions
            .log()
            .into_iter()
            .map(|t| (t.sequence, t))
            .collect();
        for record in intent.undo.iter().chain(&intent.redo) {
            if !self
                .transactions
                .is_transaction_committed(record.transaction_id)
                || self
                    .transactions
                    .is_transaction_rolled_back(record.transaction_id)
                || available
                    .get(&record.sequence)
                    .is_some_and(|stored| stored != record)
            {
                bail!("Version movement intent contradicts its committed records.");
            }
        }
        self.active_movement = Some(MovementState {
            intent,
            link,
            prepared: true,
            rows_flushed: false,
            published: false,
        });
        self.transactions.begin_version_replay()?;
        self.trace("Recovering interrupted version movement.");
        self.finish_movement()
    }

    fn validate_movement_ancestry(&self, branch: &str) -> Result<()> {
        let mut visited = std::collections::HashSet::new();
        let mut current = Some(branch);
        // Reject broken chains before build_branch_seqs follows them.
        while let Some(name) = current {
            let info = self
                .branches
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("Version movement has invalid branch ancestry."))?;
            if !visited.insert(name) || info.fork_seq < 0 || info.head < info.fork_seq {
                bail!("Version movement has invalid branch ancestry.");
            }
            current = info.parent.as_deref();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_movement_vectors_preserve_exact_paths_and_payloads() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/protocol/version-movement-vectors.json"
        ))
        .unwrap();
        for row in vectors["movements"].as_array().unwrap() {
            let marker = row["marker"].as_str().unwrap();
            let intent = MovementIntent::decode(marker).unwrap();
            assert_eq!(intent.from_branch, row["fromBranch"].as_str().unwrap());
            assert_eq!(intent.from, row["fromSequence"].as_i64().unwrap());
            assert_eq!(intent.to_branch, row["toBranch"].as_str().unwrap());
            assert_eq!(intent.to, row["toSequence"].as_i64().unwrap());
            for (actual, field) in [(&intent.undo, "undo"), (&intent.redo, "redo")] {
                assert_eq!(
                    actual.iter().map(|t| t.serialize()).collect::<Vec<_>>(),
                    row[field]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap())
                        .collect::<Vec<_>>()
                );
            }
            assert_eq!(intent.encode(), marker);
        }
        for row in vectors["invalidMovements"]
            .as_array()
            .unwrap()
            .iter()
            .chain(vectors["nonUnicodeMovements"].as_array().unwrap())
        {
            assert!(
                MovementIntent::decode(row.as_str().unwrap()).is_none(),
                "{row}"
            );
        }
    }
}
