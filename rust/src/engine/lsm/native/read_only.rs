use super::super::invalid;
use crate::engine::paged::{
    close_named, open_file, PageDevice, RepairPageDevice, MAX_PAGES, PAGE_SIZE,
};
use crate::LinkError;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(super) fn regular(path: &Path, directory: bool, missing: bool) -> Result<bool, LinkError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if missing && error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("native LSM reparse point"));
        }
    }
    if if directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    } {
        return Err(invalid("native LSM entry must be regular and nonsymbolic"));
    }
    Ok(true)
}

pub(super) fn parts(name: &str) -> [String; 3] {
    [
        name.into(),
        format!("{name}.meta0"),
        format!("{name}.meta1"),
    ]
}

pub(super) struct ReadOnly {
    name: PathBuf,
    files: Vec<Option<File>>,
    failure: Option<String>,
    closed: bool,
}
impl ReadOnly {
    pub fn open(directory: &Path, name: &str) -> Result<Self, LinkError> {
        let mut result = Self {
            name: directory.join(name),
            files: Vec::new(),
            failure: None,
            closed: false,
        };
        let acquire = (|| {
            for name in parts(name) {
                let path = directory.join(name);
                if !regular(&path, false, true)? {
                    result.files.push(None);
                    continue;
                }
                let file = open_file(&path, OpenOptions::new().read(true))?;
                result.files.push(Some(file));
                if !result
                    .files
                    .last()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .metadata()?
                    .is_file()
                {
                    return Err(invalid("nonregular native LSM discovery handle"));
                }
            }
            Ok(())
        })();
        if let Err(cause) = acquire {
            if let Err(cleanup) = result.close() {
                return Err(invalid(&format!("{cause}; discovery cleanup: {cleanup}")));
            }
            return Err(cause);
        }
        Ok(result)
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.closed {
            Err(invalid("closed native LSM discovery"))
        } else {
            Ok(())
        }
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        if !self.closed {
            self.closed = true;
            let errors: Vec<_> = self
                .files
                .drain(..)
                .enumerate()
                .rev()
                .filter_map(|(index, file)| file.map(|file| (index, file)))
                .filter_map(|(index, file)| {
                    close_named(&self.name, index, file)
                        .err()
                        .map(|e| e.to_string())
                })
                .collect();
            if !errors.is_empty() {
                self.failure = Some(errors.join("; "));
            }
        }
        match &self.failure {
            Some(error) => Err(invalid(error)),
            None => Ok(()),
        }
    }
}
impl Drop for ReadOnly {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
fn rejected() -> LinkError {
    invalid("read-only native LSM discovery")
}
impl PageDevice for ReadOnly {
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        Err(rejected())
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        self.check()?;
        Ok(match &self.files[0] {
            Some(file) => file.metadata()?.len(),
            None => 0,
        })
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        self.check()?;
        if slot > 1 {
            return Err(invalid("native LSM metadata slot"));
        }
        let Some(file) = &mut self.files[slot + 1] else {
            return Ok(None);
        };
        let length = file.metadata()?.len();
        if length == 0 {
            return Ok(None);
        }
        if length != 128 {
            return Ok(Some(vec![0]));
        }
        let mut bytes = vec![0; 128];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut bytes)?;
        Ok(Some(bytes))
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        self.check()?;
        if index >= MAX_PAGES {
            return Err(invalid("native LSM page index"));
        }
        let file = self.files[0]
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        let mut bytes = [0; PAGE_SIZE];
        file.seek(SeekFrom::Start(index * PAGE_SIZE as u64))?;
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn write_metadata(&mut self, _: usize, _: &[u8; 128]) -> Result<(), LinkError> {
        Err(rejected())
    }
    fn sync_metadata(&mut self, _: usize) -> Result<(), LinkError> {
        Err(rejected())
    }
    fn write_page(&mut self, _: u64, _: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        Err(rejected())
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        Err(rejected())
    }
    fn truncate_data(&mut self, _: u64) -> Result<(), LinkError> {
        Err(rejected())
    }
}
impl RepairPageDevice for ReadOnly {
    fn repair_page(&mut self, _: u64, _: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        Err(rejected())
    }
}
