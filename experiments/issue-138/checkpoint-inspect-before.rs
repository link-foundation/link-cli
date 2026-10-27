// Type-check the proposed borrowed API before production implementation.
use link_cli::engine::paged::{CheckpointPagedLog, PageDevice};
fn inspect<D: PageDevice>(root: &mut D, banks: &mut [D; 2]) {
    let _: Result<Vec<String>, _> = CheckpointPagedLog::inspect(root, banks, 128);
}
#[test]
fn inspection_api_is_available() { let _ = inspect::<Placeholder>; }
// An abstract bound above is sufficient to expose a missing associated API.
type Placeholder = link_cli::engine::paged::FilePageDevice;
