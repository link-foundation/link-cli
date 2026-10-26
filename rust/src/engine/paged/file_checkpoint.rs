//! Selected native ownership of the root and both fixed checkpoint banks.
use super::{
    format::invalid, CheckpointPagedLog, FilePageDevice, MirroredCheckpointRoot,
    MirroredPageDevice, PageDevice, RepairPageDevice, MAX_PAGES, PAGE_SIZE,
};
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

// Retain independent raw owners across fallible portable construction. Dropping
// a partially constructed bank must not hide its explicit cleanup errors.
#[derive(Clone)]
struct Lease(Arc<Mutex<FilePageDevice>>);
impl Lease {
    fn open(path: &Path) -> Result<Self, LinkError> {
        Ok(Self(Arc::new(Mutex::new(FilePageDevice::open(path)?))))
    }
    fn io<T>(
        &self,
        action: impl FnOnce(&mut FilePageDevice) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        let mut device = self
            .0
            .lock()
            .map_err(|_| invalid("native checkpoint device mutex is poisoned"))?;
        action(&mut device)
    }
}
impl PageDevice for Lease {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.io(PageDevice::new_database_id)
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.io(PageDevice::data_length)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.io(|device| device.read_metadata(slot))
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.io(|device| device.write_metadata(slot, bytes))
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.io(|device| device.sync_metadata(slot))
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.io(|device| device.read_page(index))
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.io(|device| device.write_page(index, bytes))
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.io(PageDevice::sync_data)
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.io(|device| device.truncate_data(length))
    }
}
impl RepairPageDevice for Lease {
    fn repair_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.io(|device| device.repair_page(index, bytes))
    }
}

enum NativeDevice {
    Single(Lease),
    Mirrored(Box<MirroredPageDevice<Lease>>),
    Root(Box<MirroredCheckpointRoot<Lease>>),
}
impl NativeDevice {
    fn device(&mut self) -> &mut dyn PageDevice {
        match self {
            Self::Single(device) => device,
            Self::Mirrored(device) => &mut **device,
            Self::Root(device) => &mut **device,
        }
    }
}
impl PageDevice for NativeDevice {
    fn metadata_flags(&self) -> u32 {
        match self {
            Self::Single(_) => 0,
            Self::Mirrored(_) => 1,
            Self::Root(device) => device.metadata_flags(),
        }
    }
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.device().new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.device().data_length()
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.device().read_metadata(slot)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.device().write_metadata(slot, bytes)
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.device().sync_metadata(slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.device().read_page(index)
    }
    fn read_checked_page(
        &mut self,
        index: u64,
        expected: u64,
    ) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.device().read_checked_page(index, expected)
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        self.device().complete_recovery()
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.device().write_page(index, bytes)
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.device().sync_data()
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.device().truncate_data(length)
    }
}

/// Independently selected root and bank copies. Defaults retain the existing
/// single-root, single-bank mode and its file/I/O costs.
#[derive(Clone, Copy, Debug)]
pub struct FileCheckpointOptions {
    pub mirrored_banks: bool,
    pub mirrored_root: bool,
    pub max_pages: u64,
}
impl Default for FileCheckpointOptions {
    fn default() -> Self {
        Self {
            mirrored_banks: false,
            mirrored_root: false,
            max_pages: MAX_PAGES,
        }
    }
}

