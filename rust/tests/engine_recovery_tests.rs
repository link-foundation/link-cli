//! Damaged journals: what `check_journal` reports, what `recover_journal`
//! keeps, and that a repaired file journal opens at the recovered revision.
use link_cli::engine::recovery::{check_journal, recover_journal};
use link_cli::engine::{Constraint, Engine, EngineOptions, MemoryLog, RevisionEncoding};
use link_cli::LinksStorage;

/// Revision k holds links 1..=k, each pointing at itself.
fn journal(commits: u64, encoding: RevisionEncoding) -> Vec<String> {
    let options = EngineOptions {
        revision_encoding: encoding,
        ..EngineOptions::default()
    };
    let db = Engine::<u64, _>::open(MemoryLog::default(), options).unwrap();
    for _ in 0..commits {
        let mut tx = db.begin().unwrap();
        let id = tx.create_link(0, 0).unwrap();
        tx.update_link(id, id, id).unwrap();
        tx.commit().unwrap();
    }
    db.into_log().ok().unwrap().entries
}

fn expected(revision: u64) -> Vec<(u64, u64, u64)> {
    (1..=revision).map(|id| (id, id, id)).collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_intact_journal_reports_every_record() {
    for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
        let entries = journal(4, encoding);
        let check = check_journal::<u64>(&entries, &EngineOptions::default());
        assert!(check.is_intact());
        assert_eq!(
            (check.valid_records, check.total_records, check.revision),
            (4, 4, 4)
        );
        assert_eq!(check.discarded_records(), 0);
    }
    let empty = check_journal::<u64>(&[], &EngineOptions::default());
    assert!(empty.is_intact());
    assert_eq!(empty.revision, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn damage_anywhere_keeps_exactly_the_records_before_it() {
    for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
        let entries = journal(3, encoding);
        for record in 0..entries.len() {
            for position in 0..entries[record].len() {
                let mut damaged = entries.clone();
                let mut bytes = damaged[record].clone().into_bytes();
                bytes[position] ^= 0x01;
                damaged[record] = String::from_utf8(bytes).unwrap();
                let check = check_journal::<u64>(&damaged, &EngineOptions::default());
                let damage = check.damage.as_ref().unwrap_or_else(|| {
                    panic!("{encoding:?} flip at {record}:{position} went unnoticed")
                });
                assert_eq!(damage.record, record);
                assert_eq!(check.valid_records, record);
                assert_eq!(check.revision, record as u64);
                assert_eq!(check.discarded_records(), entries.len() - record);
                let recovery =
                    recover_journal::<u64>(&[damaged], &EngineOptions::default()).unwrap();
                let db = Engine::<u64, _>::open(
                    MemoryLog {
                        entries: recovery.entries,
                    },
                    EngineOptions::default(),
                )
                .unwrap();
                let links = db
                    .snapshot()
                    .unwrap()
                    .links()
                    .into_iter()
                    .map(|link| (link.index, link.source, link.target))
                    .collect::<Vec<_>>();
                assert_eq!(links, expected(record as u64));
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_reordered_and_foreign_records_end_the_valid_prefix() {
    let entries = journal(4, RevisionEncoding::Snapshot);
    let options = EngineOptions::default();
    let mut missing = entries.clone();
    missing.remove(1);
    let check = check_journal::<u64>(&missing, &options);
    assert_eq!((check.valid_records, check.revision), (1, 1));
    assert!(check.damage.unwrap().reason.contains("contiguous"));
    let mut swapped = entries.clone();
    swapped.swap(2, 3);
    assert_eq!(check_journal::<u64>(&swapped, &options).valid_records, 2);
    let mut foreign = entries.clone();
    foreign.insert(2, "not a journal record".into());
    let check = check_journal::<u64>(&foreign, &options);
    assert_eq!((check.valid_records, check.total_records), (2, 5));
    // A wider-address record is refused by a narrower reader, not truncated.
    let wide = Engine::<u64, _>::open(MemoryLog::default(), options.clone()).unwrap();
    let mut tx = wide.begin().unwrap();
    let id = tx.create_link(0, 0).unwrap();
    tx.update_link(id, u64::from(u32::MAX) + 1, id).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let wide = wide.into_log().ok().unwrap().entries;
    let check = check_journal::<u32>(&wide, &options);
    assert_eq!(check.valid_records, 0);
    assert!(check_journal::<u64>(&wide, &options).is_intact());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_constraints_are_part_of_the_check() {
    let db = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = db.begin().unwrap();
    tx.create_link(5, 5).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let entries = db.into_log().ok().unwrap().entries;
    assert!(check_journal::<u64>(&entries, &EngineOptions::default()).is_intact());
    let strict = EngineOptions {
        constraints: vec![Constraint::References],
        ..EngineOptions::default()
    };
    let check = check_journal::<u64>(&entries, &strict);
    assert_eq!(check.valid_records, 0);
    assert!(check.damage.unwrap().reason.contains("dangling"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_checkpoint_is_only_accepted_first() {
    let db = Engine::<u64, _>::open(
        MemoryLog {
            entries: journal(3, RevisionEncoding::Snapshot),
        },
        EngineOptions::default(),
    )
    .unwrap();
    let checkpoint = db.export_checkpoint().unwrap();
    assert!(checkpoint[0].starts_with("engine3|"));
    let check = check_journal::<u64>(&checkpoint, &EngineOptions::default());
    assert_eq!((check.valid_records, check.revision), (1, 3));
    let mut late = journal(3, RevisionEncoding::Snapshot);
    late.push(checkpoint[0].clone());
    assert_eq!(
        check_journal::<u64>(&late, &EngineOptions::default()).valid_records,
        3
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_intact_copy_restores_what_the_damaged_one_lost() {
    let entries = journal(5, RevisionEncoding::Changes);
    let mut damaged = entries.clone();
    damaged[2].replace_range(10..12, "zz");
    let lagging = entries[..3].to_vec();
    let options = EngineOptions::default();
    let recovery = recover_journal::<u64>(
        &[damaged.clone(), lagging.clone(), entries.clone()],
        &options,
    )
    .unwrap();
    assert_eq!(recovery.source, 2);
    assert_eq!(recovery.revision, 5);
    assert_eq!(recovery.entries, entries);
    assert_eq!(
        recovery
            .checks
            .iter()
            .map(|check| check.valid_records)
            .collect::<Vec<_>>(),
        vec![2, 3, 5]
    );
    // Ties keep the earliest copy, normally the primary.
    let tie = recover_journal::<u64>(&[lagging.clone(), lagging], &options).unwrap();
    assert_eq!(tie.source, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn copies_that_disagree_are_refused() {
    let options = EngineOptions::default();
    let first = journal(3, RevisionEncoding::Snapshot);
    let other = {
        let db = Engine::<u64, _>::open(MemoryLog::default(), options.clone()).unwrap();
        for (source, target) in [(0, 0), (1, 1)] {
            let mut tx = db.begin().unwrap();
            tx.create_link(source, target).unwrap();
            tx.commit().unwrap();
        }
        db.into_log().ok().unwrap().entries
    };
    let error = recover_journal::<u64>(&[first, other], &options).unwrap_err();
    assert!(error.to_string().contains("diverge"), "{error}");
    let error = recover_journal::<u64>(&[], &options).unwrap_err();
    assert!(error.to_string().contains("at least one copy"), "{error}");
}

#[cfg(not(target_arch = "wasm32"))]
mod file {
    use super::*;
    use link_cli::engine::recovery::{read_journal_file_lossy, repair_journal_file};
    use link_cli::{FileTransitionLog, TransitionLogStore};
    use std::path::Path;

    fn write(path: &Path, entries: &[String]) {
        let mut text = String::new();
        for entry in entries {
            text.push_str(entry);
            text.push('\n');
        }
        std::fs::write(path, text).unwrap();
    }

    fn open_revision(path: &Path) -> u64 {
        let log = FileTransitionLog::open_exclusive(path).unwrap();
        let db = Engine::<u64, _>::open(log, EngineOptions::default()).unwrap();
        let snapshot = db.snapshot().unwrap();
        let links = snapshot
            .links()
            .into_iter()
            .map(|link| (link.index, link.source, link.target))
            .collect::<Vec<_>>();
        assert_eq!(links, expected(snapshot.revision));
        snapshot.revision
    }

    #[test]
    fn every_damaged_byte_of_a_file_is_repaired_to_its_valid_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("links.journal");
        let entries = journal(3, RevisionEncoding::Changes);
        write(&path, &entries);
        let pristine = std::fs::read(&path).unwrap();
        let mut starts = vec![0usize];
        for (index, byte) in pristine.iter().enumerate() {
            if *byte == b'\n' {
                starts.push(index + 1);
            }
        }
        for position in 0..pristine.len() {
            let mut bytes = pristine.clone();
            bytes[position] = if bytes[position] == 0xff { 0 } else { 0xff };
            std::fs::write(&path, &bytes).unwrap();
            // The final newline turns the last record into a torn tail, which
            // opening drops by design; every other byte is refused.
            if position + 1 < pristine.len() {
                let refused = FileTransitionLog::open(&path).and_then(|log| {
                    Engine::<u64, _>::open(log, EngineOptions::default()).map(|_| ())
                });
                assert!(refused.is_err(), "damage at {position} opened");
            }
            let repair = repair_journal_file::<u64>(&path, &[], &EngineOptions::default()).unwrap();
            let line = starts.iter().filter(|start| **start <= position).count() as u64 - 1;
            assert_eq!(repair.recovery.revision, line, "damage at {position}");
            assert!(repair.changed);
            let preserved = repair.preserved.unwrap();
            assert_eq!(std::fs::read(&preserved).unwrap(), bytes);
            std::fs::remove_file(preserved).unwrap();
            assert_eq!(open_revision(&path), line);
        }
    }

    #[test]
    fn a_replica_file_restores_the_lost_suffix_and_evidence_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("primary.journal");
        let replica = directory.path().join("replica.journal");
        let entries = journal(4, RevisionEncoding::Snapshot);
        write(&path, &entries);
        write(&replica, &entries);
        // A zeroed block, as a lost page write leaves behind.
        let mut bytes = std::fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle..middle + 24].fill(0);
        std::fs::write(&path, &bytes).unwrap();
        let first =
            repair_journal_file::<u64>(&path, &[&replica], &EngineOptions::default()).unwrap();
        assert_eq!((first.recovery.source, first.recovery.revision), (1, 4));
        assert_eq!(
            first.preserved.as_deref(),
            Some(directory.path().join("primary.journal.damaged-1").as_path())
        );
        assert_eq!(open_revision(&path), 4);
        // A second damage does not overwrite the first evidence.
        std::fs::write(&path, b"garbage\n").unwrap();
        let second =
            repair_journal_file::<u64>(&path, &[&replica], &EngineOptions::default()).unwrap();
        assert_eq!(
            second.preserved.as_deref(),
            Some(directory.path().join("primary.journal.damaged-2").as_path())
        );
        assert_eq!(
            std::fs::read(directory.path().join("primary.journal.damaged-1")).unwrap(),
            bytes
        );
        // An intact journal is left alone.
        let again =
            repair_journal_file::<u64>(&path, &[&replica], &EngineOptions::default()).unwrap();
        assert!(!again.changed && again.preserved.is_none());
    }

    #[test]
    fn torn_tails_invalid_utf8_and_missing_files_are_read_leniently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("links.journal");
        assert!(read_journal_file_lossy(&path).unwrap().is_empty());
        let missing = repair_journal_file::<u64>(&path, &[], &EngineOptions::default()).unwrap();
        assert!(!missing.changed && missing.recovery.entries.is_empty());
        let entries = journal(2, RevisionEncoding::Snapshot);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(entries[0].as_bytes());
        bytes.extend_from_slice(b"\n\n");
        bytes.extend_from_slice(entries[1].as_bytes());
        bytes.extend_from_slice(b"\n\xc3\x28 torn");
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read_journal_file_lossy(&path).unwrap(), entries);
        let repair = repair_journal_file::<u64>(&path, &[], &EngineOptions::default()).unwrap();
        assert!(repair.recovery.checks[0].is_intact());
        assert!(repair.changed, "the blank line and torn tail are rewritten");
        assert_eq!(open_revision(&path), 2);
        // Invalid UTF-8 inside a complete record is damage, not a read error.
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[entries[0].len() + 3] = 0xc3;
        std::fs::write(&path, &bytes).unwrap();
        assert!(FileTransitionLog::open(&path)
            .unwrap()
            .read_log_entries()
            .is_err());
        let repair = repair_journal_file::<u64>(&path, &[], &EngineOptions::default()).unwrap();
        assert_eq!(repair.recovery.revision, 1);
    }

    #[test]
    fn a_running_writer_blocks_repair() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("links.journal");
        let _writer = FileTransitionLog::open_exclusive(&path).unwrap();
        let error = repair_journal_file::<u64>(&path, &[], &EngineOptions::default()).unwrap_err();
        assert!(error.to_string().contains("busy"), "{error}");
    }

    #[test]
    fn divergent_file_copies_leave_the_journal_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("links.journal");
        let other = directory.path().join("other.journal");
        write(&path, &journal(2, RevisionEncoding::Snapshot));
        write(&other, &journal(2, RevisionEncoding::Changes));
        let before = std::fs::read(&path).unwrap();
        assert!(repair_journal_file::<u64>(&path, &[&other], &EngineOptions::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
