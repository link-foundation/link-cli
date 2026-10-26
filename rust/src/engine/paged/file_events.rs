//! Test-only observation around real native mutation calls. No runtime fault
//! environment switches or observer allocations enter a production build.
use std::{cell::RefCell, path::Path};
type Observer = Box<dyn Fn(&Path, usize, &str, bool)>;
thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = RefCell::new(None);
}
pub(in crate::engine::paged) fn set(observer: Option<Observer>) {
    OBSERVER.with(|current| *current.borrow_mut() = observer);
}
pub(super) fn observe(path: &Path, file: usize, operation: &str, after: bool) {
    OBSERVER.with(|current| {
        if let Some(observer) = &*current.borrow() {
            observer(path, file, operation, after);
        }
    });
}
