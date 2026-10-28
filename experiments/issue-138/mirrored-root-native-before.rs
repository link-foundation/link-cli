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
        initial.replace_log_entries(&["acknowledged 雪".into()]).unwrap();
        initial.close().unwrap();
        let sidecar = |suffix: &str| path.with_file_name(format!("journal{suffix}"));
        assert!(sidecar(".mirror.meta0").exists());
        std::fs::write(sidecar(".meta0"), "torn").unwrap();
        let mut recovered = FileCheckpointPagedLog::with_options(&path, options).unwrap();
        assert_eq!(recovered.epoch().unwrap(), 2);
        assert_eq!(recovered.read_log_entries().unwrap(), ["acknowledged 雪"]);
        recovered.append_log_entry("following").unwrap();
        recovered.flush_log().unwrap();
        recovered.replace_log_entries(&["next".into(), "following".into()]).unwrap();
        recovered.close().unwrap();
        assert_eq!(std::fs::read(sidecar(".meta1")).unwrap(), std::fs::read(sidecar(".mirror.meta1")).unwrap());
        let mut again = FileCheckpointPagedLog::with_options(&path, options).unwrap();
        assert_eq!(again.read_log_entries().unwrap(), ["next", "following"]);
        again.close().unwrap();
    }
}
