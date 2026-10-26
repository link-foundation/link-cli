//! Optional native I/O; no filesystem/entropy/close bindings enter WASM.
use super::{format::invalid, PageDevice, RepairPageDevice, PAGE_SIZE};
use crate::LinkError;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Native exclusive page device. Parent must exist. Existing/stale `.writer`
/// ownership is never stolen; externally fence its process before removal.
/// Close attempts all handles and retains ownership after any uncertain error.
/// Prefer explicit close to observe cleanup errors; Drop preserves the fence.
pub struct FilePageDevice {
    files: Vec<File>,
    owner: Option<File>,
    lock: PathBuf,
    closed: bool,
}
fn suffix(path: &Path, extension: &str) -> PathBuf {
    let mut result = path.as_os_str().to_owned();
    result.push(extension);
    result.into()
}
impl FilePageDevice {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LinkError> {
        let path = std::path::absolute(path)?;
        let lock = suffix(&path, ".writer");
        let mut ownership = OpenOptions::new();
        ownership.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            ownership.mode(0o600);
        }
        let owner = ownership.open(&lock)?;
        let mut result = Self {
            files: Vec::new(),
            owner: Some(owner),
            lock,
            closed: false,
        };
        let initialize = (|| -> Result<(), LinkError> {
            for file in [
                path.clone(),
                suffix(&path, ".meta0"),
                suffix(&path, ".meta1"),
            ] {
                let mut options = OpenOptions::new();
                options.read(true).write(true).create(true).truncate(false);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                result.files.push(options.open(file)?);
            }
            #[cfg(unix)]
            File::open(path.parent().expect("absolute file parent"))?.sync_all()?;
            Ok(())
        })();
        if let Err(error) = initialize {
            if let Err(cleanup) = result.close() {
                return Err(invalid(&format!("{error}; cleanup failed: {cleanup}")));
            }
            return Err(error);
        }
        Ok(result)
    }
    fn file(&mut self, index: usize) -> Result<&mut File, LinkError> {
        if self.closed {
            return Err(invalid("native page device is closed"));
        }
        self.files
            .get_mut(index)
            .ok_or_else(|| invalid("page device file index"))
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        self.close_with(close_file)
    }
    pub(super) fn close_with(
        &mut self,
        mut close: impl FnMut(File) -> io::Result<()>,
    ) -> Result<(), LinkError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let mut failures = Vec::new();
        for file in self.files.drain(..).rev().chain(self.owner.take()) {
            if let Err(error) = close(file) {
                failures.push(error.to_string());
            }
        }
        // Never retry close: a failed syscall may already have released and
        // allowed reuse of its raw handle. Retain the owner file instead.
        if failures.is_empty() {
            if let Err(error) = std::fs::remove_file(&self.lock) {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "native page cleanup failed: {}",
                failures.join("; ")
            ))
            .into())
        }
    }
}
impl Drop for FilePageDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl PageDevice for FilePageDevice {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        self.file(0)?;
        let mut identity = [0; 16];
        getrandom::fill(&mut identity)
            .map_err(|error| invalid(&format!("database entropy: {error}")))?;
        Ok(identity)
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        Ok(self.file(0)?.metadata()?.len())
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        if slot > 1 {
            return Err(invalid("metadata slot"));
        }
        let file = self.file(slot + 1)?;
        match file.metadata()?.len() {
            0 => Ok(None),
            128 => {
                let mut bytes = vec![0; 128];
                file.rewind()?;
                file.read_exact(&mut bytes)?;
                Ok(Some(bytes))
            }
            _ => Ok(Some(vec![0])), // Reject malformed length without unbounded allocation.
        }
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        if slot > 1 {
            return Err(invalid("metadata slot"));
        }
        let file = self.file(slot + 1)?;
        file.rewind()?;
        file.write_all(bytes)?;
        file.set_len(128)?;
        Ok(())
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        if slot > 1 {
            return Err(invalid("metadata slot"));
        }
        self.file(slot + 1)?.sync_all()?;
        Ok(())
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        if index >= super::MAX_PAGES {
            return Err(invalid("physical page address"));
        }
        let file = self.file(0)?;
        file.seek(SeekFrom::Start(index * PAGE_SIZE as u64))?;
        let mut bytes = [0; PAGE_SIZE];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        if index >= super::MAX_PAGES {
            return Err(invalid("physical page address"));
        }
        let file = self.file(0)?;
        let position = index * PAGE_SIZE as u64;
        if file.metadata()?.len() != position {
            return Err(invalid("native page file changed during ownership"));
        }
        file.seek(SeekFrom::Start(position))?;
        file.write_all(bytes)?;
        Ok(())
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.file(0)?.sync_all()?;
        Ok(())
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        if length > super::MAX_PAGES * PAGE_SIZE as u64 {
            return Err(invalid("physical data length"));
        }
        self.file(0)?.set_len(length)?;
        Ok(())
    }
}
impl RepairPageDevice for FilePageDevice {
    fn repair_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        if index >= super::MAX_PAGES {
            return Err(invalid("physical repair address"));
        }
        let file = self.file(0)?;
        let offset = index * PAGE_SIZE as u64;
        if file.metadata()?.len() < offset {
            return Err(invalid("repair would create a hole"));
        }
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        Ok(())
    }
}
#[cfg(unix)]
fn close_file(file: File) -> io::Result<()> {
    use std::os::fd::IntoRawFd;
    // SAFETY: consume the uniquely owned File exactly once; do not retry an
    // uncertain close, whose descriptor may already have been released.
    if unsafe { libc::close(file.into_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(windows)]
fn close_file(file: File) -> io::Result<()> {
    use std::os::windows::io::IntoRawHandle;
    // SAFETY: transfer sole File ownership to one CloseHandle invocation.
    if unsafe { windows_sys::Win32::Foundation::CloseHandle(file.into_raw_handle()) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_close_preserves_ownership_and_attempts_all_handles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        let mut device = FilePageDevice::open(&path).unwrap();
        let mut closed = 0;
        let error = device
            .close_with(|file| {
                close_file(file)?;
                closed += 1;
                Err(io::Error::other("injected close"))
            })
            .unwrap_err();
        assert!(error.to_string().contains("injected close"));
        assert_eq!(closed, 4);
        assert!(FilePageDevice::open(&path).is_err());
        device.close().unwrap();
        assert!(suffix(&path, ".writer").exists());
    }
}
