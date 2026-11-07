//! Test-only observation around real native mutation calls. No runtime fault
//! environment switches or observer allocations enter a production build.
use std::{cell::RefCell, fs::File, io, path::Path};
type Observer = Box<dyn Fn(&Path, usize, &str, bool)>;
type FailureObserver = Box<dyn Fn(&Path, usize, &str, bool) -> io::Result<()>>;
type Retained = Box<dyn FnOnce() -> io::Result<()>>;
thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = RefCell::new(None);
    static FAILURE: RefCell<Option<FailureObserver>> = RefCell::new(None);
    static RETAINED: RefCell<Vec<Retained>> = RefCell::new(Vec::new());
}
pub(in crate::engine::paged) fn set(observer: Option<Observer>) {
    OBSERVER.with(|current| *current.borrow_mut() = observer);
}
pub(crate) fn observe(path: &Path, file: usize, operation: &str, after: bool) -> io::Result<()> {
    OBSERVER.with(|current| {
        if let Some(observer) = &*current.borrow() {
            observer(path, file, operation, after);
        }
    });
    boundary(path, file, operation, after)
}
#[cfg(feature = "physical-lsm")]
pub(crate) fn set_failure(observer: Option<FailureObserver>) {
    FAILURE.with(|current| *current.borrow_mut() = observer);
}
pub(crate) fn boundary(path: &Path, file: usize, operation: &str, after: bool) -> io::Result<()> {
    FAILURE.with(|current| match &*current.borrow() {
        Some(observer) => observer(path, file, operation, after),
        None => Ok(()),
    })
}
pub(crate) fn retain(cleanup: Retained) {
    RETAINED.with(|retained| retained.borrow_mut().push(cleanup));
}
#[cfg(feature = "physical-lsm")]
pub(crate) fn release_retained() {
    let retained = RETAINED.with(|current| std::mem::take(&mut *current.borrow_mut()));
    for close in retained {
        close().expect("external recovery closes retained native resource");
    }
}
pub(crate) fn open_file(
    path: &Path,
    action: impl FnOnce() -> io::Result<File>,
) -> io::Result<File> {
    boundary(path, 0, "open", false)?;
    let file = action()?;
    if let Err(error) = boundary(path, 0, "open", true) {
        retain(Box::new(move || super::close_file(file)));
        return Err(error);
    }
    Ok(file)
}
pub(crate) fn close_file(path: &Path, index: usize, file: File) -> io::Result<()> {
    if let Err(error) = boundary(path, index, "close", false) {
        retain(Box::new(move || super::close_file(file)));
        return Err(error);
    }
    super::close_file(file)?;
    boundary(path, index, "close", true)
}
