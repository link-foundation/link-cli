//! Persistent memory-mapped backing store for `doublets`.
//!
//! # Why this wrapper exists
//!
//! `doublets` resizes its memory through `RawMem::grow_filled`, whose
//! default implementation in `platform-mem` fills the **entire** newly
//! mapped region with `Default::default()` — including the part that is
//! already backed by bytes on disk:
//!
//! ```text
//! fn grow_filled(&mut self, cap: usize, value: Self::Item) -> Result<&mut [Self::Item]> {
//!     unsafe { self.grow(cap, |_, (_, uninit)| { uninit::fill(uninit, value); }) }
//! }
//! ```
//!
//! `FileMapped` computes how many of those elements were already
//! initialised on disk and passes it as the `inited` argument, but the
//! default `grow_filled` ignores it. The consequence is that opening an
//! existing file-mapped `doublets` database zeroes it: every link is
//! lost. `docs/case-studies/issue-98/evidence/doublets_persistence.rs`
//! reproduces this against upstream `doublets` directly.
//! The bug is reported upstream as
//! <https://github.com/linksplatform/mem-rs/issues/36>.
//!
//! [`PersistentFileMapped`] fixes this by forwarding to
//! `RawMem::grow_filled_exact`, which fills only `uninit[inited..]` and
//! therefore preserves whatever was already written to the file.
//!
//! # Reopening an existing mapping
//!
//! Upstream [`FileMapped::new`] starts with a logical capacity of zero even
//! when its file already contains initialized elements. Use
//! [`PersistentFileMapped::open_existing`] when the file's bytes are a complete
//! persisted mapping and its capacity must be visible immediately. The safe
//! API is available only for [`FileMappedValue`] types, whose representations
//! this crate can soundly adopt without asking each caller for an `unsafe`
//! block.
//!
//! # Durability
//!
//! Writes land in a `MAP_SHARED` mapping, which on Linux *is* the page
//! cache, and the kernel writes dirty pages back. A checked durability barrier
//! is available through [`MappedFlush`] handles:
//! Windows flushes the mapped view before synchronizing the owned file;
//! Unix synchronizes the file's shared page cache. Drop attempts that barrier
//! without reporting errors. Explicit flush reports them. These barriers do
//! not repair torn pages or make a sequence of in-place mutations atomic.

use std::fs::File;
use std::io;
use std::mem::{self, MaybeUninit};
use std::path::Path;

use doublets::data::LinkReference;
use doublets::mem::{FileMapped, RawMem, Result as MemResult};
use doublets::unit::LinkPart;

use super::MappedFlush;

#[path = "file_mem_grow.rs"]
mod grow;

/// A value whose representation can safely be adopted from existing file bytes.
///
/// This is the safety boundary used by
/// [`PersistentFileMapped::open_existing`]. The crate implements it for the
/// unsigned integer link-address types and for [`LinkPart`] values containing
/// those addresses.
///
/// # Safety
///
/// Every initialized byte pattern of `Self` must represent a valid value, it
/// must be safe to drop any such value, and `Self` must not be zero-sized.
pub unsafe trait FileMappedValue {}

macro_rules! impl_file_mapped_value_for_unsigned {
    ($($ty:ty),+ $(,)?) => {
        $(
            // SAFETY: every bit pattern is valid for unsigned integers, they
            // have no drop glue, and none of these types is zero-sized.
            unsafe impl FileMappedValue for $ty {}
        )+
    };
}

impl_file_mapped_value_for_unsigned!(u8, u16, u32, u64, u128, usize);

// SAFETY: `LinkPart<T>` is `repr(C)` and consists only of eight `T` fields.
// When every bit pattern is valid for `T`, it is therefore valid for the
// complete link part as well, and dropping it only drops those fields.
unsafe impl<T: FileMappedValue + LinkReference> FileMappedValue for LinkPart<T> {}

/// A [`FileMapped`] region that does **not** wipe pre-existing file
/// contents when `doublets` grows it.
///
/// See the module documentation for the upstream behaviour this works
/// around.
#[derive(Debug)]
pub struct PersistentFileMapped<T> {
    inner: FileMapped<T>,
    flush: MappedFlush,
    initialized_bytes: u64,
}

