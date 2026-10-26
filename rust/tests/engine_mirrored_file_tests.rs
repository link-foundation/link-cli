#![cfg(all(feature = "native-pages", not(target_arch = "wasm32")))]
//! Real files, independent vectors, ownership, and bounded child exits.
use link_cli::engine::paged::{FilePageDevice, MirroredPageDevice, PagedLog, RepairPageDevice};
use link_cli::{LinkError, TransitionLogStore};
use std::path::Path;

fn open(path: &Path) -> Result<PagedLog<MirroredPageDevice<FilePageDevice>>, LinkError> {
    PagedLog::open(MirroredPageDevice::open(path)?)
}
#[test]
fn redundant_native_repair_preserves_exact_records_and_mode() -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log");
    let mut log = open(&path)?;
    let record = "雪".repeat(2100);
    log.append_log_entry(&record)?;
    log.flush_log()?;
    log.append_log_entry("\u{feff}\0😀")?;
    log.flush_log()?;
    assert!(open(&path).is_err());
    log.into_device().close()?;
    let healthy = std::fs::read(&path)?;
    std::fs::write(&path, &healthy[..19])?;
    std::fs::write(dir.path().join("log.meta0"), [0; 17])?;
    let mut log = open(&path)?;
    assert_eq!(
        log.read_log_entries()?,
        [record.clone(), "\u{feff}\0😀".into()]
    );
    log.append_log_entry("next")?;
    log.flush_log()?;
    log.into_device().close()?;
    assert_eq!(
        std::fs::read(&path)?,
        std::fs::read(dir.path().join("log.mirror"))?
    );
    assert!(PagedLog::open(FilePageDevice::open(&path)?).is_err());
    let mut device = FilePageDevice::open(&path)?;
    let prior = std::fs::read(&path)?;
    assert!(device.repair_page(u64::MAX, &[0; 4096]).is_err());
    assert!(device.repair_page(99, &[0; 4096]).is_err());
    assert_eq!(std::fs::read(&path)?, prior);
    device.close()?;
    assert!(device.repair_page(0, &[0; 4096]).is_err());
    Ok(())
}
#[test]
fn failed_secondary_acquisition_releases_only_its_own_primary() -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log");
    std::fs::write(dir.path().join("log.mirror.writer"), b"other owner")?;
    assert!(open(&path).is_err());
    assert!(!dir.path().join("log.writer").exists());
    assert_eq!(
        std::fs::read(dir.path().join("log.mirror.writer"))?,
        b"other owner"
    );
    std::fs::remove_file(dir.path().join("log.mirror.writer"))?;
    std::fs::write(&path, b"unrelated")?;
    assert!(open(&path).is_err());
    assert_eq!(std::fs::read(&path)?, b"unrelated");
    assert!(!dir.path().join("log.writer").exists());
    assert!(!dir.path().join("log.mirror.writer").exists());
    Ok(())
}
#[test]
fn child_process_exits_preserve_acknowledgements_and_require_fencing() -> Result<(), LinkError> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir()?;
    for phase in ["append", "flush"] {
        let path = dir.path().join(phase);
        let mut log = open(&path)?;
        log.append_log_entry("acknowledged")?;
        log.flush_log()?;
        drop(log);
        let output = dir.path().join(format!("{phase}.output"));
        let mut child = Command::new(std::env::current_exe()?)
            .args(["--exact", "mirror_exit_child", "--ignored", "--nocapture"])
            .env("CLINK_MIRROR_EXIT_PATH", &path)
            .env("CLINK_MIRROR_EXIT_PHASE", phase)
            .stdout(std::fs::File::create(&output)?)
            .stderr(Stdio::null())
            .spawn()?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > Duration::from_secs(10) {
                child.kill()?;
                let _ = child.wait();
                panic!("bounded page child exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            status.code(),
            Some(73),
            "{}",
            std::fs::read_to_string(&output)?
        );
        assert!(std::fs::metadata(&output)?.len() < 65536);
        assert!(open(&path).is_err());
        // The observed child exit fences its owner. Never reclaim a live writer.
        std::fs::remove_file(dir.path().join(format!("{phase}.writer")))?;
        std::fs::remove_file(dir.path().join(format!("{phase}.mirror.writer")))?;
        let mut log = open(&path)?;
        let mut expected = vec!["acknowledged".to_owned()];
        if phase == "flush" {
            expected.push("pending 雪".into());
        }
        assert_eq!(log.read_log_entries()?, expected);
        log.append_log_entry("following")?;
        log.flush_log()?;
        drop(log);
        expected.push("following".into());
        assert_eq!(open(&path)?.read_log_entries()?, expected);
    }
    Ok(())
}
#[test]
#[ignore = "bounded child entry; exercised by child_process_exits_preserve_acknowledgements_and_require_fencing"]
fn mirror_exit_child() {
    let path = std::env::var_os("CLINK_MIRROR_EXIT_PATH").expect("child-only path");
    let mut log = open(Path::new(&path)).unwrap();
    log.append_log_entry("pending 雪").unwrap();
    if std::env::var("CLINK_MIRROR_EXIT_PHASE").unwrap() == "flush" {
        log.flush_log().unwrap();
    }
    std::process::exit(73);
}
