//! Selected redundancy for checkpoint decisions, independent of page-bank mode.
use super::checkpoint_format::Decision;
use super::format::invalid;
use super::{PageDevice, PAGE_SIZE};
use crate::LinkError;
use std::collections::BTreeMap;

struct Repair {
    bytes: [u8; 128],
    source: usize,
    destinations: Vec<usize>,
}
/// Owns two exclusive raw root devices. No repair runs during selection; the
/// checkpoint log validates its selected bank before completing root recovery.
/// Both decision copies must synchronize before publication is acknowledged.
/// Extracting the devices never flushes or repairs them.
pub struct MirroredCheckpointRoot<D> {
    devices: [D; 2],
    flags: u32,
    repairs: BTreeMap<usize, Repair>,
    fenced: bool,
}
impl<D: PageDevice> MirroredCheckpointRoot<D> {
    /// `bank_flags` is zero for raw banks or one for mirrored page banks.
    pub fn new(primary: D, secondary: D, bank_flags: u32) -> Result<Self, LinkError> {
        if bank_flags > 1 || primary.metadata_flags() != 0 || secondary.metadata_flags() != 0 {
            return Err(invalid(
                "checkpoint root mirroring requires raw roots and bank mode 0 or 1",
            ));
        }
        Ok(Self {
            devices: [primary, secondary],
            flags: 2 | bank_flags,
            repairs: BTreeMap::new(),
            fenced: false,
        })
    }
    pub fn into_devices(self) -> [D; 2] {
        self.devices
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.fenced {
            Err(invalid(
                "uncertain checkpoint root is fenced; reopen both devices",
            ))
        } else {
            Ok(())
        }
    }
    fn slot(&self, slot: usize) -> Result<(), LinkError> {
        self.check()?;
        if slot > 1 {
            Err(invalid("checkpoint root metadata slot"))
        } else {
            Ok(())
        }
    }
    fn mutate(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.check()?;
        let result = action(self);
        if result.is_err() {
            self.fenced = true;
        }
        result
    }
    fn both(
        &mut self,
        mut action: impl FnMut(&mut D) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.mutate(|s| {
            for device in &mut s.devices {
                action(device)?;
            }
            Ok(())
        })
    }
}
impl<D: PageDevice> PageDevice for MirroredCheckpointRoot<D> {
    fn metadata_flags(&self) -> u32 {
        self.flags
    }
    fn supports_durable_flush(&self) -> bool {
        self.devices.iter().all(PageDevice::supports_durable_flush)
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.check()?;
        self.devices[0].new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.check()?;
        for device in &mut self.devices {
            if device.data_length()? != 0 {
                return Err(invalid("checkpoint root data must be empty"));
            }
        }
        Ok(0)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.slot(slot)?;
        let raw = self
            .devices
            .each_mut()
            .map(|device| device.read_metadata(slot).unwrap_or_else(|_| Some(vec![0])));
        let states = [
            Decision::decode(raw[0].as_deref().unwrap_or_default(), slot, self.flags)?,
            Decision::decode(raw[1].as_deref().unwrap_or_default(), slot, self.flags)?,
        ];
        if let [Some(a), Some(b)] = &states {
            if a.identity != b.identity || a.epoch.abs_diff(b.epoch) > 2 {
                return Err(invalid("conflicting checkpoint root copies"));
            }
        }
        let source = match &states {
            [None, None] => {
                if raw.iter().flatten().any(|bytes| !bytes.is_empty()) {
                    return Err(invalid("both checkpoint root copies are unreadable"));
                }
                self.repairs.remove(&slot);
                return Ok(None);
            }
            [Some(_), None] => 0,
            [None, Some(_)] => 1,
            [Some(a), Some(b)] => usize::from(b.epoch > a.epoch),
        };
        let bytes: [u8; 128] = raw[source]
            .as_deref()
            .expect("valid root bytes")
            .try_into()
            .expect("fixed root size");
        let destinations = (0..2)
            .filter(|&i| raw[i].as_deref() != Some(bytes.as_slice()))
            .collect();
        self.repairs.insert(
            slot,
            Repair {
                bytes,
                source,
                destinations,
            },
        );
        Ok(Some(bytes.to_vec()))
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.mutate(|s| {
            // Validate every selected source before any repair writes.
            for (&slot, repair) in &s.repairs {
                if !repair.destinations.is_empty()
                    && s.devices[repair.source].read_metadata(slot)?.as_deref()
                        != Some(repair.bytes.as_slice())
                {
                    return Err(invalid("checkpoint root repair source changed"));
                }
            }
            for (&slot, repair) in &s.repairs {
                for &side in &repair.destinations {
                    s.devices[side].write_metadata(slot, &repair.bytes)?;
                    s.devices[side].sync_metadata(slot)?;
                }
            }
            s.repairs.clear();
            Ok(())
        })
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.slot(slot)?;
        if Decision::decode(bytes, slot, self.flags)?.is_none() {
            return Err(invalid("invalid checkpoint root publication"));
        }
        self.both(|device| device.write_metadata(slot, bytes))?;
        self.repairs.remove(&slot);
        Ok(())
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.slot(slot)?;
        self.both(|device| device.sync_metadata(slot))
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.both(PageDevice::sync_data)
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.check()?;
        if length != 0 {
            return Err(invalid("checkpoint root data must be empty"));
        }
        self.both(|device| device.truncate_data(0))
    }
    fn read_page(&mut self, _index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.check()?;
        Err(invalid("checkpoint root has no data pages"))
    }
    fn write_page(&mut self, _index: u64, _bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.check()?;
        Err(invalid("checkpoint root has no data pages"))
    }
}
