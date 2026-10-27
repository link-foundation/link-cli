use super::{read_only::ReadOnly, *};
use crate::engine::paged::{PageDevice, RepairPageDevice, MAX_PAGES, PAGE_SIZE};
use std::fs;

#[test]
fn native_discovery_absent_images_and_mutators_never_create_files() {
    let directory = tempfile::tempdir().unwrap();
    let mut reader = ReadOnly::open(directory.path(), "missing").unwrap();
    assert_eq!(reader.data_length().unwrap(), 0);
    for slot in 0..2 {
        assert!(reader.read_metadata(slot).unwrap().is_none());
    }
    assert!(
        matches!(reader.read_page(0), Err(LinkError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    assert!(reader.read_page(MAX_PAGES).is_err());
    assert!(reader.read_metadata(2).is_err());
    assert!(reader.new_database_id().is_err());
    assert!(reader.write_page(0, &[0; PAGE_SIZE]).is_err());
    assert!(reader.repair_page(0, &[0; PAGE_SIZE]).is_err());
    assert!(reader.truncate_data(0).is_err());
    assert!(reader.write_metadata(0, &[0; 128]).is_err());
    assert!(reader.sync_metadata(0).is_err());
    assert!(reader.sync_data().is_err());
    reader.close().unwrap();
    reader.close().unwrap();
    assert!(reader.data_length().is_err());
    assert!(reader.read_page(0).is_err());
    assert!(reader.read_metadata(0).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn native_discovery_metadata_allocation_and_page_reads_are_fixed_size() {
    let directory = tempfile::tempdir().unwrap();
    for length in [0, 127, 128, 129, 1048576] {
        let file = fs::File::create(directory.path().join("image.meta0")).unwrap();
        file.set_len(length).unwrap();
        drop(file);
        fs::write(directory.path().join("image"), [0; PAGE_SIZE]).unwrap();
        let mut reader = ReadOnly::open(directory.path(), "image").unwrap();
        let metadata = reader.read_metadata(0).unwrap();
        assert_eq!(
            metadata.map(|bytes| bytes.len()),
            match length {
                0 => None,
                128 => Some(128),
                _ => Some(1),
            }
        );
        assert_eq!(reader.read_page(0).unwrap(), [0; PAGE_SIZE]);
        assert!(reader.read_page(1).is_err());
        reader.close().unwrap();
    }
}
