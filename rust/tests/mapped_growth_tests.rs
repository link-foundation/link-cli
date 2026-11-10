//! Ordinary raw mappings retain ownership until successful replacement.
use anyhow::Result;
use doublets::mem::RawMem;
use link_cli::PersistentFileMapped;
use std::io::{Seek, SeekFrom, Write};

#[test]
fn overflow_keeps_the_current_pointer_contents_and_flush_owner() -> Result<()> {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile()?)?;
    mapped.grow_filled(32, 0)?[0] = 42;
    let previous = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    for _ in 0..3 {
        assert!(mapped.grow_filled(usize::MAX, 0).is_err());
        barrier.flush()?;
        assert_eq!(mapped.allocated().as_ptr(), previous);
        assert_eq!(mapped.allocated()[0], 42);
    }
    mapped.grow_filled(64, 9)?;
    barrier.flush()?;
    assert_eq!(mapped.allocated()[0], 42);
    Ok(())
}

#[test]
fn fresh_padding_and_uninitialized_extension_are_filled_exactly() -> Result<()> {
    let file = tempfile::tempfile()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    assert_eq!(mapped.grow_filled(32, 9)?, [9; 32]);
    assert_eq!(mapped.grow_filled(64, 17)?, [17; 64]);
    assert_eq!(&mapped.allocated()[..32], [9; 32]);
    mapped.flush_handle().flush()?;
    Ok(())
}

#[test]
fn only_original_file_items_are_reported_initialized_to_the_fill_callback() -> Result<()> {
    let mut file = tempfile::tempfile()?;
    for value in 1_u64..=16 {
        file.write_all(&value.to_ne_bytes())?;
    }
    file.seek(SeekFrom::Start(0))?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    assert_eq!(mapped.grow_filled(8, 91)?, [1, 2, 3, 4, 5, 6, 7, 8]);
    // SAFETY: persisted items are valid u64 values; every remaining item is
    // initialized below. The initialized count is asserted independently.
    let tail = unsafe {
        mapped.grow(24, |initialized, (prefix, tail)| {
            assert_eq!(initialized, 8);
            assert_eq!(prefix, [1, 2, 3, 4, 5, 6, 7, 8]);
            assert_eq!(tail.len(), 24);
            for item in &mut tail[initialized..] {
                item.write(91);
            }
        })?
    };
    assert_eq!(&tail[..8], [9, 10, 11, 12, 13, 14, 15, 16]);
    assert_eq!(&tail[8..], [91; 16]);
    mapped.flush_handle().flush()?;
    Ok(())
}

#[test]
fn panicking_fill_retains_the_old_allocation_and_checked_barrier() -> Result<()> {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile()?)?;
    mapped.grow_filled(32, 0)?[0] = 42;
    let barrier = mapped.flush_handle();
    let previous = mapped.allocated().as_ptr();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the closure never returns, so it exposes no uninitialized
        // items as initialized. Its prefix edit is deliberately observable.
        let _ = unsafe {
            mapped.grow(8192, |_, (prefix, _)| {
                prefix[1] = 73;
                panic!("finite initialization failure");
            })
        };
    }));
    assert!(result.is_err());
    barrier.flush()?;
    assert_eq!(mapped.allocated().as_ptr(), previous);
    assert_eq!(mapped.allocated().len(), 32);
    assert_eq!(&mapped.allocated()[..2], [42, 73]);
    assert_eq!(mapped.grow_filled(8192, 19)?, [19; 8192]);
    barrier.flush()?;
    Ok(())
}

#[test]
fn generic_destructor_values_keep_their_existing_single_owner_behavior() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Clone)]
    struct Byte(u8);
    impl Drop for Byte {
        fn drop(&mut self) {
            assert!(self.0 <= 9);
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut mapped = PersistentFileMapped::<Byte>::new(tempfile::tempfile()?)?;
    // This byte-only type has a valid representation for every file byte.
    // grow_with initializes all new items, independent of persisted padding.
    mapped.grow_with(32, || Byte(9))?;
    mapped.grow_with(8, || Byte(9))?;
    mapped.shrink(8)?;
    assert_eq!(DROPS.load(Ordering::SeqCst), 8);
    drop(mapped);
    assert_eq!(DROPS.load(Ordering::SeqCst), 40);
    Ok(())
}

#[cfg(target_os = "linux")]
#[path = "support/mapped_growth_quota.rs"]
mod quota;
