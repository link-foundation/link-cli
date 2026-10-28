//! First-authored contracts for the optional physical SQLite journal.
#![cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
use link_cli::engine::sqlite::{CheckpointMode, SqliteLog, SqliteOptions};
use link_cli::engine::{
    Engine, EngineOptions, HistoryPolicy, IsolationLevel, RevisionEncoding, StateStructure,
};
use link_cli::{LinksStorage, TransitionLogStore};
use rusqlite::{params, Connection};

fn open(path: &std::path::Path) -> SqliteLog {
    SqliteLog::open(path, SqliteOptions::default()).unwrap()
}
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("../../docs/protocol/sqlite-log-vectors.json")).unwrap()
}
#[test]
fn blobs_preserve_independent_bytes_and_close_rolls_pending_records_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("log.db");
    let fixtures = vectors();
    let expected: Vec<_> = fixtures["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["text"].as_str().unwrap().to_owned())
        .collect();
    let mut log = open(&path);
    let reader = Connection::open(&path).unwrap();
    assert!(log.supports_durable_flush());
    for record in &expected {
        log.append_log_entry(record).unwrap();
    }
    assert_eq!(log.read_log_entries().unwrap(), expected);
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM link_cli_log", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    for row in fixtures["invalidRecords"].as_array().unwrap() {
        assert!(log.append_log_entry(row.as_str().unwrap()).is_err());
    }
    log.flush_log().unwrap();
    let actual: Vec<Vec<u8>> = reader
        .prepare("SELECT record FROM link_cli_log ORDER BY sequence")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for (bytes, row) in actual.iter().zip(fixtures["records"].as_array().unwrap()) {
        assert_eq!(
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            row["utf8Hex"].as_str().unwrap()
        );
    }
    log.append_log_entry("unflushed").unwrap();
    drop(log);
    let mut reopened = open(&path);
    assert_eq!(reopened.read_log_entries().unwrap(), expected);
    reopened.append_log_entry("following").unwrap();
    reopened.flush_log().unwrap();
}
#[test]
fn stale_owners_and_busy_writers_are_fenced_until_reopen() {
    for read in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("log.db");
        let mut winner = open(&path);
        let mut stale = open(&path);
        winner.append_log_entry("winner").unwrap();
        winner.flush_log().unwrap();
        let error = if read {
            stale.read_log_entries().map(|_| ())
        } else {
            stale.append_log_entry("stale")
        }
        .unwrap_err();
        assert!(error.to_string().contains("stale"));
        assert!(stale
            .read_log_entries()
            .unwrap_err()
            .to_string()
            .contains("indeterminate"));
        assert!(stale.flush_log().is_err());
        assert!(stale.append_log_entry("late").is_err());
        drop(stale);
        drop(winner);
        assert_eq!(open(&path).read_log_entries().unwrap(), ["winner"]);
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("log.db");
    let mut winner = open(&path);
    let mut busy = open(&path);
    winner.append_log_entry("winner").unwrap();
    assert!(winner
        .checkpoint(CheckpointMode::Passive)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    assert!(busy.append_log_entry("busy").is_err());
    assert!(busy.flush_log().is_err());
    winner.flush_log().unwrap();
    assert_eq!(winner.read_log_entries().unwrap(), ["winner"]);
}
#[test]
fn checkpoint_reports_pinned_reader_and_truncates_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("log.db");
    let mut log = SqliteLog::open(
        &path,
        SqliteOptions {
            auto_checkpoint_pages: 0,
            ..Default::default()
        },
    )
    .unwrap();
    log.append_log_entry("one").unwrap();
    log.flush_log().unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM link_cli_log", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    log.append_log_entry("two").unwrap();
    log.flush_log().unwrap();
    let blocked = log.checkpoint(CheckpointMode::Truncate).unwrap();
    assert_eq!(blocked.busy, 1);
    assert!(blocked.log_frames > blocked.checkpointed_frames);
    reader.execute_batch("COMMIT").unwrap();
    let done = log.checkpoint(CheckpointMode::Truncate).unwrap();
    assert_eq!(done.busy, 0);
    assert_eq!(done.log_frames, 0);
    for mode in [
        CheckpointMode::Passive,
        CheckpointMode::Full,
        CheckpointMode::Restart,
        CheckpointMode::Truncate,
    ] {
        assert_eq!(log.checkpoint(mode).unwrap().busy, 0);
    }
}
#[test]
fn foreign_schema_sequence_holes_and_invalid_utf8_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("foreign.db");
    let foreign = Connection::open(&path).unwrap();
    foreign
        .execute_batch("CREATE TABLE keep(value TEXT); INSERT INTO keep VALUES ('keep')")
        .unwrap();
    assert!(SqliteLog::open(&path, SqliteOptions::default()).is_err());
    assert_eq!(
        foreign
            .query_row("SELECT value FROM keep", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    for sequence in [2, 3] {
        let path = directory.path().join(format!("hole-{sequence}.db"));
        let mut log = open(&path);
        log.append_log_entry("one").unwrap();
        log.flush_log().unwrap();
        drop(log);
        let db = Connection::open(&path).unwrap();
        if sequence == 2 {
            db.execute("DELETE FROM link_cli_log", []).unwrap();
        }
        db.execute(
            "INSERT INTO link_cli_log VALUES (?1, ?2)",
            params![sequence, b"hole".as_slice()],
        )
        .unwrap();
        assert!(SqliteLog::open(&path, SqliteOptions::default()).is_err());
    }
    for (i, hex) in vectors()["invalidUtf8Hex"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let path = directory.path().join(format!("utf8-{i}.db"));
        drop(open(&path));
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO link_cli_log VALUES (1, x'{}')",
            hex.as_str().unwrap()
        ))
        .unwrap();
        let mut log = open(&path);
        assert!(log.read_log_entries().is_err());
        assert!(log.flush_log().is_err());
    }
}
#[test]
fn engine_groups_and_pins_reopen_across_maps_and_encodings() {
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("engine.db");
            let options = EngineOptions {
                state_structure: structure,
                revision_encoding: encoding,
                history: HistoryPolicy::All,
                isolation: IsolationLevel::Snapshot,
                ..Default::default()
            };
            let db = Engine::<u64, _>::open(open(&path), options.clone()).unwrap();
            let pinned = db.snapshot().unwrap();
            let mut a = db.begin().unwrap();
            let mut b = db.begin().unwrap();
            a.ensure_link_created(1).unwrap();
            a.set_name(1, Some("one 雪\0😀".into())).unwrap();
            b.ensure_link_created(2).unwrap();
            assert_eq!(db.commit_group(vec![a, b]).unwrap().len(), 2);
            assert!(pinned.rows().is_empty());
            drop(db);
            let reopened = Engine::<u64, _>::open(open(&path), options).unwrap();
            assert_eq!(reopened.snapshot().unwrap().rows().len(), 2);
            assert_eq!(
                reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                Some("one 雪\0😀")
            );
            let mut following = reopened.begin().unwrap();
            assert_eq!(following.create_link(0, 0).unwrap(), 3);
            following.commit().unwrap();
        }
    }
}