/// Owns native devices at `path`, `.bank0` and `.bank1`, with independently
/// selected `.mirror` copies of the root and banks. Acquires every writer fence
/// before validation or repair.
/// Close never flushes pending records; it attempts every device in reverse
/// order, root last. Uncertain cleanup retains the affected native writer fence.
pub struct FileCheckpointPagedLog {
    inner: Option<CheckpointPagedLog<NativeDevice, NativeDevice>>,
    owners: Vec<Lease>,
}
fn suffix(path: &Path, extension: &str) -> PathBuf {
    let mut result = path.as_os_str().to_owned();
    result.push(extension);
    result.into()
}
impl FileCheckpointPagedLog {
    pub fn with_options(
        path: impl AsRef<Path>,
        options: FileCheckpointOptions,
    ) -> Result<Self, LinkError> {
        Self::build_selected(path.as_ref(), options, Lease::open, |_, device| {
            device.close()
        })
    }
    pub fn open(path: impl AsRef<Path>, mirrored_banks: bool) -> Result<Self, LinkError> {
        Self::with_max_pages(path, mirrored_banks, MAX_PAGES)
    }
    pub fn with_max_pages(
        path: impl AsRef<Path>,
        mirrored_banks: bool,
        max_pages: u64,
    ) -> Result<Self, LinkError> {
        Self::build(
            path.as_ref(),
            mirrored_banks,
            max_pages,
            Lease::open,
            |_, device| device.close(),
        )
    }
    fn build(
        path: &Path,
        mirrored: bool,
        max_pages: u64,
        open: impl FnMut(&Path) -> Result<Lease, LinkError>,
        close: impl FnMut(usize, &mut FilePageDevice) -> Result<(), LinkError>,
    ) -> Result<Self, LinkError> {
        Self::build_selected(
            path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                max_pages,
                ..Default::default()
            },
            open,
            close,
        )
    }
    fn build_selected(
        path: &Path,
        options: FileCheckpointOptions,
        mut open: impl FnMut(&Path) -> Result<Lease, LinkError>,
        close: impl FnMut(usize, &mut FilePageDevice) -> Result<(), LinkError>,
    ) -> Result<Self, LinkError> {
        let mut owners = Vec::new();
        let result = (|| {
            let mut acquire = |name: &Path| {
                let lease = open(name)?;
                owners.push(lease.clone());
                Ok::<_, LinkError>(lease)
            };
            let primary_root = acquire(path)?;
            let secondary_root = if options.mirrored_root {
                Some(acquire(&suffix(path, ".mirror"))?)
            } else {
                None
            };
            let mut bank = |index| {
                let name = suffix(path, &format!(".bank{index}"));
                let primary = acquire(&name)?;
                Ok::<_, LinkError>(if options.mirrored_banks {
                    NativeDevice::Mirrored(Box::new(MirroredPageDevice::new(
                        primary,
                        acquire(&suffix(&name, ".mirror"))?,
                    )))
                } else {
                    NativeDevice::Single(primary)
                })
            };
            let banks = [bank(0)?, bank(1)?];
            let root = match secondary_root {
                Some(secondary) => NativeDevice::Root(Box::new(MirroredCheckpointRoot::new(
                    primary_root,
                    secondary,
                    u32::from(options.mirrored_banks),
                )?)),
                None => NativeDevice::Single(primary_root),
            };
            CheckpointPagedLog::with_max_pages(root, banks, options.max_pages)
        })();
        match result {
            Ok(inner) => Ok(Self {
                inner: Some(inner),
                owners,
            }),
            Err(cause) => match Self::close_owned(&mut owners, close) {
                Ok(()) => Err(cause),
                Err(cleanup) => Err(invalid(&format!(
                    "{cause}; native checkpoint initialization cleanup failed: {cleanup}"
                ))),
            },
        }
    }
    fn inner(&mut self) -> Result<&mut CheckpointPagedLog<NativeDevice, NativeDevice>, LinkError> {
        self.inner
            .as_mut()
            .ok_or_else(|| invalid("native checkpoint log is closed"))
    }
    pub fn epoch(&self) -> Result<u64, LinkError> {
        self.inner
            .as_ref()
            .map(CheckpointPagedLog::epoch)
            .ok_or_else(|| invalid("native checkpoint log is closed"))
    }
    fn close_owned(
        owners: &mut Vec<Lease>,
        mut close: impl FnMut(usize, &mut FilePageDevice) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        let mut errors = Vec::new();
        for (index, owner) in owners.drain(..).enumerate().rev() {
            // Recovery of a poisoned guard is only for closing every handle.
            let mut device = owner.0.lock().unwrap_or_else(|cause| cause.into_inner());
            if let Err(cause) = close(index, &mut device) {
                errors.push(cause.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(invalid(&format!(
                "native checkpoint cleanup failed: {}",
                errors.join("; ")
            )))
        }
    }
    fn close_with(
        &mut self,
        close: impl FnMut(usize, &mut FilePageDevice) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.inner = None;
        Self::close_owned(&mut self.owners, close)
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        self.close_with(|_, device| device.close())
    }
}
impl Drop for FileCheckpointPagedLog {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl TransitionLogStore for FileCheckpointPagedLog {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner()?.read_log_entries()
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.inner()?.append_log_entry(entry)
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.inner()?.flush_log()
    }
}
impl ReplaceableTransitionLogStore for FileCheckpointPagedLog {
    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        self.inner()?.replace_log_entries(entries)
    }
}

#[cfg(test)]
#[path = "file_checkpoint_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "file_checkpoint_process_tests.rs"]
mod process_tests;
