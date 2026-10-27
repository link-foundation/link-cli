//! Prepare plain-data growth without retiring the current view on an error.
use super::*;
use doublets::mem::Error::CapacityOverflow;
use std::alloc::Layout;

impl<T> PersistentFileMapped<T> {
    // This path is only used for values without drop glue. The upstream owner
    // can then release an overlapping old view without dropping its live data.
    pub(super) unsafe fn grow_retained(
        &mut self,
        addition: usize,
        fill: impl FnOnce(usize, (&mut [T], &mut [MaybeUninit<T>])),
    ) -> MemResult<&mut [T]> {
        unsafe { self.grow_prepared(addition, fill, prepare::<T>) }
    }

    unsafe fn grow_prepared(
        &mut self,
        addition: usize,
        fill: impl FnOnce(usize, (&mut [T], &mut [MaybeUninit<T>])),
        prepare: impl FnOnce(File, usize) -> MemResult<FileMapped<T>>,
    ) -> MemResult<&mut [T]> {
        debug_assert!(!mem::needs_drop::<T>());
        let previous = self.inner.allocated().len();
        let capacity = previous.checked_add(addition).ok_or(CapacityOverflow)?;
        let bytes = Layout::array::<T>(capacity)
            .map_err(|_| CapacityOverflow)?
            .size();
        if addition == 0 {
            fill(0, (self.inner.allocated_mut(), &mut []));
            return Ok(&mut self.inner.allocated_mut()[previous..]);
        }
        let initialized = (self.initialized_bytes / mem::size_of::<T>() as u64)
            .saturating_sub(previous as u64)
            .min(addition as u64) as usize;
        let file = self.flush.clone_file()?;
        let mut replacement = prepare(file, bytes)?;
        // SAFETY: the current view remains alive. Its prefix consists of live
        // T values; file items below initialized were captured before padding
        // or a failed extension. The caller initializes the remaining tail.
        // No reference into the old view is used during the callback.
        unsafe {
            replacement.grow(capacity, |_, (_, items)| {
                let (prefix, tail) = items.split_at_mut(previous);
                let prefix = std::slice::from_raw_parts_mut(prefix.as_mut_ptr().cast(), previous);
                fill(initialized, (prefix, tail));
            })?;
        }
        // Preparation, mapping and initialization all succeeded. Retire the old
        // descriptor under its lock before replacing/unmapping the old owner.
        self.flush.invalidate();
        self.inner = replacement;
        self.initialized_bytes = self.initialized_bytes.max(bytes as u64);
        self.flush.publish(self.inner.allocated());
        Ok(&mut self.inner.allocated_mut()[previous..])
    }
}

fn prepare<T>(file: File, bytes: usize) -> MemResult<FileMapped<T>> {
    #[cfg(windows)]
    extend_for_mapping(&file, bytes.max(4096) as u64)?;
    #[cfg(not(windows))]
    let _ = bytes;
    Ok(FileMapped::new(file)?)
}

#[cfg(windows)]
fn extend_for_mapping(file: &File, bytes: u64) -> io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READWRITE};
    if file.metadata()?.len() >= bytes {
        return Ok(());
    }
    // SAFETY: the borrowed file remains owned; no name or inheritable security
    // attributes are supplied. Windows extends it through this new mapping
    // object while existing local views stay alive. SetEndOfFile would require
    // releasing those views first. No view is created for this temporary handle.
    let mapping = unsafe {
        CreateFileMappingW(
            file.as_raw_handle().cast(),
            std::ptr::null(),
            PAGE_READWRITE,
            (bytes >> 32) as u32,
            bytes as u32,
            std::ptr::null(),
        )
    };
    if mapping.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful call returned a new, uniquely owned kernel handle.
    drop(unsafe { OwnedHandle::from_raw_handle(mapping.cast()) });
    Ok(())
}

#[cfg(test)]
#[path = "file_mem_grow_tests.rs"]
mod tests;
