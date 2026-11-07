//! Native ownership control, copied into the actual test target before implementation.
#![cfg(all(feature = "native-pages", any(unix, windows)))]
use link_cli::engine::paged::{FileCheckpointPagedLog, FilePageDevice, PagedLog};
use link_cli::{ReplaceableTransitionLogStore, TransitionLogStore};

#[test]
fn selected_checkpoint_factory_rejects_implicit_ordinary_journal_conversion() {
    for mirrored in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut ordinary = PagedLog::open(FilePageDevice::open(&path).unwrap()).unwrap();
        ordinary.append_log_entry("ordinary 雪").unwrap();
        ordinary.flush_log().unwrap();
        ordinary.into_device().close().unwrap();
        let files = [
            path.clone(),
            path.with_extension("meta0"),
            path.with_extension("meta1"),
        ];
        let before: Vec<_> = files
            .iter()
            .map(|name| std::fs::read(name).unwrap())
            .collect();
        assert!(FileCheckpointPagedLog::open(&path, mirrored).is_err());
        assert_eq!(
            files
                .iter()
                .map(|name| std::fs::read(name).unwrap())
                .collect::<Vec<_>>(),
            before
        );
        assert!(!path.with_extension("writer").exists());
        let mut reopened = PagedLog::open(FilePageDevice::open(&path).unwrap()).unwrap();
        assert_eq!(reopened.read_log_entries().unwrap(), ["ordinary 雪"]);
        reopened.append_log_entry("following").unwrap();
        reopened.flush_log().unwrap();
        reopened.into_device().close().unwrap();
        let selected = dir.path().join("checkpoint");
        let mut checkpoint = FileCheckpointPagedLog::open(&selected, mirrored).unwrap();
        checkpoint
            .replace_log_entries(&["selected".into()])
            .unwrap();
        checkpoint.close().unwrap();
        assert!(PagedLog::open(FilePageDevice::open(&selected).unwrap()).is_err());
        let mut again = FileCheckpointPagedLog::open(&selected, mirrored).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["selected"]);
        again.close().unwrap();
    }
}

#[test]
fn checkpoint_factory_owns_every_fixed_device_and_never_flushes_on_close() {
    for mirrored in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut log = FileCheckpointPagedLog::with_max_pages(&path, mirrored, 64).unwrap();
        assert!(log.supports_durable_flush());
        assert!(log.supports_replacement());
        for suffix in if mirrored {
            vec!["", ".bank0", ".bank0.mirror", ".bank1", ".bank1.mirror"]
        } else {
            vec!["", ".bank0", ".bank1"]
        } {
            assert!(FilePageDevice::open(format!("{}{suffix}", path.display())).is_err());
        }
        assert!(log
            .replace_log_entries(&["acknowledged 雪".into()])
            .unwrap());
        assert!(!log
            .replace_log_entries(&["acknowledged 雪".into()])
            .unwrap());
        log.append_log_entry("private pending").unwrap();
        log.close().unwrap();
        log.close().unwrap();
        assert!(log.epoch().is_err());
        assert!(log.read_log_entries().is_err());
        assert!(log.append_log_entry("closed").is_err());
        assert!(log.flush_log().is_err());
        assert!(log.replace_log_entries(&[]).is_err());
        let mut reopened = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(reopened.read_log_entries().unwrap(), ["acknowledged 雪"]);
        reopened.append_log_entry("following").unwrap();
        reopened.flush_log().unwrap();
        assert!(reopened.replace_log_entries(&["compacted".into()]).unwrap());
        reopened.close().unwrap();
        let mut again = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["compacted"]);
        again.close().unwrap();
    }
}

#[test]
fn every_native_fence_is_acquired_before_selected_bank_repair_or_reset() {
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    fn suffix(path: &Path, suffix: &str) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        name.into()
    }
    for mirrored in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let mut initial = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        initial.replace_log_entries(&["previous".into()]).unwrap();
        initial
            .replace_log_entries(&["acknowledged 雪".into()])
            .unwrap();
        let selected = suffix(&path, &format!(".bank{}", initial.epoch().unwrap() % 2));
        initial.close().unwrap();
        if mirrored {
            fs::OpenOptions::new()
                .write(true)
                .open(&selected)
                .unwrap()
                .set_len(19)
                .unwrap();
            fs::write(suffix(&selected, ".meta0"), b"torn").unwrap();
        }
        let mut devices = vec![path.clone()];
        for bank in 0..2 {
            let primary = suffix(&path, &format!(".bank{bank}"));
            devices.push(primary.clone());
            if mirrored {
                devices.push(suffix(&primary, ".mirror"));
            }
        }
        let files: Vec<_> = devices
            .iter()
            .flat_map(|name| [name.clone(), suffix(name, ".meta0"), suffix(name, ".meta1")])
            .collect();
        let before: Vec<_> = files.iter().map(|name| fs::read(name).unwrap()).collect();
        for foreign in &devices {
            fs::write(suffix(foreign, ".writer"), b"foreign owner").unwrap();
            assert!(FileCheckpointPagedLog::open(&path, mirrored).is_err());
            assert_eq!(
                files
                    .iter()
                    .map(|name| fs::read(name).unwrap())
                    .collect::<Vec<_>>(),
                before
            );
            for name in &devices {
                assert_eq!(suffix(name, ".writer").exists(), name == foreign);
            }
            assert_eq!(
                fs::read(suffix(foreign, ".writer")).unwrap(),
                b"foreign owner"
            );
            fs::remove_file(suffix(foreign, ".writer")).unwrap();
        }
        let mut recovered = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged 雪"]);
        if mirrored {
            assert_eq!(
                fs::read(&selected).unwrap(),
                fs::read(suffix(&selected, ".mirror")).unwrap()
            );
        }
        recovered.append_log_entry("following").unwrap();
        recovered.flush_log().unwrap();
        recovered
            .replace_log_entries(&["compacted".into()])
            .unwrap();
        recovered.close().unwrap();
        let mut again = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["compacted"]);
        again.close().unwrap();
    }
}
