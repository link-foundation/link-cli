//! Finite real-component faults at private SQL boundaries.
use super::*;

#[test]
fn errors_before_and_after_actual_commit_resolve_only_the_committed_prefix() {
    for after in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("commit.db");
        let mut log = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
        log.append_log_entry("before").unwrap();
        log.flush_log().unwrap();
        log.append_log_entry("uncertain").unwrap();
        let result = log.flush_with(|connection| {
            if after {
                connection.execute_batch("COMMIT").map_err(error)?;
            }
            Err(error("injected commit failure"))
        });
        assert!(result.unwrap_err().to_string().contains("injected"));
        assert!(log.read_log_entries().is_err());
        assert!(log.append_log_entry("retry").is_err());
        assert!(log.flush_log().is_err());
        assert!(log.checkpoint(CheckpointMode::Passive).is_err());
        drop(log);
        let mut reopened = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
        assert_eq!(
            reopened.read_log_entries().unwrap(),
            if after {
                vec!["before", "uncertain"]
            } else {
                vec!["before"]
            }
        );
        reopened.append_log_entry("following").unwrap();
        reopened.flush_log().unwrap();
    }
}
#[test]
fn finite_sqlite_full_rolls_uncommitted_insert_back_without_losing_previous_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("full.db");
    let mut log = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
    log.append_log_entry("durable").unwrap();
    log.flush_log().unwrap();
    let pages = log.scalar("PRAGMA page_count").unwrap();
    log.connection
        .execute_batch(&format!("PRAGMA max_page_count={pages}"))
        .unwrap();
    let failure = log.append_log_entry(&"x".repeat(256 * 1024)).unwrap_err();
    assert!(failure.to_string().contains("full"));
    assert!(log.flush_log().is_err());
    drop(log);
    let mut reopened = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
    assert_eq!(reopened.read_log_entries().unwrap(), ["durable"]);
    let check: String = reopened
        .connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    reopened.append_log_entry("following").unwrap();
    reopened.flush_log().unwrap();
}
#[test]
fn settings_drift_exhaustion_and_invalid_options_do_not_publish() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.db");
    let mut log = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
    log.tail = i64::MAX;
    assert!(log
        .append_log_entry("exhausted")
        .unwrap_err()
        .to_string()
        .contains("exhausted"));
    log.flush_log().unwrap();
    assert_eq!(log.checkpoint(CheckpointMode::Passive).unwrap().busy, 0);
    log.tail = 0;
    log.connection
        .execute_batch("PRAGMA synchronous=NORMAL")
        .unwrap();
    assert!(log
        .append_log_entry("unsafe")
        .unwrap_err()
        .to_string()
        .contains("settings"));
    assert!(log.flush_log().is_err());
    drop(log);
    let mut reopened = SqliteLog::open(&path, SqliteOptions::default()).unwrap();
    assert!(reopened.read_log_entries().unwrap().is_empty());
    for options in [
        SqliteOptions {
            busy_timeout_ms: u32::MAX,
            ..Default::default()
        },
        SqliteOptions {
            auto_checkpoint_pages: u32::MAX,
            ..Default::default()
        },
    ] {
        assert!(SqliteLog::open(&path, options).is_err());
    }
    assert!(SqliteLog::open(":memory:", SqliteOptions::default()).is_err());
}

#[test]
fn copied_wal_torn_frame_and_checksum_damage_recover_the_prior_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.db");
    let mut log = SqliteLog::open(
        &path,
        SqliteOptions {
            auto_checkpoint_pages: 0,
            ..Default::default()
        },
    )
    .unwrap();
    log.append_log_entry("checkpointed").unwrap();
    log.flush_log().unwrap();
    log.checkpoint(CheckpointMode::Truncate).unwrap();
    log.append_log_entry("new committed frame").unwrap();
    log.flush_log().unwrap();
    let wal = std::fs::read(path.with_extension("db-wal")).unwrap();
    assert!(wal.len() >= 32 + 24);
    let page_size = u32::from_be_bytes(wal[8..12].try_into().unwrap()) as usize;
    assert!((512..=65536).contains(&page_size));
    let frame_size = 24 + page_size;
    assert_eq!((wal.len() - 32) % frame_size, 0);
    let frame = wal.len() - frame_size;
    assert_ne!(
        &wal[frame + 4..frame + 8],
        &[0; 4],
        "last frame marks a commit"
    );
    for damage in ["control", "torn", "checksum"] {
        let copy = directory.path().join(format!("{damage}.db"));
        std::fs::copy(&path, &copy).unwrap();
        let mut bytes = wal.clone();
        match damage {
            "torn" => bytes.truncate(bytes.len() - page_size / 2),
            "checksum" => bytes[frame + 24] ^= 1,
            _ => {}
        }
        std::fs::write(copy.with_extension("db-wal"), bytes).unwrap();
        let mut recovered = SqliteLog::open(&copy, SqliteOptions::default()).unwrap();
        assert_eq!(
            recovered.read_log_entries().unwrap(),
            if damage == "control" {
                vec!["checkpointed", "new committed frame"]
            } else {
                vec!["checkpointed"]
            }
        );
        recovered.append_log_entry("following").unwrap();
        recovered.flush_log().unwrap();
        let check: String = recovered
            .connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(check, "ok");
    }
}
