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

#[cfg(target_os = "linux")]
const CHILD: &str = "LINK_CLI_MAPPED_CHILD";

#[cfg(target_os = "linux")]
fn in_child() -> bool {
    std::env::var_os(CHILD).is_some()
}

// Runs one test of this binary alone in a child process, for failures that
// are per process or end it. Core dumps stay off in every case.
#[cfg(target_os = "linux")]
fn run_child(name: &str, setup: &str) -> Result<std::process::Output> {
    let script = format!("ulimit -c 0; {setup} exec \"$@\"");
    Ok(std::process::Command::new("sh")
        .args(["-c", &script, "sh"])
        .arg(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()?)
}

#[cfg(target_os = "linux")]
#[test]
fn quota_rejected_destructor_growth_keeps_the_view_readable() -> Result<()> {
    if !in_child() {
        // One page admits the initial view but none of the rejected extension.
        // SIGXFSZ stays ignored across exec so set_len returns its error.
        let output = run_child(
            "quota_rejected_destructor_growth_keeps_the_view_readable",
            "ulimit -f 8; trap '' XFSZ;",
        )?;
        assert!(
            output.status.success(),
            "{:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        return Ok(());
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let file = tempfile::tempfile()?;
    let observer = file.try_clone()?;
    let mut mapped = PersistentFileMapped::<Value>::new(file)?;
    mapped.grow_with(32, || value(&drops, 9))?[0].number = 42;
    let pointer = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    let length = observer.metadata()?.len();
    for _ in 0..3 {
        // SAFETY: the quota rejects the extension before any callback runs.
        let error = unsafe { mapped.grow(8192, |_, _| panic!("rejected growth was filled")) }
            .err()
            .expect("the file quota must reject this extension");
        assert_eq!(
            error.to_string(),
            std::io::Error::from_raw_os_error(27).to_string()
        );
        assert_eq!(mapped.allocated().as_ptr(), pointer);
        assert_eq!(mapped.allocated()[0].number, 42);
        assert_eq!(mapped.allocated().len(), 32);
        assert_eq!(observer.metadata()?.len(), length);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        barrier.flush()?;
    }
    // The same owner still grows within the quota and destroys each value once.
    mapped.grow_with(8, || value(&drops, 19))?;
    assert_eq!(mapped.allocated()[0].number, 42);
    assert_eq!(mapped.allocated()[32].number, 19);
    barrier.flush()?;
    drop(mapped);
    assert_eq!(drops.load(Ordering::SeqCst), 40);
    Ok(())
}

// A panicking tail destructor must never leave retired values reachable: the
// safe view and the owner's own destructor would use them a second time.
#[cfg(target_os = "linux")]
#[test]
fn panicking_tail_destructor_cannot_drop_a_value_twice() -> Result<()> {
    use std::os::unix::process::ExitStatusExt;
    // A static counter: a shared heap owner would corrupt the allocator on the
    // second drop, and that unrelated abort would look like the required one.
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Tail(#[allow(dead_code)] u64);
    impl Drop for Tail {
        fn drop(&mut self) {
            if DROPS.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("finite tail destructor failure");
            }
        }
    }
    if !in_child() {
        let output = run_child("panicking_tail_destructor_cannot_drop_a_value_twice", "")?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.signal(), Some(6), "{stderr}");
        assert!(
            stderr.contains("finite tail destructor failure"),
            "{stderr}"
        );
        return Ok(());
    }
    let mut mapped = PersistentFileMapped::<Tail>::new(tempfile::tempfile()?)?;
    mapped.grow_with(4, || Tail(7))?;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mapped.shrink(2)));
    drop(mapped);
    // Reached only if the panic was not stopped. Every value has one owner.
    assert_eq!(DROPS.load(Ordering::SeqCst), 4);
    Ok(())
}
