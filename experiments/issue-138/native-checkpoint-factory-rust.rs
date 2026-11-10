//! Native ownership control, copied into the actual test target before implementation.
#![cfg(all(feature = "native-pages", any(unix, windows)))]
use link_cli::engine::paged::{FileCheckpointPagedLog, FilePageDevice};
use link_cli::{ReplaceableTransitionLogStore, TransitionLogStore};

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
        } else { vec!["", ".bank0", ".bank1"] } {
            assert!(FilePageDevice::open(format!("{}{suffix}", path.display())).is_err());
        }
        assert!(log.replace_log_entries(&["acknowledged 雪".into()]).unwrap());
        assert!(!log.replace_log_entries(&["acknowledged 雪".into()]).unwrap());
        log.append_log_entry("private pending").unwrap();
        log.close().unwrap(); log.close().unwrap();
        assert!(log.read_log_entries().is_err());
        assert!(log.append_log_entry("closed").is_err());
        assert!(log.flush_log().is_err());
        assert!(log.replace_log_entries(&[]).is_err());
        let mut reopened = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(reopened.read_log_entries().unwrap(), ["acknowledged 雪"]);
        reopened.append_log_entry("following").unwrap(); reopened.flush_log().unwrap();
        assert!(reopened.replace_log_entries(&["compacted".into()]).unwrap());
        reopened.close().unwrap();
        let mut again = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["compacted"]);
        again.close().unwrap();
    }
}
