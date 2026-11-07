//! Optional two-copy repair; never repairs before the complete prefix validates.
use super::{
    format::{invalid, page_hash, u64_at, Metadata},
    PageDevice, RepairPageDevice, MAX_PAGES, PAGE_SIZE,
};
use crate::LinkError;
use std::collections::BTreeMap;
struct Repair {
    expected: u64,
    source: usize,
    destination: usize,
}
struct MetaRepair {
    bytes: [u8; 128],
    sides: Vec<usize>,
}
/// Owns two exclusive repair-capable devices. Extraction does not flush records.
/// Both data syncs precede any metadata write; both decision syncs precede ACK.
pub struct MirroredPageDevice<D> {
    devices: [D; 2],
    pages: BTreeMap<u64, Repair>,
    metadata: BTreeMap<usize, MetaRepair>,
    fenced: bool,
}
fn checked(page: &[u8; PAGE_SIZE], expected: u64) -> bool {
    page_hash(page) == expected && u64_at(page, 56) == expected
}
impl<D: RepairPageDevice> MirroredPageDevice<D> {
    pub fn new(primary: D, secondary: D) -> Self {
        Self {
            devices: [primary, secondary],
            pages: BTreeMap::new(),
            metadata: BTreeMap::new(),
            fenced: false,
        }
    }
    pub fn into_devices(self) -> [D; 2] {
        self.devices
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.fenced {
            Err(invalid(
                "uncertain mirror owner is fenced; reopen both devices",
            ))
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
            for d in &mut s.devices {
                action(d)?;
            }
            Ok(())
        })
    }
}
impl<D: RepairPageDevice> PageDevice for MirroredPageDevice<D> {
    fn metadata_flags(&self) -> u32 {
        1
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
        let mut length = 0;
        for d in &mut self.devices {
            let n = d.data_length()?;
            if n > MAX_PAGES * PAGE_SIZE as u64 {
                return Err(invalid("mirror physical length"));
            }
            length = length.max(n);
        }
        Ok(length)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.check()?;
        if slot > 1 {
            return Err(invalid("mirror metadata slot"));
        }
        let raw = self
            .devices
            .each_mut()
            .map(|d| d.read_metadata(slot).unwrap_or_else(|_| Some(vec![0])));
        let states = [
            Metadata::decode(raw[0].as_deref().unwrap_or_default(), 1)?,
            Metadata::decode(raw[1].as_deref().unwrap_or_default(), 1)?,
        ];
        if let [Some(a), Some(b)] = &states {
            if a.identity != b.identity || (a.generation == b.generation && raw[0] != raw[1]) {
                return Err(invalid("conflicting mirror metadata copies"));
            }
        }
        let side = match &states {
            [None, None] => {
                if raw.iter().flatten().any(|b| !b.is_empty()) {
                    return Err(invalid("both mirror metadata copies are unreadable"));
                }
                return Ok(None);
            }
            [Some(_), None] => 0,
            [None, Some(_)] => 1,
            [Some(a), Some(b)] => usize::from(b.generation >= a.generation),
        };
        let bytes = raw[side].as_ref().expect("decoded metadata").clone();
        let sides = (0..2)
            .filter(|&i| raw[i].as_ref() != Some(&bytes))
            .collect();
        self.metadata.insert(
            slot,
            MetaRepair {
                bytes: bytes.clone().try_into().expect("fixed metadata"),
                sides,
            },
        );
        Ok(Some(bytes))
    }
    fn read_page(&mut self, _index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        Err(invalid("expected page checksum is required for mirror"))
    }
    fn read_checked_page(
        &mut self,
        index: u64,
        expected: u64,
    ) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.check()?;
        let copies = self.devices.each_mut().map(|d| d.read_page(index).ok());
        let valid = copies
            .each_ref()
            .map(|p| p.as_ref().is_some_and(|p| checked(p, expected)));
        if !valid.iter().any(|v| *v) {
            return Err(invalid(
                "neither mirror page copy matches expected checksum",
            ));
        }
        if valid == [true, true] && copies[0] != copies[1] {
            return Err(invalid("conflicting mirror page copies"));
        }
        let source = usize::from(!valid[0]);
        if valid != [true, true] {
            self.pages.insert(
                index,
                Repair {
                    expected,
                    source,
                    destination: 1 - source,
                },
            );
        }
        Ok(copies[source].expect("validated page"))
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.mutate(|s| {
            for (&index, repair) in &s.pages {
                let page = s.devices[repair.source].read_page(index)?;
                if !checked(&page, repair.expected) {
                    return Err(invalid("mirror repair source changed"));
                }
                s.devices[repair.destination].repair_page(index, &page)?;
            }
            if !s.pages.is_empty() {
                for d in &mut s.devices {
                    d.sync_data()?;
                }
            }
            for (&slot, repair) in &s.metadata {
                for &side in &repair.sides {
                    s.devices[side].write_metadata(slot, &repair.bytes)?;
                    s.devices[side].sync_metadata(slot)?;
                }
            }
            s.pages.clear();
            s.metadata.clear();
            Ok(())
        })
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.both(|d| d.write_page(index, bytes))
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.both(PageDevice::sync_data)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.both(|d| d.write_metadata(slot, bytes))
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.both(|d| d.sync_metadata(slot))
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.both(|d| d.truncate_data(length))
    }
}
#[cfg(all(feature = "native-pages", any(unix, windows)))]
impl MirroredPageDevice<super::FilePageDevice> {
    /// Acquires both native `.writer` files, never stealing existing ownership.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, LinkError> {
        let path = path.as_ref();
        let mut secondary = path.as_os_str().to_owned();
        secondary.push(".mirror");
        let mut primary = super::FilePageDevice::open(path)?;
        match super::FilePageDevice::open(std::path::Path::new(&secondary)) {
            Ok(second) => Ok(Self::new(primary, second)),
            Err(error) => {
                if let Err(cleanup) = primary.close() {
                    return Err(invalid(&format!(
                        "{error}; mirror cleanup failed: {cleanup}"
                    )));
                }
                Err(error)
            }
        }
    }
    /// Attempts both closes; uncertain underlying ownership files remain fenced.
    pub fn close(&mut self) -> Result<(), LinkError> {
        self.close_with(super::FilePageDevice::close)
    }
    fn close_with(
        &mut self,
        mut close: impl FnMut(&mut super::FilePageDevice) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.fenced = true;
        let failures: Vec<_> = self
            .devices
            .iter_mut()
            .rev()
            .filter_map(|d| close(d).err().map(|e| e.to_string()))
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(invalid(&format!(
                "mirror cleanup failed: {}",
                failures.join("; ")
            )))
        }
    }
}

#[cfg(all(test, feature = "native-pages", any(unix, windows)))]
mod tests {
    use super::*;
    #[test]
    fn both_close_errors_preserve_fences_and_attempt_all_handles_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut pair = MirroredPageDevice::open(&path).unwrap();
        let mut closed = 0;
        let error = pair
            .close_with(|device| {
                device.close_with(|file| {
                    drop(file);
                    closed += 1;
                    Err(std::io::Error::other("injected close"))
                })
            })
            .unwrap_err();
        assert_eq!(closed, 8);
        assert_eq!(error.to_string().matches("injected close").count(), 8);
        assert!(dir.path().join("journal.writer").exists());
        assert!(dir.path().join("journal.mirror.writer").exists());
        assert!(MirroredPageDevice::open(&path).is_err());
        assert!(pair.sync_data().is_err());
        pair.close().unwrap();
        assert!(dir.path().join("journal.writer").exists());
        assert!(dir.path().join("journal.mirror.writer").exists());
    }
}
