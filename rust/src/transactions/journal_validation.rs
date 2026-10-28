//! Prepare a validated journal without changing live history or storage.
use std::collections::{HashMap, HashSet};

use doublets::data::LinkReference;

use super::types::{
    APPLIED_MARKER_PREFIX, COMMIT_MARKER_PREFIX, ROLLBACK_MARKER_PREFIX, TRANSITION_NAME_PREFIX,
};
use super::undo_image::UndoImage;
use super::JournalRecord;
use crate::error::LinkError;

pub(super) struct RecoveryJournal<T> {
    pub(super) log: Vec<JournalRecord<T>>,
    pub(super) committed: HashSet<u128>,
    pub(super) rolled_back: HashSet<u128>,
    pub(super) applied: HashSet<i64>,
    pub(super) sequence: i64,
    pub(super) applied_sequence: i64,
    pub(super) images: Vec<UndoImage<T>>,
}

impl<T: LinkReference> RecoveryJournal<T> {
    pub(super) fn parse(entries: Vec<String>) -> Result<Self, LinkError> {
        let mut transitions = HashMap::new();
        let mut committed = HashSet::new();
        let mut rolled_back = HashSet::new();
        let mut applied = HashSet::new();
        let mut sequence = 0;
        let mut applied_sequence = 0;
        let mut images: Vec<UndoImage<T>> = Vec::new();
        let mut image_owners = HashMap::<u128, usize>::new();
        for entry in entries {
            if let Some(payload) = entry.strip_prefix(TRANSITION_NAME_PREFIX) {
                if payload.starts_with("v3|") {
                    let image = UndoImage::<T>::parse(payload)?;
                    if let Some(index) = image_owners.get(&image.owner) {
                        let previous = &images[*index];
                        if previous != &image {
                            return Err(LinkError::InvalidFormat(
                                "conflicting write-ahead undo images".into(),
                            ));
                        }
                    } else {
                        sequence = sequence.max(image.base);
                        image_owners.insert(image.owner, images.len());
                        images.push(image);
                    }
                    continue;
                }
                // Compare parsed values: identical retries may retain older
                // lexical forms and extra fields accepted by the public codec.
                let transition = JournalRecord::<T>::parse(payload)?;
                if transition.transaction_id == 0 || transition.sequence <= 0 {
                    return Err(LinkError::InvalidFormat(
                        "transaction records require a nonzero owner and positive sequence".into(),
                    ));
                }
                if transitions
                    .get(&transition.sequence)
                    .is_some_and(|previous| *previous != transition)
                {
                    return Err(LinkError::InvalidFormat(format!(
                        "conflicting transaction records for sequence {}",
                        transition.sequence
                    )));
                }
                sequence = sequence.max(transition.sequence);
                transitions.insert(transition.sequence, transition);
            } else if let Some(owner) = entry.strip_prefix(COMMIT_MARKER_PREFIX) {
                let owner = parse_owner(owner, "commit")?;
                if rolled_back.contains(&owner) {
                    return Err(contradiction(owner));
                }
                committed.insert(owner);
            } else if let Some(owner) = entry.strip_prefix(ROLLBACK_MARKER_PREFIX) {
                let owner = parse_owner(owner, "rollback")?;
                if committed.contains(&owner) {
                    return Err(contradiction(owner));
                }
                rolled_back.insert(owner);
            } else if let Some(value) = entry.strip_prefix(APPLIED_MARKER_PREFIX) {
                let seq = value
                    .parse::<i64>()
                    .map_err(|_| LinkError::InvalidFormat("invalid applied marker".into()))?;
                if seq <= 0 {
                    return Err(LinkError::InvalidFormat(
                        "nonpositive applied sequence".into(),
                    ));
                }
                applied.insert(seq);
                applied_sequence = applied_sequence.max(seq);
                // Checkpoints reserve sequences even after payload retention.
                sequence = sequence.max(seq);
            }
        }
        let mut log: Vec<_> = transitions.into_values().collect();
        log.sort_unstable_by_key(|transition| transition.sequence);
        let mut first_sequences = HashMap::new();
        if !images.is_empty() {
            for record in &log {
                first_sequences
                    .entry(record.transaction_id)
                    .or_insert(record.sequence);
            }
        }
        let mut pending_bases = HashSet::new();
        for image in &images {
            if first_sequences
                .get(&image.owner)
                .is_some_and(|sequence| *sequence <= image.base)
            {
                return Err(LinkError::InvalidFormat(
                    "undo image follows its owner's writes".into(),
                ));
            }
            if !committed.contains(&image.owner)
                && !rolled_back.contains(&image.owner)
                && !pending_bases.insert(image.base)
            {
                return Err(LinkError::InvalidFormat(
                    "ambiguous pending undo images at the same base".into(),
                ));
            }
        }
        Ok(Self {
            log,
            committed,
            rolled_back,
            applied,
            sequence,
            applied_sequence,
            images,
        })
    }
}

fn parse_owner(value: &str, kind: &str) -> Result<u128, LinkError> {
    u128::from_str_radix(value, 16)
        .ok()
        .filter(|owner| *owner != 0)
        .ok_or_else(|| LinkError::InvalidFormat(format!("invalid {kind} marker")))
}
fn contradiction(owner: u128) -> LinkError {
    LinkError::InvalidFormat(format!(
        "transaction {owner:032x} has both commit and rollback decisions"
    ))
}
