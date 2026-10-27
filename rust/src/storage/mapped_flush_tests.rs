use super::*;
use std::cell::RefCell;

struct RecordingIo {
    calls: RefCell<Vec<&'static str>>,
    failure: Option<&'static str>,
}
impl RecordingIo {
    fn new(failure: Option<&'static str>) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failure,
        }
    }
    fn record(&self, phase: &'static str) -> io::Result<()> {
        self.calls.borrow_mut().push(phase);
        if self.failure == Some(phase) {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, phase))
        } else {
            Ok(())
        }
    }
}
impl FlushIo for RecordingIo {
    fn view(&self, view: View) -> io::Result<()> {
        assert_eq!((view.address, view.bytes), (0x1000, 24));
        self.record("view")
    }
    fn file(&self, _: &File) -> io::Result<()> {
        self.record("file")
    }
}
fn state(mapping: Mapping) -> State {
    State {
        file: Some(tempfile::tempfile().unwrap()),
        mapping,
    }
}
fn ready() -> Mapping {
    Mapping::Ready(View {
        address: 0x1000,
        bytes: 24,
    })
}

#[test]
fn view_barrier_precedes_file_barrier_and_each_failure_remains_retryable() {
    let state = state(ready());
    for failure in [None, Some("view"), Some("file")] {
        let io = RecordingIo::new(failure);
        let result = flush(&state, &io);
        if let Some(phase) = failure {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(error.to_string(), phase);
        } else {
            result.unwrap();
        }
        assert_eq!(
            *io.calls.borrow(),
            if failure == Some("view") {
                vec!["view"]
            } else {
                vec!["view", "file"]
            }
        );
        let retry = RecordingIo::new(None);
        flush(&state, &retry).unwrap();
        assert_eq!(*retry.calls.borrow(), ["view", "file"]);
    }
}

#[test]
fn no_view_flushes_only_the_file_and_invalid_owners_touch_neither() {
    let io = RecordingIo::new(None);
    flush(&state(Mapping::Unmapped), &io).unwrap();
    assert_eq!(*io.calls.borrow(), ["file"]);
    for mapping in [Mapping::Invalidated, Mapping::Closed] {
        let io = RecordingIo::new(None);
        assert!(flush(&state(mapping), &io).is_err());
        assert!(io.calls.borrow().is_empty());
    }
    let mut closed_file = state(Mapping::Unmapped);
    closed_file.file.take();
    let io = RecordingIo::new(None);
    assert_eq!(
        flush(&closed_file, &io).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(io.calls.borrow().is_empty());
}

#[test]
fn publishing_and_close_update_all_handles_without_stale_access() {
    let owner = MappedFlush::new(tempfile::tempfile().unwrap());
    let handle = owner.clone();
    owner.flush().unwrap();
    owner.invalidate();
    assert!(handle.flush().is_err());
    let items = [1_u64, 2, 3];
    owner.publish(&items);
    {
        let state = handle.0.lock().unwrap();
        let Mapping::Ready(view) = state.mapping else {
            panic!("successful remap must publish its new view")
        };
        assert_eq!(view.address, items.as_ptr() as usize);
        assert_eq!(view.bytes, 24);
    }
    // Never send these heap bytes to a real mapping syscall on Windows.
    owner.publish::<u64>(&[]);
    handle.flush().unwrap();
    owner.close();
    assert!(handle.0.lock().unwrap().file.is_none());
    assert_eq!(
        handle.flush().unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn poisoned_ownership_remains_failed_but_can_be_closed_safely() {
    let owner = MappedFlush::new(tempfile::tempfile().unwrap());
    let shared = owner.clone();
    assert!(std::thread::spawn(move || {
        let _guard = shared.0.lock().unwrap();
        panic!("finite poison control");
    })
    .join()
    .is_err());
    assert!(owner.flush().unwrap_err().to_string().contains("poisoned"));
    owner.invalidate();
    owner.publish::<u8>(&[]);
    assert!(owner.flush().is_err());
    owner.close();
    let state = owner.0.lock().unwrap_err().into_inner();
    assert!(matches!(state.mapping, Mapping::Closed));
    assert!(state.file.is_none());
}

#[test]
fn owner_cannot_replace_or_close_a_view_during_either_barrier() {
    struct LockedIo<'a>(&'a MappedFlush, RefCell<Vec<&'static str>>);
    impl FlushIo for LockedIo<'_> {
        fn view(&self, _: View) -> io::Result<()> {
            assert!(matches!(
                self.0 .0.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            self.1.borrow_mut().push("view");
            Ok(())
        }
        fn file(&self, _: &File) -> io::Result<()> {
            assert!(matches!(
                self.0 .0.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            self.1.borrow_mut().push("file");
            Ok(())
        }
    }
    let owner = MappedFlush::new(tempfile::tempfile().unwrap());
    owner.0.lock().unwrap().mapping = ready();
    let io = LockedIo(&owner, RefCell::new(Vec::new()));
    owner.flush_with(&io).unwrap();
    assert_eq!(*io.1.borrow(), ["view", "file"]);
    // This test used only mock I/O; close must not pass the dummy view to Windows.
    owner.invalidate();
    owner.close();
}

#[test]
fn real_owner_publishes_only_its_current_allocation() {
    use doublets::mem::RawMem;
    let mut owner =
        super::super::PersistentFileMapped::<u64>::new(tempfile::tempfile().unwrap()).unwrap();
    let handle = owner.flush_handle();
    for operation in [0, 1, 2] {
        match operation {
            0 => {
                owner.grow_filled(32, 0).unwrap();
            }
            1 => {
                owner.grow_filled(8192, 0).unwrap();
            }
            _ => owner.shrink(12).unwrap(),
        }
        {
            let state = handle.0.lock().unwrap();
            let Mapping::Ready(view) = state.mapping else {
                panic!("mapping has not published its current allocation")
            };
            assert_eq!(view.address, owner.allocated().as_ptr() as usize);
            assert_eq!(view.bytes, std::mem::size_of_val(owner.allocated()));
        }
        handle.flush().unwrap();
    }
    drop(owner);
    let closed = handle.0.lock().unwrap();
    assert!(matches!(closed.mapping, Mapping::Closed));
    assert!(closed.file.is_none());
}
