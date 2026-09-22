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
//! cache, so they survive a process crash without any explicit action
//! and are written back by the kernel. `FileMapped` additionally
//! `sync_all()`s the file when it is dropped, and
//! [`LinksStorage::flush`](crate::LinksStorage::flush) `fsync`s on
//! demand for durability across a machine crash.

use std::fs::File;
use std::io;
use std::mem::{self, MaybeUninit};
use std::path::Path;

use doublets::data::LinkReference;
use doublets::mem::{FileMapped, RawMem, Result as MemResult};
use doublets::unit::LinkPart;

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
pub struct PersistentFileMapped<T>(FileMapped<T>);

impl<T> PersistentFileMapped<T> {
    /// Opens (creating it if needed) the file at `path` and maps it.
    pub fn from_path<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        FileMapped::from_path(path).map(Self)
    }

    /// Maps an already-opened file.
    pub fn new(file: File) -> io::Result<Self> {
        FileMapped::new(file).map(Self)
    }

    /// Borrows the wrapped [`FileMapped`].
    pub fn inner(&self) -> &FileMapped<T> {
        &self.0
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
        let metadata = file.try_clone()?;
        let mut mapped = FileMapped::new(file)?;
        let byte_len = metadata.metadata()?.len();
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

        Ok(Self(mapped))
    }
}

impl<T> RawMem for PersistentFileMapped<T> {
    type Item = T;

    fn allocated(&self) -> &[Self::Item] {
        self.0.allocated()
    }

    fn allocated_mut(&mut self) -> &mut [Self::Item] {
        self.0.allocated_mut()
    }

    unsafe fn grow(
        &mut self,
        addition: usize,
        fill: impl FnOnce(usize, (&mut [Self::Item], &mut [MaybeUninit<Self::Item>])),
    ) -> MemResult<&mut [Self::Item]> {
        unsafe { self.0.grow(addition, fill) }
    }

    fn shrink(&mut self, cap: usize) -> MemResult<()> {
        self.0.shrink(cap)
    }

    /// Fills only the genuinely uninitialised tail of the grown region,
    /// keeping the bytes that were already persisted in the file.
    fn grow_filled(&mut self, cap: usize, value: Self::Item) -> MemResult<&mut [Self::Item]>
    where
        Self::Item: Clone,
    {
        // SAFETY: `FileMapped::grow` derives `inited` from the size the
        // file had before growing, so the elements below it really are
        // initialised (they were written by a previous session).
        unsafe { self.0.grow_filled_exact(cap, value) }
    }
}
