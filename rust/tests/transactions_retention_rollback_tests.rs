//! Retention must never replay rolled-back operations or archive them as data.
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LinkStorage, LogRetentionPolicy,
    Transition,
};

fn verify(chunked: bool) {
    for operation in ["create", "update", "delete"] {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let log = directory.path().join("log");
        let archive = directory.path().join("archive");
        let mut store = LinkStorage::new(&data, false).unwrap();
        for id in 1..=3 {
            store.create(id + 10, id + 20);
        }
        store.save().unwrap();
        let before: Vec<_> = store.all().into_iter().copied().collect();
        let policy = if chunked {
            LogRetentionPolicy::Chunked {
                chunk_size: 2,
                archive_directory: archive.clone(),
            }
        } else {
            LogRetentionPolicy::Sized { max_transitions: 1 }
        };
        let mut tx = GenericTransactionsDecorator::new(
            store,
            FileTransitionLog::open(&log).unwrap(),
            policy,
            CommitMode::Sync,
            false,
        )
        .unwrap();
        let aborted = tx.begin_transaction().unwrap().id;
        for id in 1..=3 {
            match operation {
                "create" => {
                    tx.create(id + 100, id + 200).unwrap();
                }
                "update" => {
                    tx.update(id, id + 100, id + 200).unwrap();
                }
                "delete" => {
                    tx.delete(id).unwrap();
                }
                _ => unreachable!(),
            }
        }
        tx.rollback().unwrap();
        let restored: Vec<_> = tx.inner().all().into_iter().copied().collect();
        assert_eq!(restored, before, "{operation}, chunked={chunked}");
        assert_eq!(
            tx.applied_sequence(),
            0,
            "aborted writes must not get applied markers"
        );
        assert!(tx.log().len() <= 1);
        assert!(
            !archive.exists(),
            "aborted changes are not committed archive data"
        );
        // Later retention may span both an aborted entry and a committed one.
        let following = tx.create(400, 500).unwrap();
        assert!(tx.exists(following));
        let expected: Vec<_> = tx.inner().all().into_iter().copied().collect();
        drop(tx);
        let reopened = GenericTransactionsDecorator::new(
            LinkStorage::new(&data, false).unwrap(),
            FileTransitionLog::open(&log).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )
        .unwrap();
        assert_eq!(
            reopened
                .inner()
                .all()
                .into_iter()
                .copied()
                .collect::<Vec<_>>(),
            expected
        );
        if chunked {
            let files: Vec<_> = std::fs::read_dir(archive).unwrap().collect();
            assert_eq!(files.len(), 1);
            let contents = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
            let archived: Vec<_> = contents
                .lines()
                .map(|line| Transition::parse(line).unwrap())
                .collect();
            assert_eq!(archived.len(), 1);
            assert!(archived
                .iter()
                .all(|change| change.transaction_id != aborted));
            assert_eq!(archived[0].after.index, following);
        }
    }
}

#[test]
fn sized_retention_preserves_rollback_and_following_commits() {
    verify(false);
}

#[test]
fn chunked_retention_preserves_rollback_and_archives_only_committed_data() {
    verify(true);
}
