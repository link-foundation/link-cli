use super::*;
use std::{cell::RefCell, io};

#[test]
fn native_bank_ports_expose_identifiers_and_reject_unchecked_mirror_reads() {
    for mirrored in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bank");
        let primary = Lease::open(&path).unwrap();
        let mut bank = if mirrored {
            NativeDevice::Mirrored(Box::new(MirroredPageDevice::new(
                primary,
                Lease::open(&suffix(&path, ".mirror")).unwrap(),
            )))
        } else {
            NativeDevice::Single(primary)
        };
        assert!(bank.supports_durable_flush());
        assert_eq!(bank.metadata_flags(), u32::from(mirrored));
        assert_eq!(bank.new_database_id().unwrap().len(), 16);
        assert_eq!(bank.data_length().unwrap(), 0);
        assert!(bank.read_metadata(0).unwrap().is_none());
        assert!(bank.read_page(0).is_err());
        bank.write_page(0, &[0; PAGE_SIZE]).unwrap();
        bank.sync_data().unwrap();
        if mirrored {
            assert!(bank
                .read_page(0)
                .unwrap_err()
                .to_string()
                .contains("expected page checksum"));
        } else {
            assert_eq!(bank.read_page(0).unwrap(), [0; PAGE_SIZE]);
        }
        bank.truncate_data(0).unwrap();
        bank.sync_data().unwrap();
    }
}

pub(super) fn names_selected(path: &Path, mirrored: bool, mirrored_root: bool) -> Vec<PathBuf> {
    let mut result = vec![path.to_path_buf()];
    if mirrored_root {
        result.push(suffix(path, ".mirror"));
    }
    for bank in 0..2 {
        let primary = suffix(path, &format!(".bank{bank}"));
        result.push(primary.clone());
        if mirrored {
            result.push(suffix(&primary, ".mirror"));
        }
    }
    result
}

#[test]
fn every_acquisition_failure_and_invalid_initialization_closes_in_reverse_order() {
    for (mirrored, mirrored_root) in [(false, false), (true, false), (false, true), (true, true)] {
        let count = (if mirrored { 5 } else { 3 }) + usize::from(mirrored_root);
        for failed in 0..=count {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal");
            let mut opened = 0;
            let closed = RefCell::new(Vec::new());
            let result = FileCheckpointPagedLog::build_selected(
                &path,
                FileCheckpointOptions {
                    mirrored_banks: mirrored,
                    mirrored_root,
                    max_pages: if failed == count { 0 } else { 64 },
                },
                |name| {
                    if opened == failed {
                        return Err(invalid("injected acquisition"));
                    }
                    opened += 1;
                    Lease::open(name)
                },
                |index, device| {
                    closed.borrow_mut().push(index);
                    device.close()
                },
            );
            let error = result.err().expect("injected failure");
            assert!(error.to_string().contains(if failed == count {
                "invalid checkpoint page limit"
            } else {
                "injected acquisition"
            }));
            assert_eq!(*closed.borrow(), (0..failed).rev().collect::<Vec<_>>());
            for name in names_selected(&path, mirrored, mirrored_root) {
                assert!(!suffix(&name, ".writer").exists());
            }
        }
    }
}

#[test]
fn ordinary_explicit_close_releases_all_banks_before_root_and_never_retries() {
    for (mirrored, mirrored_root) in [(false, false), (true, false), (false, true), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut log = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        let mut order = Vec::new();
        let count = log.owners.len();
        log.close_with(|index, device| {
            order.push(index);
            device.close()
        })
        .unwrap();
        assert_eq!(order, (0..count).rev().collect::<Vec<_>>());
        log.close_with(|_, _| panic!("repeated close retried a raw handle"))
            .unwrap();
        assert!(log.epoch().is_err());
        let mut reopened = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(reopened.epoch().unwrap(), 0);
        reopened.close().unwrap();
    }
}

#[test]
fn every_uncertain_close_attempts_all_four_raw_handles_and_preserves_each_fence() {
    for (mirrored, mirrored_root) in [(false, false), (true, false), (false, true), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut log = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        let mut handles = 0;
        let mut order = Vec::new();
        let error = log
            .close_with(|index, device| {
                order.push(index);
                device.close_with(|file| {
                    drop(file);
                    handles += 1;
                    Err(io::Error::other(format!("uncertain handle {index}")))
                })
            })
            .unwrap_err();
        let paths = names_selected(&path, mirrored, mirrored_root);
        assert_eq!(handles, paths.len() * 4);
        assert_eq!(order, (0..paths.len()).rev().collect::<Vec<_>>());
        for (index, name) in paths.iter().enumerate() {
            assert!(error
                .to_string()
                .contains(&format!("uncertain handle {index}")));
            assert!(suffix(name, ".writer").exists());
        }
        log.close().unwrap();
        assert_eq!(handles, paths.len() * 4);
        assert!(log.read_log_entries().is_err());
        assert!(FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            }
        )
        .is_err());
    }
}

