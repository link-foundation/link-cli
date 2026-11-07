//! Directory handles have an observable, exactly-once close, like file owners.
use super::super::invalid;
use crate::LinkError;
use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        ffi::{CStr, CString, OsStr},
        os::unix::ffi::OsStrExt,
        ptr::NonNull,
    };
    pub(super) struct Directory {
        handle: Option<NonNull<libc::DIR>>,
    }
    impl Directory {
        pub fn open(path: &Path) -> Result<Self, LinkError> {
            let name = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| invalid("native LSM directory contains NUL"))?;
            // SAFETY: the path is NUL-terminated and remains alive for the call.
            let handle = NonNull::new(unsafe { libc::opendir(name.as_ptr()) })
                .ok_or_else(io::Error::last_os_error)?;
            Ok(Self {
                handle: Some(handle),
            })
        }
        pub fn next(&mut self) -> Result<Option<PathBuf>, LinkError> {
            let handle = self
                .handle
                .ok_or_else(|| invalid("closed native LSM directory"))?;
            loop {
                errno::set_errno(errno::Errno(0));
                // SAFETY: this directory uniquely owns an open DIR and only this
                // method advances it. Copy each name before another readdir.
                let entry = unsafe { libc::readdir(handle.as_ptr()) };
                if entry.is_null() {
                    let error = errno::errno().0;
                    return if error == 0 {
                        Ok(None)
                    } else {
                        Err(io::Error::from_raw_os_error(error).into())
                    };
                }
                // SAFETY: successful readdir supplies a NUL-terminated d_name.
                let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                return Ok(Some(PathBuf::from(OsStr::from_bytes(name))));
            }
        }
        pub fn close(&mut self) -> Result<(), LinkError> {
            if let Some(handle) = self.handle.take() {
                // SAFETY: take sole ownership before exactly one closedir. A
                // failed call may have released it; never retry the raw value.
                if unsafe { libc::closedir(handle.as_ptr()) } != 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            Ok(())
        }
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{FindClose, FindFirstFileW, FindNextFileW, WIN32_FIND_DATAW},
    };
    pub(super) struct Directory {
        handle: Option<HANDLE>,
        first: Option<WIN32_FIND_DATAW>,
    }
    impl Directory {
        pub fn open(path: &Path) -> Result<Self, LinkError> {
            let pattern: Vec<u16> = path
                .join("*")
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            // SAFETY: WIN32_FIND_DATAW contains only integer arrays/fields, for
            // which zero is valid; FindFirstFileW initializes it on success.
            let mut entry: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
            // SAFETY: the UTF-16 path is terminated; the output is writable.
            let handle = unsafe { FindFirstFileW(pattern.as_ptr(), &mut entry) };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error().into());
            }
            Ok(Self {
                handle: Some(handle),
                first: Some(entry),
            })
        }
        pub fn next(&mut self) -> Result<Option<PathBuf>, LinkError> {
            let handle = self
                .handle
                .ok_or_else(|| invalid("closed native LSM directory"))?;
            loop {
                let entry = if let Some(entry) = self.first.take() {
                    entry
                } else {
                    // SAFETY: a zeroed output is valid and this iterator owns
                    // the open search handle until its exactly-once close.
                    let mut entry: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
                    if unsafe { FindNextFileW(handle, &mut entry) } == 0 {
                        // SAFETY: obtain the thread's error immediately after
                        // the failed native call, before performing other I/O.
                        let error = unsafe { GetLastError() };
                        return if error == ERROR_NO_MORE_FILES {
                            Ok(None)
                        } else {
                            Err(io::Error::from_raw_os_error(error as i32).into())
                        };
                    }
                    entry
                };
                let length = entry
                    .cFileName
                    .iter()
                    .position(|c| *c == 0)
                    .ok_or_else(|| invalid("unterminated native LSM filename"))?;
                let name = OsString::from_wide(&entry.cFileName[..length]);
                if name == "." || name == ".." {
                    continue;
                }
                return Ok(Some(PathBuf::from(name)));
            }
        }
        pub fn close(&mut self) -> Result<(), LinkError> {
            if let Some(handle) = self.handle.take() {
                // SAFETY: consume sole ownership once; never retry uncertainty.
                if unsafe { FindClose(handle) } == 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            Ok(())
        }
    }
}
pub(super) struct Entries {
    directory: PathBuf,
    inner: Option<platform::Directory>,
    closed: bool,
    failure: Option<String>,
}
impl Entries {
    pub fn open(path: &Path) -> Result<Self, LinkError> {
        #[cfg(test)]
        crate::engine::paged::events::boundary(path, 0, "enumeration-open", false)?;
        let inner = platform::Directory::open(path)?;
        #[cfg(test)]
        if let Err(error) =
            crate::engine::paged::events::boundary(path, 0, "enumeration-open", true)
        {
            retain(inner);
            return Err(error.into());
        }
        Ok(Self {
            directory: path.into(),
            inner: Some(inner),
            closed: false,
            failure: None,
        })
    }
    pub fn next(&mut self) -> Result<Option<PathBuf>, LinkError> {
        if self.closed {
            return Err(invalid("closed native LSM enumeration"));
        }
        #[cfg(test)]
        crate::engine::paged::events::boundary(&self.directory, 0, "enumeration-next", false)?;
        let name = self.inner.as_mut().unwrap().next()?;
        #[cfg(test)]
        crate::engine::paged::events::boundary(&self.directory, 0, "enumeration-next", true)?;
        Ok(name.map(|name| self.directory.join(name)))
    }
    pub fn close(&mut self) -> Result<(), LinkError> {
        if !self.closed {
            self.closed = true;
            if let Err(error) = self.close_inner() {
                self.failure = Some(error.to_string());
            }
        }
        match &self.failure {
            Some(error) => Err(invalid(error)),
            None => Ok(()),
        }
    }
    fn close_inner(&mut self) -> Result<(), LinkError> {
        let mut inner = self.inner.take().unwrap();
        #[cfg(test)]
        if let Err(error) =
            crate::engine::paged::events::boundary(&self.directory, 0, "enumeration-close", false)
        {
            retain(inner);
            return Err(error.into());
        }
        inner.close()?;
        #[cfg(test)]
        crate::engine::paged::events::boundary(&self.directory, 0, "enumeration-close", true)?;
        Ok(())
    }
}
#[cfg(test)]
fn retain(mut inner: platform::Directory) {
    crate::engine::paged::events::retain(Box::new(move || {
        inner
            .close()
            .map_err(|error| io::Error::other(error.to_string()))
    }));
}
impl Drop for Entries {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
