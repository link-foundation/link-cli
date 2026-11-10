use super::super::{invalid, lease::Lease, LsmTableNamespace};
use super::{
    device::{Device, Image, Raw, RawLease},
    directory::Entries,
    read_only::{parts, regular, ReadOnly},
    FileLsmOptions,
};
use crate::engine::paged::{close_named, open_file, FilePageDevice};
use crate::LinkError;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(super) struct Files {
    directory: PathBuf,
    options: FileLsmOptions,
    owner: Option<File>,
    raw: Vec<(String, RawLease, bool)>,
    owned: BTreeSet<String>,
    uncertain: bool,
    closed: bool,
    failure: Option<String>,
}
impl Files {
    pub fn new(directory: &Path, options: FileLsmOptions) -> Result<Self, LinkError> {
        let directory = std::path::absolute(directory)?;
        regular(&directory, true, false)?;
        let mut open = OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        let owner = open_file(&directory.join(".lsm-writer"), &open)?;
        Ok(Self {
            directory,
            options,
            owner: Some(owner),
            raw: Vec::new(),
            owned: BTreeSet::new(),
            uncertain: false,
            closed: false,
            failure: None,
        })
    }
    fn wal_names(&self) -> Vec<String> {
        let mut names = vec!["wal".into()];
        if self.options.mirrored_root {
            names.push("wal.mirror".into());
        }
        for index in 0..2 {
            let name = format!("wal.bank{index}");
            names.push(name.clone());
            if self.options.mirrored_banks {
                names.push(format!("{name}.mirror"));
            }
        }
        names
    }
    pub fn list(&mut self, bound: usize) -> Result<Vec<u64>, LinkError> {
        let mut permitted: BTreeSet<_> = self
            .wal_names()
            .iter()
            .flat_map(|name| parts(name))
            .collect();
        permitted.insert(".lsm-writer".into());
        let maximum = permitted.len()
            + self.owned.len()
            + self.options.store.max_namespace_entries
                * if self.options.mirrored_tables { 6 } else { 3 };
        let mut entries = match Entries::open(&self.directory) {
            Ok(entries) => entries,
            Err(error) => {
                self.uncertain = true;
                return Err(error);
            }
        };
        let result = (|| {
            let mut ids = BTreeSet::new();
            let mut count = 0;
            while let Some(path) = entries.next()? {
                count += 1;
                if count > maximum {
                    return Err(invalid("native LSM enumeration bound"));
                }
                regular(&path, false, false)?;
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("foreign native LSM filename"))?;
                if permitted.contains(name) || self.owned.contains(name) {
                    continue;
                }
                let mut table = name
                    .strip_suffix(".meta0")
                    .or_else(|| name.strip_suffix(".meta1"))
                    .unwrap_or(name);
                if let Some(first) = table.strip_suffix(".mirror") {
                    if !self.options.mirrored_tables {
                        return Err(invalid("unexpected native LSM mirror"));
                    }
                    table = first;
                }
                let id = table
                    .strip_prefix("table-")
                    .and_then(|text| text.parse::<u64>().ok())
                    .filter(|id| *id > 0)
                    .ok_or_else(|| invalid("foreign native LSM entry or owner"))?;
                if table != format!("table-{id}") {
                    return Err(invalid("noncanonical native LSM identity"));
                }
                ids.insert(id);
                if ids.len() > self.options.store.max_namespace_entries || ids.len() > bound {
                    return Err(invalid("native LSM namespace bound"));
                }
            }
            Ok(ids.into_iter().collect())
        })();
        match (result, entries.close()) {
            (result, Ok(())) => result,
            (result, Err(cleanup)) => {
                self.uncertain = true;
                Err(invalid(&format!(
                    "discovery: {:?}; enumeration cleanup: {cleanup}",
                    result.err()
                )))
            }
        }
    }
    fn acquire(&mut self, name: &str, read_only: bool) -> Result<RawLease, LinkError> {
        let result = if read_only {
            ReadOnly::open(&self.directory, name).map(Image::Read)
        } else {
            FilePageDevice::open(self.directory.join(name)).map(Image::Write)
        };
        let image = match result {
            Ok(image) => image,
            Err(error) => {
                self.uncertain = true;
                return Err(error);
            }
        };
        let lease = Lease::new(Raw::new(image));
        self.raw.push((name.into(), lease.clone(), read_only));
        if !read_only {
            self.owned.insert(format!("{name}.writer"));
        }
        Ok(lease)
    }
    fn device(
        &mut self,
        name: &str,
        mirrored: bool,
        root: bool,
        read_only: bool,
    ) -> Result<Device, LinkError> {
        let mut raw = vec![self.acquire(name, read_only)?];
        if mirrored {
            raw.push(self.acquire(&format!("{name}.mirror"), read_only)?);
        }
        Device::new(
            raw,
            root,
            u32::from(self.options.mirrored_banks),
            read_only && !root,
        )
    }
    pub fn wal(&mut self, read_only: bool) -> Result<(Device, [Device; 2]), LinkError> {
        Ok((
            self.device("wal", self.options.mirrored_root, true, read_only)?,
            [
                self.device("wal.bank0", self.options.mirrored_banks, false, read_only)?,
                self.device("wal.bank1", self.options.mirrored_banks, false, read_only)?,
            ],
        ))
    }
    fn table_names(&self, id: u64) -> Vec<String> {
        let name = format!("table-{id}");
        if self.options.mirrored_tables {
            vec![name.clone(), format!("{name}.mirror")]
        } else {
            vec![name]
        }
    }
    fn table(&mut self, id: u64, read_only: bool, create: bool) -> Result<Device, LinkError> {
        if create {
            for name in self.table_names(id).iter().flat_map(|name| parts(name)) {
                if regular(&self.directory.join(name), false, true)? {
                    return Err(invalid("occupied native LSM table identity"));
                }
            }
        }
        self.device(
            &format!("table-{id}"),
            self.options.mirrored_tables,
            false,
            read_only,
        )
    }
    fn close_device(&mut self, device: &mut Device) -> Result<(), LinkError> {
        let mut errors = Vec::new();
        for raw in device.raw.iter().rev() {
            if let Err(error) = raw.use_device(Raw::close) {
                errors.push(error.to_string());
            }
        }
        if !errors.is_empty() {
            self.uncertain = true;
            return Err(invalid(&errors.join("; ")));
        }
        self.raw.retain(|(name, lease, _)| {
            let closed = lease
                .device
                .lock()
                .map(|raw| raw.image.is_none())
                .unwrap_or(false);
            if closed {
                self.owned.remove(&format!("{name}.writer"));
            }
            !closed
        });
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<(), LinkError> {
        for name in self.table_names(id).iter().flat_map(|name| parts(name)) {
            let path = self.directory.join(name);
            #[cfg(test)]
            crate::engine::paged::events::boundary(&path, 0, "delete", false)?;
            match fs::remove_file(&path) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            #[cfg(test)]
            crate::engine::paged::events::boundary(&path, 0, "delete", true)?;
        }
        #[cfg(unix)]
        {
            if let Err(error) = crate::engine::paged::sync_directory(&self.directory) {
                self.uncertain = true;
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn close_readers(&mut self) -> Result<(), LinkError> {
        let mut errors = Vec::new();
        for (_, lease, read_only) in self.raw.iter().rev() {
            if *read_only {
                if let Err(error) = lease.use_device(Raw::close) {
                    errors.push(error.to_string());
                }
            }
        }
        if !errors.is_empty() {
            self.uncertain = true;
            return Err(invalid(&errors.join("; ")));
        }
        self.raw.retain(|(_, _, read_only)| !read_only);
        Ok(())
    }
    pub fn close(&mut self, mut errors: Vec<String>) -> Result<(), LinkError> {
        if self.closed {
            return match &self.failure {
                Some(error) => Err(invalid(error)),
                None => Ok(()),
            };
        }
        self.closed = true;
        for (_, raw, _) in self.raw.iter().rev() {
            if let Err(error) = raw.use_device(Raw::close) {
                errors.push(error.to_string());
            }
        }
        if let Some(owner) = self.owner.take() {
            if let Err(error) = close_named(&self.directory.join(".lsm-writer"), 0, owner) {
                self.uncertain = true;
                errors.push(error.to_string());
            }
        }
        if !self.uncertain && errors.is_empty() {
            let path = self.directory.join(".lsm-writer");
            let remove = (|| -> io::Result<()> {
                #[cfg(test)]
                crate::engine::paged::events::boundary(&path, 0, "delete", false)?;
                fs::remove_file(&path)?;
                #[cfg(test)]
                crate::engine::paged::events::boundary(&path, 0, "delete", true)?;
                Ok(())
            })();
            if let Err(error) = remove {
                errors.push(error.to_string());
            }
        }
        if self.uncertain && errors.is_empty() {
            errors.push("uncertain native LSM ownership".into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            self.failure = Some(errors.join("; "));
            Err(invalid(self.failure.as_ref().unwrap()))
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = self.close(Vec::new());
    }
}

pub(super) struct Namespace {
    pub files: Arc<Mutex<Files>>,
    pub read_only: bool,
}
impl Namespace {
    fn io<T>(
        &self,
        action: impl FnOnce(&mut Files) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        action(
            &mut *self
                .files
                .lock()
                .map_err(|_| invalid("native LSM namespace mutex poisoned"))?,
        )
    }
}
impl LsmTableNamespace for Namespace {
    type Device = Device;
    fn supports_durable_flush(&self) -> bool {
        !self.read_only && cfg!(unix)
    }
    fn list(&mut self, bound: usize) -> Result<Vec<u64>, LinkError> {
        self.io(|files| files.list(bound))
    }
    fn open(&mut self, id: u64) -> Result<Device, LinkError> {
        self.io(|files| files.table(id, self.read_only, false))
    }
    fn create(&mut self, id: u64) -> Result<Device, LinkError> {
        if self.read_only {
            return Err(invalid("read-only native LSM create"));
        }
        self.io(|files| files.table(id, false, true))
    }
    fn close(&mut self, device: &mut Device) -> Result<(), LinkError> {
        if self.read_only {
            Ok(())
        } else {
            self.io(|files| files.close_device(device))
        }
    }
    fn delete(&mut self, id: u64) -> Result<(), LinkError> {
        if self.read_only {
            Ok(())
        } else {
            self.io(|files| files.delete(id))
        }
    }
}
