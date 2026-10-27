//! Lifetime-checked barrier for an owned, offset-zero file mapping.
use std::fs::File;
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug)]
struct View {
    address: usize,
    bytes: usize,
}

#[derive(Debug)]
enum Mapping {
    Unmapped,
    Ready(View),
    Invalidated,
    Closed,
}

#[derive(Debug)]
struct State {
    file: Option<File>,
    mapping: Mapping,
}

/// A flush barrier belonging to one [`PersistentFileMapped`](super::PersistentFileMapped).
///
/// Clones follow successful remaps. Flushing an invalidated or dropped owner
/// fails without touching its old address. On Windows the view is flushed
/// before its owned file handle; on Unix the shared mapping uses the file's
/// synchronous writeback. Callers must serialize logical writes themselves.
#[derive(Clone, Debug)]
pub struct MappedFlush(Arc<Mutex<State>>);

impl MappedFlush {
    pub(super) fn new(file: File) -> Self {
        Self(Arc::new(Mutex::new(State {
            file: Some(file),
            mapping: Mapping::Unmapped,
        })))
    }

    /// Synchronizes this owner's mapped writes, propagating view/file errors.
    ///
    /// This does not repair torn pages or make a transaction atomic. Storage
    /// and devices must honor their platform's synchronization contract.
    pub fn flush(&self) -> io::Result<()> {
        self.flush_with(&SystemIo)
    }

    fn flush_with(&self, io: &impl FlushIo) -> io::Result<()> {
        let state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("mapped flush ownership is poisoned"))?;
        flush(&state, io)
    }

    // Clear under the same lock used by flush before the owner can unmap.
    // Recovery from poisoning is only for lifetime bookkeeping, never success
    // reporting. flush() continues to fail on a poisoned mutex.
    pub(super) fn invalidate(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.mapping = Mapping::Invalidated;
    }

    pub(super) fn publish<T>(&self, items: &[T]) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.mapping = if items.is_empty() {
            Mapping::Unmapped
        } else {
            Mapping::Ready(View {
                address: items.as_ptr() as usize,
                bytes: std::mem::size_of_val(items),
            })
        };
    }

    pub(super) fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        // Drop cannot report I/O errors. Explicit flush is the checked barrier.
        if !self.0.is_poisoned() {
            let _ = flush(&state, &SystemIo);
        }
        state.mapping = Mapping::Closed;
        // Retained logical handles must not retain an open file after close.
        state.file.take();
    }
}

trait FlushIo {
    fn view(&self, view: View) -> io::Result<()>;
    fn file(&self, file: &File) -> io::Result<()>;
}

fn flush(state: &State, io: &impl FlushIo) -> io::Result<()> {
    match state.mapping {
        Mapping::Ready(view) => io.view(view)?,
        Mapping::Unmapped => {}
        Mapping::Invalidated => {
            return Err(io::Error::other(
                "mapped flush owner has an unfinished remap",
            ));
        }
        Mapping::Closed => {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mapped flush owner is closed",
            ));
        }
    }
    let file = state
        .file
        .as_ref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "mapped flush file is closed"))?;
    io.file(file)
}

struct SystemIo;

impl FlushIo for SystemIo {
    fn view(&self, view: View) -> io::Result<()> {
        #[cfg(windows)]
        {
            // SAFETY: the owner publishes only its actual offset-zero mapping.
            // Its state lock remains held until both barriers finish; resize
            // and Drop invalidate the descriptor under that lock before unmap.
            if unsafe {
                windows_sys::Win32::System::Memory::FlushViewOfFile(
                    view.address as *const _,
                    view.bytes,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(not(windows))]
        let _ = (view.address, view.bytes);
        Ok(())
    }

    fn file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
}

#[cfg(test)]
#[path = "mapped_flush_tests.rs"]
mod tests;