#[test]
fn constructor_preserves_original_error_and_every_reverse_cleanup_failure() {
    for (mirrored, mirrored_root) in [(false, false), (true, false), (false, true), (true, true)] {
        let count = (if mirrored { 5 } else { 3 }) + usize::from(mirrored_root);
        for failed in 1..=count {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal");
            let mut opened = 0;
            let mut handles = 0;
            let mut order = Vec::new();
            let error = FileCheckpointPagedLog::build_selected(
                &path,
                FileCheckpointOptions {
                    mirrored_banks: mirrored,
                    mirrored_root,
                    max_pages: if failed == count { 0 } else { 64 },
                },
                |name| {
                    if opened == failed {
                        return Err(invalid("original acquisition error"));
                    }
                    opened += 1;
                    Lease::open(name)
                },
                |index, device| {
                    order.push(index);
                    device.close_with(|file| {
                        drop(file);
                        handles += 1;
                        Err(io::Error::other(format!("cleanup device {index}")))
                    })
                },
            )
            .err()
            .expect("initialization failed");
            assert!(error.to_string().contains(if failed == count {
                "invalid checkpoint page limit"
            } else {
                "original acquisition error"
            }));
            assert_eq!(order, (0..failed).rev().collect::<Vec<_>>());
            assert_eq!(handles, 4 * failed);
            for index in 0..failed {
                assert!(error
                    .to_string()
                    .contains(&format!("cleanup device {index}")));
                assert!(suffix(
                    &names_selected(&path, mirrored, mirrored_root)[index],
                    ".writer"
                )
                .exists());
            }
        }
    }
}

#[test]
fn poisoned_leases_reject_io_but_cleanup_still_attempts_every_raw_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal");
    let mut log = FileCheckpointPagedLog::open(&path, false).unwrap();
    let mut lease = log.owners[1].clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = lease.0.lock().unwrap();
        panic!("poison the selected device");
    }))
    .is_err());
    assert!(lease
        .data_length()
        .unwrap_err()
        .to_string()
        .contains("poisoned"));
    let mut closed = Vec::new();
    log.close_with(|index, device| {
        closed.push(index);
        device.close()
    })
    .unwrap();
    assert_eq!(closed, [2, 1, 0]);
    drop(lease);
    let mut reopened = FileCheckpointPagedLog::open(&path, false).unwrap();
    assert!(reopened.read_log_entries().unwrap().is_empty());
    reopened.close().unwrap();
}

#[test]
fn one_uncertain_bank_remains_fenced_after_root_release_before_any_repair() {
    for (mirrored, mirrored_root) in [(false, false), (true, false), (false, true), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut log = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        log.replace_log_entries(&["acknowledged".into()]).unwrap();
        log.append_log_entry("pending must stay private").unwrap();
        let devices = names_selected(&path, mirrored, mirrored_root);
        let files: Vec<_> = devices
            .iter()
            .flat_map(|name| [name.clone(), suffix(name, ".meta0"), suffix(name, ".meta1")])
            .collect();
        let before: Vec<_> = files
            .iter()
            .map(|name| std::fs::read(name).unwrap())
            .collect();
        let bank_index = 1 + usize::from(mirrored_root);
        let mut attempts = 0;
        let error = log
            .close_with(|index, device| {
                if index == bank_index {
                    device.close_with(|file| {
                        drop(file);
                        attempts += 1;
                        Err(io::Error::other("uncertain bank close"))
                    })
                } else {
                    device.close()
                }
            })
            .unwrap_err();
        assert!(error.to_string().contains("uncertain bank close"));
        assert_eq!(attempts, 4);
        for (index, name) in devices.iter().enumerate() {
            assert_eq!(suffix(name, ".writer").exists(), index == bank_index);
        }
        log.close().unwrap();
        assert_eq!(attempts, 4);
        assert!(FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(
            files
                .iter()
                .map(|name| std::fs::read(name).unwrap())
                .collect::<Vec<_>>(),
            before
        );
        for (index, name) in devices.iter().enumerate() {
            assert_eq!(suffix(name, ".writer").exists(), index == bank_index);
        }
        // The injected close dropped every raw handle before returning error.
        std::fs::remove_file(suffix(&devices[bank_index], ".writer")).unwrap();
        let mut reopened = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(reopened.read_log_entries().unwrap(), ["acknowledged"]);
        reopened.append_log_entry("following").unwrap();
        reopened.flush_log().unwrap();
        reopened.replace_log_entries(&["compacted".into()]).unwrap();
        reopened.close().unwrap();
        let mut again = FileCheckpointPagedLog::with_options(
            &path,
            FileCheckpointOptions {
                mirrored_banks: mirrored,
                mirrored_root,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["compacted"]);
        again.close().unwrap();
    }
}

#[test]
fn native_root_port_forwards_metadata_recovery_and_rejects_page_storage() {
    for bank_flags in [0, 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("root");
        let root = MirroredCheckpointRoot::new(
            Lease::open(&path).unwrap(),
            Lease::open(&suffix(&path, ".mirror")).unwrap(),
            bank_flags,
        )
        .unwrap();
        let mut root = NativeDevice::Root(Box::new(root));
        assert!(root.supports_durable_flush());
        assert_eq!(root.metadata_flags(), 2 | bank_flags);
        let identity = root.new_database_id().unwrap();
        assert_eq!(root.data_length().unwrap(), 0);
        assert!(root.read_metadata(0).unwrap().is_none());
        let decision = super::super::checkpoint_format::Decision {
            identity,
            epoch: 0,
            flags: 2 | bank_flags,
        }
        .encode();
        root.write_metadata(0, &decision).unwrap();
        root.sync_metadata(0).unwrap();
        assert_eq!(root.read_metadata(0).unwrap().unwrap(), decision);
        root.complete_recovery().unwrap();
        assert!(root.read_page(0).is_err());
        assert!(root.read_checked_page(0, 0).is_err());
        assert!(root.write_page(0, &[0; PAGE_SIZE]).is_err());
        root.truncate_data(0).unwrap();
        root.sync_data().unwrap();
        assert!(root.truncate_data(1).is_err());
    }
}
