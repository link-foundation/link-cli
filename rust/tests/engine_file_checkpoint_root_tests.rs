#![cfg(all(feature = "native-pages", not(target_arch = "wasm32")))]
use link_cli::engine::paged::{FileCheckpointOptions, FileCheckpointPagedLog};
use link_cli::{ReplaceableTransitionLogStore, TransitionLogStore};

#[test]
fn native_root_mirroring_recovers_the_acknowledged_epoch_with_either_bank_mode() {
    for mirrored_banks in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("journal");
        let options = FileCheckpointOptions {
            mirrored_banks,
            mirrored_root: true,
            ..Default::default()
        };
        let mut initial = FileCheckpointPagedLog::with_options(&path, options).unwrap();
        initial.replace_log_entries(&["old".into()]).unwrap();
        initial
            .replace_log_entries(&["acknowledged 雪".into()])
            .unwrap();
        initial.close().unwrap();
        let sidecar = |suffix: &str| path.with_file_name(format!("journal{suffix}"));
        assert!(sidecar(".mirror.meta0").exists());
        std::fs::write(sidecar(".meta0"), "torn").unwrap();
        let mut recovered = FileCheckpointPagedLog::with_options(&path, options).unwrap();
        assert_eq!(recovered.epoch().unwrap(), 2);
        assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged 雪"]);
        recovered.append_log_entry("following").unwrap();
        recovered.flush_log().unwrap();
        recovered
            .replace_log_entries(&["next".into(), "following".into()])
            .unwrap();
        recovered.close().unwrap();
        assert_eq!(
            std::fs::read(sidecar(".meta1")).unwrap(),
            std::fs::read(sidecar(".mirror.meta1")).unwrap()
        );
        let mut again = FileCheckpointPagedLog::with_options(&path, options).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["next", "following"]);
        again.close().unwrap();
    }
}

fn device_names(path: &std::path::Path, options: FileCheckpointOptions) -> Vec<std::path::PathBuf> {
    let mut result = vec![path.to_path_buf()];
    if options.mirrored_root {
        result.push(sidecar(path, ".mirror"));
    }
    for bank in 0..2 {
        let name = sidecar(path, &format!(".bank{bank}"));
        result.push(name.clone());
        if options.mirrored_banks {
            result.push(sidecar(&name, ".mirror"));
        }
    }
    result
}
fn sidecar(path: &std::path::Path, extension: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(extension);
    name.into()
}
fn files(devices: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    devices
        .iter()
        .flat_map(|name| {
            [
                name.clone(),
                sidecar(name, ".meta0"),
                sidecar(name, ".meta1"),
            ]
        })
        .collect()
}
fn snapshot(files: &[std::path::PathBuf]) -> Vec<Vec<u8>> {
    files
        .iter()
        .map(|name| std::fs::read(name).unwrap())
        .collect()
}
#[test]
fn all_independently_selected_fences_precede_root_and_bank_repair() {
    for mirrored_root in [false, true] {
        for mirrored_banks in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("journal");
            let options = FileCheckpointOptions {
                mirrored_root,
                mirrored_banks,
                ..Default::default()
            };
            let mut log = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            log.replace_log_entries(&["previous".into()]).unwrap();
            log.replace_log_entries(&["acknowledged 雪".into()])
                .unwrap();
            log.close().unwrap();
            if mirrored_root {
                std::fs::write(sidecar(&path, ".meta0"), b"torn root").unwrap();
            }
            if mirrored_banks {
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(sidecar(&path, ".bank0"))
                    .unwrap()
                    .set_len(19)
                    .unwrap();
                std::fs::write(sidecar(&path, ".bank0.meta0"), b"torn bank").unwrap();
            }
            let devices = device_names(&path, options);
            let files = files(&devices);
            let before = snapshot(&files);
            for foreign in &devices {
                std::fs::write(sidecar(foreign, ".writer"), b"foreign owner").unwrap();
                assert!(FileCheckpointPagedLog::with_options(&path, options).is_err());
                assert_eq!(snapshot(&files), before);
                for name in &devices {
                    assert_eq!(sidecar(name, ".writer").exists(), name == foreign);
                }
                assert_eq!(
                    std::fs::read(sidecar(foreign, ".writer")).unwrap(),
                    b"foreign owner"
                );
                std::fs::remove_file(sidecar(foreign, ".writer")).unwrap();
            }
            let mut recovered = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged 雪"]);
            recovered.close().unwrap();
            if mirrored_root {
                assert_eq!(
                    std::fs::read(sidecar(&path, ".meta0")).unwrap(),
                    std::fs::read(sidecar(&path, ".mirror.meta0")).unwrap()
                );
            }
            if mirrored_banks {
                assert_eq!(
                    std::fs::read(sidecar(&path, ".bank0")).unwrap(),
                    std::fs::read(sidecar(&path, ".bank0.mirror")).unwrap()
                );
            }
            let mut following = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            following.append_log_entry("following").unwrap();
            following.flush_log().unwrap();
            following
                .replace_log_entries(&["compacted".into()])
                .unwrap();
            following.close().unwrap();
            let mut again = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            assert_eq!(again.read_log_entries().unwrap(), ["compacted"]);
            again.close().unwrap();
        }
    }
}
#[test]
fn native_root_and_bank_mode_conversions_preserve_existing_bytes() {
    for mirrored_root in [false, true] {
        for mirrored_banks in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("journal");
            let options = FileCheckpointOptions {
                mirrored_root,
                mirrored_banks,
                ..Default::default()
            };
            let mut log = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            log.replace_log_entries(&["acknowledged".into()]).unwrap();
            log.close().unwrap();
            let files = files(&device_names(&path, options));
            let before = snapshot(&files);
            for wrong in [
                FileCheckpointOptions {
                    mirrored_root: !mirrored_root,
                    ..options
                },
                FileCheckpointOptions {
                    mirrored_banks: !mirrored_banks,
                    ..options
                },
            ] {
                let error = FileCheckpointPagedLog::with_options(&path, wrong)
                    .err()
                    .expect("mode conversion rejected");
                assert!(error.to_string().contains("flags") || error.to_string().contains("mode"));
                assert_eq!(snapshot(&files), before);
                for name in device_names(
                    &path,
                    FileCheckpointOptions {
                        mirrored_root: true,
                        mirrored_banks: true,
                        ..options
                    },
                ) {
                    assert!(!sidecar(&name, ".writer").exists());
                }
            }
            let mut again = FileCheckpointPagedLog::with_options(&path, options).unwrap();
            assert_eq!(again.read_log_entries().unwrap(), ["acknowledged"]);
            again.close().unwrap();
        }
    }
}
