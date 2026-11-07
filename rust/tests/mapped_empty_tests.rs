//! Empty and rejected shrink requests must preserve a valid mapping owner.
use anyhow::Result;
use doublets::mem::RawMem;
use link_cli::PersistentFileMapped;
use std::io::Write;

#[test]
fn zero_shrink_of_a_fresh_owner_is_a_noop() -> Result<()> {
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    let length = observer.metadata()?.len();
    let pointer = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    mapped.shrink(0)?;
    assert_eq!(observer.metadata()?.len(), length);
    assert_eq!(mapped.allocated().as_ptr(), pointer);
    barrier.flush()?;
    assert_eq!(mapped.grow_filled(8, 73)?, [73; 8]);
    Ok(())
}

#[test]
fn zero_shrink_never_erases_unadopted_persisted_items() -> Result<()> {
    let mut file = tempfile::tempfile()?;
    for value in 1_u64..=16 {
        file.write_all(&value.to_ne_bytes())?;
    }
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    assert!(mapped.allocated().is_empty());
    mapped.shrink(0)?;
    assert_eq!(mapped.grow_filled(16, 91)?, (1..=16).collect::<Vec<_>>());
    mapped.flush_handle().flush()?;
    Ok(())
}

#[test]
fn zero_shrink_of_a_live_view_keeps_its_physical_file_and_pointer() -> Result<()> {
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    mapped.grow_filled(32, 9)?[0] = 42;
    let pointer = mapped.allocated().as_ptr();
    let length = observer.metadata()?.len();
    let barrier = mapped.flush_handle();
    for _ in 0..3 {
        mapped.shrink(0)?;
        assert_eq!(observer.metadata()?.len(), length);
        assert_eq!(mapped.allocated().as_ptr(), pointer);
        assert_eq!(mapped.allocated()[0], 42);
        barrier.flush()?;
    }
    Ok(())
}

#[test]
fn shrinking_all_items_can_flush_reopen_empty_and_grow_again() -> Result<()> {
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<u64>::new(file)?;
    mapped.grow_filled(32, 9)?[0] = 42;
    let barrier = mapped.flush_handle();
    mapped.shrink(32)?;
    assert!(mapped.allocated().is_empty());
    assert_eq!(observer.metadata()?.len(), 0);
    barrier.flush()?;
    // The empty-capacity reopen must remain usable while the first owner
    // also has no live items. It adopts the zero bytes before page padding.
    let reopened = PersistentFileMapped::<u64>::open_existing(observer.try_clone()?)?;
    assert!(reopened.allocated().is_empty());
    reopened.flush_handle().flush()?;
    drop(reopened);
    mapped.shrink(0)?;
    assert_eq!(mapped.grow_filled(8, 19)?, [19; 8]);
    barrier.flush()?;
    drop(mapped);
    let reopened = PersistentFileMapped::<u64>::open_existing(observer)?;
    // Upstream pads short files. Only the live owner's logical items are
    // asserted here; arbitrary reopening still sees physical page padding.
    assert_eq!(&reopened.allocated()[..8], [19; 8]);
    Ok(())
}

#[test]
fn rejected_overshrink_panics_before_changing_view_or_barrier() -> Result<()> {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile()?)?;
    mapped.grow_filled(32, 9)?[0] = 42;
    let pointer = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = mapped.shrink(33);
    }))
    .is_err());
    barrier.flush()?;
    assert_eq!(mapped.allocated().as_ptr(), pointer);
    assert_eq!(mapped.allocated().len(), 32);
    assert_eq!(mapped.allocated()[0], 42);
    Ok(())
}

#[test]
fn shrinking_all_destructor_values_retires_each_exactly_once() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Byte(u8);
    impl Drop for Byte {
        fn drop(&mut self) {
            assert_eq!(self.0, 9);
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut mapped = PersistentFileMapped::<Byte>::new(tempfile::tempfile()?)?;
    mapped.grow_with(32, || Byte(9))?;
    let barrier = mapped.flush_handle();
    mapped.shrink(32)?;
    assert!(mapped.allocated().is_empty());
    assert_eq!(DROPS.load(Ordering::SeqCst), 32);
    barrier.flush()?;
    mapped.shrink(0)?;
    assert_eq!(DROPS.load(Ordering::SeqCst), 32);
    mapped.grow_with(8, || Byte(9))?;
    drop(mapped);
    assert_eq!(DROPS.load(Ordering::SeqCst), 40);
    Ok(())
}
