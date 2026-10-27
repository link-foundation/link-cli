use super::invalid;
use crate::{engine::paged::PageDevice, LinkError};
use std::sync::{Arc, Mutex};

// The namespace retains an observable lease even when a consuming table
// constructor fails. Send is preserved exactly when the selected device is Send.
pub(super) struct Lease<D> {
    pub device: Arc<Mutex<D>>,
    pub deferred: bool,
}
impl<D> Clone for Lease<D> {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            deferred: self.deferred,
        }
    }
}
impl<D> Lease<D> {
    pub fn new(device: D) -> Self {
        Self {
            device: Arc::new(Mutex::new(device)),
            deferred: false,
        }
    }
    fn use_device<T>(
        &self,
        action: impl FnOnce(&mut D) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        action(
            &mut *self
                .device
                .lock()
                .map_err(|_| invalid("table lease mutex poisoned"))?,
        )
    }
}
impl<D: PageDevice> PageDevice for Lease<D> {
    fn metadata_flags(&self) -> u32 {
        self.device
            .lock()
            .map(|d| d.metadata_flags())
            .unwrap_or(u32::MAX)
    }
    fn supports_durable_flush(&self) -> bool {
        self.device
            .lock()
            .map(|d| d.supports_durable_flush())
            .unwrap_or(false)
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        if self.deferred {
            Ok(())
        } else {
            self.use_device(PageDevice::complete_recovery)
        }
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.use_device(PageDevice::new_database_id)
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.use_device(PageDevice::data_length)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.use_device(|d| d.read_metadata(slot))
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.use_device(|d| d.write_metadata(slot, bytes))
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.use_device(|d| d.sync_metadata(slot))
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; 4096], LinkError> {
        self.use_device(|d| d.read_page(index))
    }
    fn read_checked_page(&mut self, index: u64, expected: u64) -> Result<[u8; 4096], LinkError> {
        self.use_device(|d| d.read_checked_page(index, expected))
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; 4096]) -> Result<(), LinkError> {
        self.use_device(|d| d.write_page(index, bytes))
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.use_device(PageDevice::sync_data)
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.use_device(|d| d.truncate_data(length))
    }
}
