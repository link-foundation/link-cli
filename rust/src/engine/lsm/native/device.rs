use super::super::{invalid, lease::Lease};
use super::read_only::ReadOnly;
use crate::engine::paged::{
    FilePageDevice, MirroredCheckpointRoot, MirroredPageDevice, PageDevice, RepairPageDevice,
    PAGE_SIZE,
};
use crate::LinkError;

pub(super) enum Image {
    Read(ReadOnly),
    Write(FilePageDevice),
}
pub(super) struct Raw {
    pub image: Option<Image>,
    failure: Option<String>,
}
impl Raw {
    pub fn new(image: Image) -> Self {
        Self {
            image: Some(image),
            failure: None,
        }
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        if let Some(mut image) = self.image.take() {
            let result = match &mut image {
                Image::Read(d) => d.close(),
                Image::Write(d) => d.close(),
            };
            if let Err(error) = result {
                self.failure = Some(error.to_string());
            }
        }
        match &self.failure {
            Some(error) => Err(invalid(error)),
            None => Ok(()),
        }
    }
    fn view(&mut self) -> Result<&mut dyn RepairPageDevice, LinkError> {
        Ok(
            match self
                .image
                .as_mut()
                .ok_or_else(|| invalid("closed native LSM image"))?
            {
                Image::Read(device) => device,
                Image::Write(device) => device,
            },
        )
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl PageDevice for Raw {
    fn supports_durable_flush(&self) -> bool {
        matches!(self.image, Some(Image::Write(_)))
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.view()?.new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.view()?.data_length()
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.view()?.read_metadata(slot)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.view()?.write_metadata(slot, bytes)
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.view()?.sync_metadata(slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.view()?.read_page(index)
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.view()?.write_page(index, bytes)
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.view()?.sync_data()
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.view()?.truncate_data(length)
    }
}
impl RepairPageDevice for Raw {
    fn repair_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.view()?.repair_page(index, bytes)
    }
}

pub(super) type RawLease = Lease<Raw>;
enum Kind {
    Single(RawLease),
    Mirrored(Box<MirroredPageDevice<RawLease>>),
    Root(Box<MirroredCheckpointRoot<RawLease>>),
}
pub(super) struct Device {
    kind: Kind,
    pub raw: Vec<RawLease>,
    pub deferred: bool,
}
impl Device {
    pub fn new(
        raw: Vec<RawLease>,
        mirrored_root: bool,
        bank_flags: u32,
        deferred: bool,
    ) -> Result<Self, LinkError> {
        let kind = if raw.len() == 1 {
            Kind::Single(raw[0].clone())
        } else if mirrored_root {
            Kind::Root(Box::new(MirroredCheckpointRoot::new(
                raw[0].clone(),
                raw[1].clone(),
                bank_flags,
            )?))
        } else {
            Kind::Mirrored(Box::new(MirroredPageDevice::new(
                raw[0].clone(),
                raw[1].clone(),
            )))
        };
        Ok(Self {
            kind,
            raw,
            deferred,
        })
    }
    fn view(&mut self) -> &mut dyn PageDevice {
        match &mut self.kind {
            Kind::Single(d) => d,
            Kind::Mirrored(d) => &mut **d,
            Kind::Root(d) => &mut **d,
        }
    }
}
impl PageDevice for Device {
    fn metadata_flags(&self) -> u32 {
        match &self.kind {
            Kind::Single(_) => 0,
            Kind::Mirrored(_) => 1,
            Kind::Root(d) => d.metadata_flags(),
        }
    }
    fn supports_durable_flush(&self) -> bool {
        match &self.kind {
            Kind::Single(d) => d.supports_durable_flush(),
            Kind::Mirrored(d) => d.supports_durable_flush(),
            Kind::Root(d) => d.supports_durable_flush(),
        }
    }
    fn complete_recovery(&mut self) -> Result<(), LinkError> {
        if self.deferred {
            Ok(())
        } else {
            self.view().complete_recovery()
        }
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.view().new_database_id()
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.view().data_length()
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.view().read_metadata(slot)
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.view().write_metadata(slot, bytes)
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.view().sync_metadata(slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.view().read_page(index)
    }
    fn read_checked_page(
        &mut self,
        index: u64,
        expected: u64,
    ) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.view().read_checked_page(index, expected)
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.view().write_page(index, bytes)
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.view().sync_data()
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.view().truncate_data(length)
    }
}
