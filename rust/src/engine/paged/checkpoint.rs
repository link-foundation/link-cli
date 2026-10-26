//! Two fixed banks: prepare a complete inactive image, then publish one root.
use super::checkpoint_format::{select, Decision};
use super::format::{invalid, Metadata, CELLS};
use super::{PageDevice, PagedLog, MAX_PAGES, PAGE_SIZE};
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};

/// Optional atomic journal replacement over three exclusive device values.
/// Root data is unused; its two metadata slots select one complete page bank.
/// Bank identities bind each reuse to a distinct monotone root epoch. Neither
/// extraction nor destruction flushes pending records. A failed I/O fences this
/// owner; reopen all devices to discover its actual durable state.
/// Destruction drops both banks before the root, following device destructors.
pub struct CheckpointPagedLog<R, D> {
    active: PagedLog<D>,
    inactive: Option<D>,
    decision: Decision,
    fenced: bool,
    root: R, // Rust drops fields in declaration order: release root last.
}
impl<R: PageDevice, D: PageDevice> CheckpointPagedLog<R, D> {
    pub fn open(root: R, banks: [D; 2]) -> Result<Self, LinkError> {
        Self::with_max_pages(root, banks, MAX_PAGES)
    }
    pub fn with_max_pages(
        mut root: R,
        mut banks: [D; 2],
        max_pages: u64,
    ) -> Result<Self, LinkError> {
        if max_pages == 0 || max_pages > MAX_PAGES {
            return Err(invalid("invalid checkpoint page limit"));
        }
        let flags = banks[0].metadata_flags();
        let root_flags = root.metadata_flags();
        if (root_flags != 0 && root_flags != (2 | flags))
            || flags > 1
            || banks[1].metadata_flags() != flags
        {
            return Err(invalid(
                "checkpoint requires matching root and bank modes (flags)",
            ));
        }
        if root.data_length()? != 0 {
            return Err(invalid("checkpoint root data must be empty"));
        }
        let raw = [root.read_metadata(0)?, root.read_metadata(1)?];
        let decision = if let Some(decision) = select(&raw, root_flags | flags)? {
            let bank = &mut banks[decision.bank()];
            let mut found = false;
            for slot in 0..2 {
                if let Some(state) = Metadata::decode(
                    bank.read_metadata(slot)?.as_deref().unwrap_or_default(),
                    flags,
                )? {
                    if state.identity != decision.bank_id() {
                        return Err(invalid("checkpoint bank epoch identity changed"));
                    }
                    found = true;
                }
            }
            if !found {
                return Err(invalid("checkpoint selected bank has no valid metadata"));
            }
            decision
        } else {
            // With no decision, only empty/partially initialized banks can be
            // initialized. Never erase an unrooted nonempty journal.
            for bank in &mut banks {
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
            let decision = Decision {
                identity: root.new_database_id()?,
                epoch: 0,
                flags: root_flags | flags,
            };
            for (i, bank) in banks.iter_mut().enumerate() {
                let initial = Decision {
                    epoch: i as u64,
                    ..decision.clone()
                };
                reset(bank, initial.bank_id(), flags)?;
            }
            root.sync_data()?;
            root.write_metadata(0, &decision.encode())?;
            root.sync_metadata(0)?;
            decision
        };
        let [first, second] = banks;
        let (chosen, inactive) = if decision.bank() == 0 {
            (first, second)
        } else {
            (second, first)
        };
        let active = PagedLog::with_max_pages(chosen, max_pages)?;
        if root_flags & 2 != 0 {
            root.complete_recovery()?;
        }
        Ok(Self {
            active,
            inactive: Some(inactive),
            decision,
            fenced: false,
            root, // Keep root local until all fallible bank opening succeeds.
        })
    }
    pub fn epoch(&self) -> u64 {
        self.decision.epoch
    }
    pub fn into_devices(self) -> (R, [D; 2]) {
        let active = self.active.into_device();
        let inactive = self.inactive.expect("inactive device retained");
        (
            self.root,
            if self.decision.bank() == 0 {
                [active, inactive]
            } else {
                [inactive, active]
            },
        )
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.fenced {
            Err(invalid(
                "uncertain checkpoint page owner is fenced; reopen all devices",
            ))
        } else {
            Ok(())
        }
    }
}
fn reset<D: PageDevice>(device: &mut D, identity: [u8; 16], flags: u32) -> Result<(), LinkError> {
    device.truncate_data(0)?;
    device.sync_data()?;
    let empty = Metadata::empty(identity, flags).encode();
    for slot in 0..2 {
        device.write_metadata(slot, &empty)?;
        device.sync_metadata(slot)?;
    }
    Ok(())
}
impl<R: PageDevice, D: PageDevice> TransitionLogStore for CheckpointPagedLog<R, D> {
    fn supports_durable_flush(&self) -> bool {
        self.root.supports_durable_flush()
            && self.active.supports_durable_flush()
            && self
                .inactive
                .as_ref()
                .is_some_and(PageDevice::supports_durable_flush)
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.check()?;
        self.active.append_log_entry(entry)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.check()?;
        let result = self.active.read_log_entries();
        if result.is_err() {
            self.fenced = true;
        }
        result
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.check()?;
        let result = self.active.flush_log();
        if result.is_err() {
            self.fenced = true;
        }
        result
    }
}
impl<R: PageDevice, D: PageDevice> ReplaceableTransitionLogStore for CheckpointPagedLog<R, D> {
    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        self.check()?;
        if !self.active.pending.is_empty() {
            return Err(invalid(
                "checkpoint replacement rejects pending physical records",
            ));
        }
        let mut rows = 0_u64;
        for entry in entries {
            if entry.contains(['\r', '\n']) {
                return Err(invalid("checkpoint records cannot contain line breaks"));
            }
            rows = rows
                .checked_add(entry.len().div_ceil(7) as u64 + 1)
                .ok_or_else(|| invalid("checkpoint capacity overflow"))?;
        }
        if rows.div_ceil(CELLS) > self.active.max_pages {
            return Err(invalid("checkpoint exceeds selected page limit"));
        }
        if self.read_log_entries()? == entries {
            return Ok(false);
        }
        let next = Decision {
            epoch: self
                .decision
                .epoch
                .checked_add(1)
                .ok_or_else(|| invalid("checkpoint epoch exhausted"))?,
            ..self.decision.clone()
        };
        self.fenced = true;
        let mut device = self.inactive.take().expect("inactive device retained");
        let prepared = (|| {
            reset(&mut device, next.bank_id(), next.flags & 1)?;
            let mut log = PagedLog::with_max_pages(Borrowed(&mut device), self.active.max_pages)?;
            for entry in entries {
                log.append_log_entry(entry)?;
            }
            log.flush_log()?;
            Ok::<_, LinkError>(log.state.clone())
        })();
        self.inactive = Some(device); // Preserve ownership even after uncertain preparation.
        let state = prepared?;
        self.root.write_metadata(next.bank(), &next.encode())?;
        self.root.sync_metadata(next.bank())?;
        let new = PagedLog {
            device: self.inactive.take().expect("prepared device retained"),
            state,
            pending: Vec::new(),
            pending_rows: 0,
            fenced: false,
            max_pages: self.active.max_pages,
        };
        let old = std::mem::replace(&mut self.active, new);
        self.inactive = Some(old.into_device());
        self.decision = next;
        self.fenced = false;
        Ok(true)
    }
}

// Preparation borrows the inactive device; every error leaves ownership in the
// enclosing log rather than dropping an uncertain native handle.
struct Borrowed<'a, D>(&'a mut D);
impl<D: PageDevice> PageDevice for Borrowed<'_, D> {
    fn metadata_flags(&self) -> u32 {
        self.0.metadata_flags()
    }
    fn supports_durable_flush(&self) -> bool {
        self.0.supports_durable_flush()
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.0.new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.0.data_length()
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.0.read_metadata(slot)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.0.write_metadata(slot, bytes)
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.0.sync_metadata(slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.0.read_page(index)
    }
    fn read_checked_page(
        &mut self,
        index: u64,
        expected: u64,
    ) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.0.read_checked_page(index, expected)
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.0.complete_recovery()
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.0.write_page(index, bytes)
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.0.sync_data()
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.0.truncate_data(length)
    }
}
