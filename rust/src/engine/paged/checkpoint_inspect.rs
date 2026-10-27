use super::checkpoint_format::select;
use super::format::{invalid, Metadata};
use super::{read_committed, CheckpointPagedLog, PageDevice, MAX_PAGES, PAGE_SIZE};
use crate::LinkError;

impl<R: PageDevice, D: PageDevice> CheckpointPagedLog<R, D> {
    /// Read the complete selected prefix without initialization, truncation,
    /// identity allocation or repair completion. Devices remain borrowed.
    /// Checked mirror reads may stage volatile repairs: discard those wrappers
    /// if higher-level semantic validation fails. Inspecting does not close any
    /// device or change the existing opening constructors' recovery behavior.
    pub fn inspect(
        root: &mut R,
        banks: &mut [D; 2],
        max_pages: u64,
    ) -> Result<Vec<String>, LinkError> {
        if max_pages == 0 || max_pages > MAX_PAGES {
            return Err(invalid("invalid checkpoint inspection page limit"));
        }
        let flags = banks[0].metadata_flags();
        let root_flags = root.metadata_flags();
        if (root_flags != 0 && root_flags != (2 | flags))
            || flags > 1
            || banks[1].metadata_flags() != flags
        {
            return Err(invalid("inspection requires matching root and bank modes"));
        }
        if root.data_length()? != 0 {
            return Err(invalid("checkpoint root data must be empty"));
        }
        let raw = [root.read_metadata(0)?, root.read_metadata(1)?];
        let Some(decision) = select(&raw, root_flags | flags)? else {
            for bank in banks {
                if bank.data_length()? != 0 {
                    return Err(invalid("unrooted checkpoint bank contains data"));
                }
                for slot in 0..2 {
                    if Metadata::decode(
                        bank.read_metadata(slot)?.as_deref().unwrap_or_default(),
                        flags,
                    )?
                    .is_some_and(|state| state.pages != 0)
                    {
                        return Err(invalid(
                            "unrooted checkpoint bank contains committed metadata",
                        ));
                    }
                }
            }
            return Ok(vec![]);
        };
        let bank = &mut banks[decision.bank()];
        let raw = [bank.read_metadata(0)?, bank.read_metadata(1)?];
        let mut valid = Vec::new();
        for bytes in &raw {
            if let Some(state) = Metadata::decode(bytes.as_deref().unwrap_or_default(), flags)? {
                if state.identity != decision.bank_id() {
                    return Err(invalid("checkpoint bank epoch identity changed"));
                }
                valid.push(state);
            }
        }
        if valid.len() == 2 && valid[0].generation == valid[1].generation && raw[0] != raw[1] {
            return Err(invalid("conflicting metadata decisions"));
        }
        let state = valid
            .into_iter()
            .max_by_key(|state| state.generation)
            .ok_or_else(|| invalid("checkpoint selected bank has no valid metadata"))?;
        if state.pages > max_pages {
            return Err(invalid("committed pages exceed selected limit"));
        }
        if bank.data_length()? < state.pages * PAGE_SIZE as u64 {
            return Err(invalid("truncated committed pages"));
        }
        read_committed(bank, &state, true)
    }
}
