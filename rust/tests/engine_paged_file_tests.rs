#![cfg(all(feature = "native-pages", not(target_arch = "wasm32")))]
//! Real files, independent vectors, ownership, and bounded child exits.
use link_cli::engine::paged::{FilePageDevice, PagedLog};
use link_cli::{LinkError, TransitionLogStore};
use std::path::Path;

fn open(path: &Path) -> Result<PagedLog<FilePageDevice>, LinkError> {
    PagedLog::open(FilePageDevice::open(path)?)
}
#[test]
fn real_files_reopen_exact_names_and_exclude_writers() -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log");
    let expected = vec![String::new(), "雪".repeat(700), "\u{feff}\0😀".into()];
    let mut first = open(&path)?;
    assert!(open(&path).is_err());
    for value in &expected {
        first.append_log_entry(value)?;
    }
    first.flush_log()?;
    first.into_device().close()?;
    let mut reopened = open(&path)?;
    assert_eq!(reopened.read_log_entries()?, expected);
    reopened.append_log_entry("next")?;
    reopened.flush_log()?;
    drop(reopened);
    assert_eq!(
        open(&path)?.read_log_entries()?,
        [expected, vec!["next".into()]].concat()
    );
    Ok(())
}
#[test]
fn invalid_open_preserves_files_and_releases_only_its_owner() -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log");
    std::fs::write(&path, b"unrelated file")?;
    assert!(open(&path).is_err());
    assert_eq!(std::fs::read(&path)?, b"unrelated file");
    assert!(!dir.path().join("log.writer").exists());
    assert!(open(&dir.path().join("missing/log")).is_err());
    Ok(())
}
#[test]
fn independent_file_vectors_open_without_rewriting_committed_pages() -> Result<(), LinkError> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/protocol/native-pages-v1.json")).unwrap();
    let hex = |s: &str| {
        s.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    for name in ["empty", "oneGroup", "twoGroups"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("log");
        let fixture = &value[name];
        let data: Vec<u8> = fixture["pages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|v| hex(v.as_str().unwrap()))
            .collect();
        std::fs::write(&path, &data)?;
        for slot in 0..2 {
            std::fs::write(
                dir.path().join(format!("log.meta{slot}")),
                hex(fixture["metadata"][slot].as_str().unwrap()),
            )?;
        }
        let mut log = open(&path)?;
        let expected: Vec<String> = fixture["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        assert_eq!(log.read_log_entries()?, expected);
        assert_eq!(std::fs::read(&path)?, data);
        log.append_log_entry("following")?;
        log.flush_log()?;
        drop(log);
        assert_eq!(
            open(&path)?.read_log_entries()?,
            [expected, vec!["following".into()]].concat()
        );
    }
    Ok(())
}

#[test]
fn uncommitted_partial_tail_repairs_but_committed_corruption_rejects() -> Result<(), LinkError> {
    use std::io::{Seek, SeekFrom, Write};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log");
    let mut log = open(&path)?;
    log.append_log_entry("acknowledged")?;
    log.flush_log()?;
    drop(log);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)?
        .write_all(b"unpublished partial page")?;
    let mut log = open(&path)?;
    assert_eq!(log.read_log_entries()?, ["acknowledged"]);
    assert_eq!(std::fs::metadata(&path)?.len(), 4096);
    log.append_log_entry("following")?;
    log.flush_log()?;
    drop(log);
    let mut file = std::fs::OpenOptions::new().write(true).open(&path)?;
    file.seek(SeekFrom::Start(4096 + 64))?;
    file.write_all(&[9])?;
    file.sync_all()?;
    drop(file);
    let before = std::fs::read(&path)?;
    assert!(open(&path).is_err());
    assert_eq!(std::fs::read(&path)?, before);
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
            .args(["--exact", "page_exit_child", "--ignored", "--nocapture"])
            .env("CLINK_PAGE_EXIT_PATH", &path)
            .env("CLINK_PAGE_EXIT_PHASE", phase)
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
fn page_exit_child() {
    let path = std::env::var_os("CLINK_PAGE_EXIT_PATH").expect("child-only path");
    let mut log = open(Path::new(&path)).unwrap();
    log.append_log_entry("pending 雪").unwrap();
    if std::env::var("CLINK_PAGE_EXIT_PHASE").unwrap() == "flush" {
        log.flush_log().unwrap();
    }
    std::process::exit(73);
}