#[test]
fn sqlite_replication_and_physical_delta_preserve_exact_following_commits() {
    use link_cli::engine::replication::{ReplicatedLog, Synchronization};
    use link_cli::engine::DeltaLog;
    fn commit<L: TransitionLogStore>(log: L) {
        let db = Engine::<u64, _>::open(
            log,
            EngineOptions {
                revision_encoding: RevisionEncoding::Changes,
                require_durable_flush: true,
                ..Default::default()
            },
        )
        .unwrap();
        let count = db.snapshot().unwrap().rows().len();
        let mut tx = db.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        tx.commit().unwrap();
        assert_eq!(db.snapshot().unwrap().rows().len(), count + 1);
    }
    for delta in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let primary = directory.path().join("primary.db");
        let follower = directory.path().join("follower.db");
        let log = ReplicatedLog::new(
            open(&primary),
            vec![open(&follower)],
            Synchronization::Synchronous {
                acknowledgements: 2,
            },
        )
        .unwrap();
        if delta {
            commit(DeltaLog::new(log, 128).unwrap());
        } else {
            commit(log);
        }
        assert_eq!(
            open(&primary).read_log_entries().unwrap(),
            open(&follower).read_log_entries().unwrap()
        );
        for path in [&primary, &follower] {
            if delta {
                commit(DeltaLog::new(open(path), 128).unwrap());
            } else {
                commit(open(path));
            }
        }
        assert_eq!(
            open(&primary).read_log_entries().unwrap(),
            open(&follower).read_log_entries().unwrap()
        );
    }
}

#[test]
fn every_independent_invalid_schema_is_rejected_without_claiming_it() {
    let directory = tempfile::tempdir().unwrap();
    for (index, schema) in vectors()["invalidSchemas"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let path = directory.path().join(format!("schema-{index}.db"));
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!(
            "{}; PRAGMA application_id=1280199498; PRAGMA user_version=1",
            schema.as_str().unwrap()
        ))
        .unwrap();
        assert!(SqliteLog::open(&path, SqliteOptions::default()).is_err());
        assert_eq!(
            db.query_row(
                "SELECT sql FROM sqlite_schema WHERE name='link_cli_log'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            schema.as_str().unwrap()
        );
    }
}
