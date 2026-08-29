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
//! # Durability
//!
//! Writes land in a `MAP_SHARED` mapping, which on Linux *is* the page
//! cache, so they survive a process crash without any explicit action
//! and are written back by the kernel. `FileMapped` additionally
//! `sync_all()`s the file when it is dropped, and
//! [`LinksStorage::flush`](crate::LinksStorage::flush) `fsync`s on
//! demand for durability across a machine crash.

use std::mem::MaybeUninit;
use std::path::Path;

use doublets::mem::{FileMapped, RawMem, Result as MemResult};

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
    pub fn new(file: std::fs::File) -> std::io::Result<Self> {
        FileMapped::new(file).map(Self)
    }

    /// Borrows the wrapped [`FileMapped`].
    pub fn inner(&self) -> &FileMapped<T> {
        &self.0
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
