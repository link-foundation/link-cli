//! Page padding is not initialized destructor-bearing application data.
use anyhow::Result;
use doublets::mem::RawMem;
use link_cli::PersistentFileMapped;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

// Each test owns a distinct counter. Every byte pattern is valid, including
// the upstream padding used by the negative control; no pointer is persisted.
macro_rules! counted_value {
    ($value:ident, $drops:ident) => {
        static $drops: AtomicUsize = AtomicUsize::new(0);
        #[repr(transparent)]
        #[derive(Clone)]
        struct $value(u64);
        impl Drop for $value {
            fn drop(&mut self) {
                $drops.fetch_add(1, Ordering::SeqCst);
            }
        }
    };
}

counted_value!(FreshValue, FRESH_DROPS);
counted_value!(PersistedValue, PERSISTED_DROPS);
counted_value!(CallbackValue, CALLBACK_DROPS);

#[test]
fn generic_filled_growth_initializes_padding_with_the_requested_value() -> Result<()> {
    let mut mapped = PersistentFileMapped::<FreshValue>::new(tempfile::tempfile()?)?;
    mapped.grow_filled(32, FreshValue(9))?;
    assert!(mapped.allocated().iter().all(|value| value.0 == 9));
    assert_eq!(FRESH_DROPS.load(Ordering::SeqCst), 0);
    mapped.grow_filled(8, FreshValue(19))?;
    assert!(mapped.allocated()[..32].iter().all(|value| value.0 == 9));
    assert!(mapped.allocated()[32..].iter().all(|value| value.0 == 19));
    mapped.flush_handle().flush()?;
    drop(mapped);
    assert_eq!(FRESH_DROPS.load(Ordering::SeqCst), 40);
    Ok(())
}

fn persisted_file() -> Result<std::fs::File> {
    let mut file = tempfile::tempfile()?;
    for value in 1_u64..=16 {
        file.write_all(&value.to_ne_bytes())?;
    }
    Ok(file)
}

#[test]
fn generic_filled_growth_preserves_only_the_original_persisted_items() -> Result<()> {
    let mut mapped = PersistentFileMapped::<PersistedValue>::new(persisted_file()?)?;
    mapped.grow_filled(8, PersistedValue(91))?;
    assert_eq!(PERSISTED_DROPS.load(Ordering::SeqCst), 1);
    // Cross the upstream minimum page so both its extension calculation and
    // its within-page path are checked independently by this test and below.
    mapped.grow_filled(1024, PersistedValue(91))?;
    assert_eq!(
        mapped.allocated()[..16]
            .iter()
            .map(|value| value.0)
            .collect::<Vec<_>>(),
        (1..=16).collect::<Vec<_>>()
    );
    assert!(mapped.allocated()[16..].iter().all(|value| value.0 == 91));
    mapped.grow_filled(8, PersistedValue(73))?;
    assert!(mapped.allocated()[1032..].iter().all(|value| value.0 == 73));
    mapped.flush_handle().flush()?;
    mapped.shrink(8)?;
    assert_eq!(PERSISTED_DROPS.load(Ordering::SeqCst), 9);
    drop(mapped);
    assert_eq!(PERSISTED_DROPS.load(Ordering::SeqCst), 1041);
    Ok(())
}

#[test]
fn generic_raw_growth_reports_the_logical_initialized_tail() -> Result<()> {
    let mut mapped = PersistentFileMapped::<CallbackValue>::new(persisted_file()?)?;
    mapped.grow_filled(8, CallbackValue(91))?;
    let mut observed = None;
    // SAFETY: these transparent u64 values accept all file bytes. The original
    // eight persisted tail items are valid; write every genuinely new item,
    // independently of the reported count. Check the count after returning so
    // the negative control never interrupts upstream initialization with panic.
    unsafe {
        mapped.grow(24, |initialized, (prefix, tail)| {
            observed = Some((initialized, prefix.len(), tail.len()));
            for item in &mut tail[8..] {
                item.write(CallbackValue(91));
            }
        })?;
    }
    assert_eq!(observed, Some((8, 8, 24)));
    assert_eq!(
        mapped.allocated()[..16]
            .iter()
            .map(|value| value.0)
            .collect::<Vec<_>>(),
        (1..=16).collect::<Vec<_>>()
    );
    assert!(mapped.allocated()[16..].iter().all(|value| value.0 == 91));
    mapped.grow_filled(8, CallbackValue(73))?;
    assert!(mapped.allocated()[32..].iter().all(|value| value.0 == 73));
    mapped.flush_handle().flush()?;
    drop(mapped);
    assert_eq!(CALLBACK_DROPS.load(Ordering::SeqCst), 41);
    Ok(())
}
