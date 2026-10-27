use super::*;

#[test]
fn rejected_mapping_after_extension_preserves_the_original_initialization_bound() {
    let file = tempfile::tempfile().unwrap();
    let observer = file.try_clone().unwrap();
    let mut mapped = PersistentFileMapped::<u64>::new(file).unwrap();
    mapped.grow_filled(32, 9).unwrap()[0] = 42;
    let pointer = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    for _ in 0..3 {
        // SAFETY: preparation returns an error before any callback or new
        // allocation is published. No uninitialized item is exposed.
        let result = unsafe {
            mapped.grow_prepared(
                8192,
                |_, _| panic!("unreachable fill"),
                |file, bytes| {
                    #[cfg(windows)]
                    extend_for_mapping(&file, bytes as u64)?;
                    #[cfg(not(windows))]
                    file.set_len(bytes as u64)?;
                    Err(io::Error::other("injected replacement mapping failure").into())
                },
            )
        };
        assert!(result.unwrap_err().to_string().contains("injected"));
        assert_eq!(mapped.allocated().as_ptr(), pointer);
        assert_eq!(mapped.allocated(), [&[42][..], &[9; 31][..]].concat());
        assert_eq!(observer.metadata().unwrap().len(), 8224 * 8);
        assert_eq!(mapped.initialized_bytes, 32 * 8);
        barrier.flush().unwrap();
    }
    // Physical file growth from the failed attempt must not turn the new tail
    // into persisted items and silently skip its requested initializer.
    assert_eq!(mapped.grow_filled(8192, 19).unwrap(), [19; 8192]);
    barrier.flush().unwrap();
    assert_eq!(mapped.allocated()[0], 42);
}

#[test]
fn zero_growth_runs_the_callback_without_retiring_or_replacing_a_view() {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile().unwrap()).unwrap();
    let barrier = mapped.flush_handle();
    for populated in [false, true] {
        if populated {
            mapped.grow_filled(32, 9).unwrap();
        }
        let previous = mapped.allocated().as_ptr();
        // SAFETY: no new items are requested; the complete tail is empty.
        let tail = unsafe {
            mapped.grow(0, |initialized, (prefix, tail)| {
                assert_eq!(initialized, 0);
                assert_eq!(prefix.len(), if populated { 32 } else { 0 });
                assert!(tail.is_empty());
                if populated {
                    prefix[0] = 73;
                }
            })
        }
        .unwrap();
        assert!(tail.is_empty());
        assert_eq!(mapped.allocated().as_ptr(), previous);
        barrier.flush().unwrap();
    }
    assert_eq!(mapped.allocated()[0], 73);
}

#[test]
fn clone_failure_never_enters_preparation_or_retires_the_current_view() {
    let mut mapped = PersistentFileMapped::<u64>::new(tempfile::tempfile().unwrap()).unwrap();
    mapped.grow_filled(32, 9).unwrap()[0] = 42;
    let pointer = mapped.allocated().as_ptr();
    // Close only the logical barrier in this private invariant control. The
    // actual FileMapped owner remains alive and must not be touched on error.
    mapped.flush.close();
    let result = unsafe {
        mapped.grow_prepared(
            64,
            |_, _| panic!("unreachable fill"),
            |_, _| panic!("unreachable preparation"),
        )
    };
    assert!(result.unwrap_err().to_string().contains("closed"));
    assert_eq!(mapped.allocated().as_ptr(), pointer);
    assert_eq!(mapped.allocated()[0], 42);
}