impl<T> PersistentFileMapped<T> {
    /// Opens (creating it if needed) the file at `path` and maps it.
    pub fn from_path<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .and_then(Self::new)
    }

    /// Maps an already-opened file.
    pub fn new(file: File) -> io::Result<Self> {
        // Capture before upstream adds minimum-page padding. Failed growth can
        // also extend a file before mapping fails; neither is initialized data.
        let initialized_bytes = file.metadata()?.len();
        Ok(Self {
            inner: FileMapped::new(file.try_clone()?)?,
            flush: MappedFlush::new(file),
            initialized_bytes,
        })
    }

    /// Borrows the wrapped [`FileMapped`].
    pub fn inner(&self) -> &FileMapped<T> {
        &self.inner
    }

    /// Returns a checked flush barrier that follows this owner's remaps.
    ///
    /// Retain this before adopting the mapping into a generic doublets store,
    /// then attach it with [`DoubletsStorage::with_mapped_flush`](super::DoubletsStorage::with_mapped_flush).
    /// Rejected growth of values without drop glue retains the existing view
    /// and barrier. Failed shrink or destructor growth can leave unsafe upstream
    /// allocator state and invalidate the barrier. Reopening alone cannot establish
    /// safe cleanup. Dropping a valid owner closes every retained handle.
    pub fn flush_handle(&self) -> MappedFlush {
        self.flush.clone()
    }
}

impl<T: FileMappedValue> PersistentFileMapped<T> {
    /// Maps `file` and adopts the capacity represented by its existing bytes.
    ///
    /// Unlike [`Self::new`], which starts with a logical capacity of zero, this
    /// constructor makes every complete `T` already present in the file
    /// immediately visible through [`RawMem::allocated`]. Any trailing bytes
    /// that do not form a complete `T` are left untouched and ignored.
    ///
    /// ```no_run
    /// #![deny(unsafe_code)]
    /// use std::fs::File;
    ///
    /// use link_cli::doublets::unit::LinkPart;
    /// use link_cli::PersistentFileMapped;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let file = File::options().read(true).write(true).open("links.data")?;
    /// let mapped = PersistentFileMapped::<LinkPart<usize>>::open_existing(file)?;
    /// # let _ = mapped;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open_existing(file: File) -> io::Result<Self> {
        // Read this before `FileMapped::new`, which extends sub-page files to
        // its minimum mapping size. That padding was not part of the persisted
        // contents and must not become visible as logical items.
        let byte_len = file.metadata()?.len();
        let mut mapped = Self::new(file)?;
        let item_size = mem::size_of::<T>() as u64;
        debug_assert_ne!(item_size, 0, "FileMappedValue must not be zero-sized");
        let capacity = usize::try_from(byte_len / item_size).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "file capacity does not fit in usize",
            )
        })?;

        // SAFETY: `FileMapped::new` guarantees the file contains `byte_len`
        // initialized bytes, and `FileMappedValue` guarantees every byte
        // pattern in each complete item is a valid, safely droppable `T`.
        if capacity != 0 {
            unsafe { mapped.grow_assumed(capacity) }.map_err(|error| match error {
                doublets::mem::Error::System(error) => error,
                error => io::Error::other(error),
            })?;
        }

        Ok(mapped)
    }
}

impl<T> RawMem for PersistentFileMapped<T> {
    type Item = T;

    fn allocated(&self) -> &[Self::Item] {
        self.inner.allocated()
    }

    fn allocated_mut(&mut self) -> &mut [Self::Item] {
        self.inner.allocated_mut()
    }

    unsafe fn grow(
        &mut self,
        addition: usize,
        fill: impl FnOnce(usize, (&mut [Self::Item], &mut [MaybeUninit<Self::Item>])),
    ) -> MemResult<&mut [Self::Item]> {
        if !mem::needs_drop::<T>() {
            // Two views may share plain data, but dropping overlapping owners
            // of destructor values would drop those values twice.
            return unsafe { self.grow_retained(addition, fill) };
        }
        self.flush.invalidate();
        let previous_len = self.inner.allocated().len();
        unsafe { self.inner.grow(addition, fill) }?;
        self.initialized_bytes = self
            .initialized_bytes
            .max(mem::size_of_val(self.inner.allocated()) as u64);
        // RawMem::grow returns only the added tail. The barrier must cover
        // the complete current mapping, including writes to its old prefix.
        self.flush.publish(self.inner.allocated());
        Ok(&mut self.inner.allocated_mut()[previous_len..])
    }

    fn shrink(&mut self, cap: usize) -> MemResult<()> {
        self.flush.invalidate();
        self.inner.shrink(cap)?;
        self.initialized_bytes = mem::size_of_val(self.inner.allocated()) as u64;
        self.flush.publish(self.inner.allocated());
        Ok(())
    }

    /// Fills only the genuinely uninitialised tail of the grown region,
    /// keeping the bytes that were already persisted in the file.
    fn grow_filled(&mut self, cap: usize, value: Self::Item) -> MemResult<&mut [Self::Item]>
    where
        Self::Item: Clone,
    {
        // SAFETY: growth reports only genuinely initialized file items below
        // `inited`; the closure fills every remaining item. Plain-data growth
        // excludes minimum-page padding and unsuccessful extension attempts.
        unsafe { self.grow_filled_exact(cap, value) }
    }
}

impl<T> Drop for PersistentFileMapped<T> {
    fn drop(&mut self) {
        // Close retained handles before the inner owner unmaps its address.
        self.flush.close();
    }
}
