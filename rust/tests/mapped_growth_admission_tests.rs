//! Rejected and zero growth retain the existing destructor-bearing owner.
use anyhow::Result;
use doublets::mem::{Error, RawMem};
use link_cli::PersistentFileMapped;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Value {
    number: u64,
    drops: Arc<AtomicUsize>,
}

impl Drop for Value {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn value(drops: &Arc<AtomicUsize>, number: u64) -> Value {
    Value {
        number,
        drops: drops.clone(),
    }
}

fn reject_overflow(addition: usize) -> Result<()> {
    let drops = Arc::new(AtomicUsize::new(0));
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<Value>::new(file)?;
    mapped.grow_with(32, || value(&drops, 9))?[0].number = 42;
    let pointer = mapped.allocated().as_ptr();
    let length = observer.metadata()?.len();
    let barrier = mapped.flush_handle();
    for _ in 0..3 {
        // SAFETY: the impossible size must reject before calling this closure.
        // The finite test allocates only 32 initial and 8 following values.
        let error = unsafe { mapped.grow(addition, |_, _| panic!("overflow admitted")) }
            .err()
            .expect("impossible capacity must be rejected");
        assert!(matches!(error, Error::CapacityOverflow));
        barrier.flush()?;
        assert_eq!(observer.metadata()?.len(), length);
        assert_eq!(mapped.allocated().as_ptr(), pointer);
        assert_eq!(mapped.allocated().len(), 32);
        assert_eq!(mapped.allocated()[0].number, 42);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
    }
    mapped.grow_with(8, || value(&drops, 19))?;
    assert_eq!(mapped.allocated()[0].number, 42);
    assert!(mapped.allocated()[32..]
        .iter()
        .all(|item| item.number == 19));
    barrier.flush()?;
    drop(mapped);
    assert_eq!(drops.load(Ordering::SeqCst), 40);
    Ok(())
}

#[test]
fn generic_arithmetic_overflow_keeps_the_view_and_flush_barrier() -> Result<()> {
    reject_overflow(usize::MAX)
}

#[test]
fn generic_layout_overflow_keeps_the_view_and_flush_barrier() -> Result<()> {
    let capacity = (isize::MAX as usize / std::mem::size_of::<Value>()) + 1;
    reject_overflow(capacity - 32)
}

fn zero_growth(panic: bool) -> Result<()> {
    let drops = Arc::new(AtomicUsize::new(0));
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<Value>::new(file)?;
    mapped.grow_with(32, || value(&drops, 9))?;
    let pointer = mapped.allocated().as_ptr();
    let length = observer.metadata()?.len();
    let barrier = mapped.flush_handle();
    let mut called = 0;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: there are no new items to initialize. The existing prefix
        // stays valid, including the deliberate edit before a callback panic.
        let tail = unsafe {
            mapped.grow(0, |initialized, (prefix, tail)| {
                called += 1;
                assert_eq!(initialized, 0);
                assert_eq!(prefix.len(), 32);
                assert!(tail.is_empty());
                prefix[0].number = 42;
                if panic {
                    panic!("finite zero-growth callback failure");
                }
            })
        }?;
        assert!(tail.is_empty());
        anyhow::Ok(())
    }));
    assert_eq!(called, 1);
    if panic {
        assert!(outcome.is_err());
    } else {
        outcome.expect("successful callback")?;
    }
    barrier.flush()?;
    assert_eq!(observer.metadata()?.len(), length);
    assert_eq!(mapped.allocated().as_ptr(), pointer);
    assert_eq!(mapped.allocated().len(), 32);
    assert_eq!(mapped.allocated()[0].number, 42);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    mapped.grow_with(8, || value(&drops, 19))?;
    barrier.flush()?;
    drop(mapped);
    assert_eq!(drops.load(Ordering::SeqCst), 40);
    Ok(())
}

#[test]
fn generic_zero_growth_callback_panic_preserves_the_live_barrier() -> Result<()> {
    zero_growth(true)
}

#[test]
fn generic_zero_growth_keeps_callback_prefix_and_empty_tail_semantics() -> Result<()> {
    zero_growth(false)
}
