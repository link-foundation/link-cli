#![cfg(all(feature = "native-pages", not(target_arch = "wasm32")))]
//! A file page device refuses addresses outside its format.
use link_cli::engine::paged::{FilePageDevice, PageDevice, MAX_PAGES, PAGE_SIZE};
use link_cli::LinkError;

fn refused<T>(outcome: Result<T, LinkError>, expected: &str) {
    match outcome {
        Ok(_) => panic!("expected a refusal about {expected}"),
        Err(error) => assert!(error.to_string().contains(expected), "{error}"),
    }
}

#[test]
fn addresses_outside_the_format_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut device = FilePageDevice::open(dir.path().join("pages")).unwrap();
    let page = [7u8; PAGE_SIZE];

    refused(device.read_metadata(2), "metadata slot");
    refused(device.write_metadata(2, &[0; 128]), "metadata slot");
    refused(device.sync_metadata(2), "metadata slot");
    refused(device.read_page(MAX_PAGES), "physical page address");
    refused(device.write_page(MAX_PAGES, &page), "physical page address");
    refused(
        device.truncate_data(MAX_PAGES * PAGE_SIZE as u64 + 1),
        "physical data length",
    );
    // Pages are written in order, so the next one must end exactly the file.
    refused(device.write_page(1, &page), "changed during ownership");
    device.write_page(0, &page).unwrap();
    assert_eq!(device.read_page(0).unwrap(), page);
}
