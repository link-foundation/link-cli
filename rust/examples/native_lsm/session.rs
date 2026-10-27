//! Ordinary sessions and retained exact requests in SQL-free directories.
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_projection::CausalConflictPolicy,
    causal_replica::CausalReplicaOptions,
    causal_session::CausalSession,
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::{LinkError, LinksStorage};
use std::{cell::RefCell, rc::Rc};
fn physical_bytes(
    directory: &str,
) -> std::io::Result<std::collections::BTreeMap<std::ffi::OsString, Vec<u8>>> {
    std::fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            Ok((entry.file_name(), std::fs::read(entry.path())?))
        })
        .collect()
}
const A: &str = "00000000000000000000000000000001";
fn token(n: u64) -> String {
    format!("{n:032x}")
}
fn row(name: &str) -> Option<CausalPayload> {
    Some(CausalPayload {
        source: 2,
        target: 3,
        name: Some(name.into()),
    })
}
#[derive(Clone)]
struct CutStore {
    physical: Rc<RefCell<FileLsmJournalStore>>,
    action: String,
}
impl AtomicJournalStore for CutStore {
    fn supports_durable_flush(&self) -> bool {
        self.physical.borrow().supports_durable_flush()
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.physical.borrow_mut().read(keys)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        if self.action == "cut-before" {
            std::process::exit(73);
        }
        let result = self
            .physical
            .borrow_mut()
            .compare_exchange(keys, expected, updates)?;
        if self.action == "cut-after" && result {
            std::process::exit(73);
        }
        Ok(result)
    }
}
fn batches(store: CutStore) -> Result<CausalBatch<CutStore>, LinkError> {
    CausalBatch::new(
        store,
        "replica",
        "batches",
        A,
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
}
fn initial_edits(first: &[CausalVersion]) -> Vec<CausalBatchEdit> {
    first
        .iter()
        .map(|v| CausalBatchEdit {
            address: v.address,
            observed: vec![],
            payload: v.payload.clone(),
        })
        .collect()
}
fn next_edits(first: &[CausalVersion], second: &[CausalVersion]) -> Vec<CausalBatchEdit> {
    second
        .iter()
        .zip(first)
        .map(|(v, prior)| CausalBatchEdit {
            address: v.address,
            observed: vec![prior.clone()],
            payload: v.payload.clone(),
        })
        .collect()
}
fn check(
    owner: &mut CausalBatch<CutStore>,
    phase: usize,
    first: &[CausalVersion],
    second: &[CausalVersion],
    following: &CausalVersion,
) -> Result<(), LinkError> {
    let mut expected = if phase == 0 {
        first.to_vec()
    } else {
        second.to_vec()
    };
    if phase == 2 {
        expected.push(following.clone());
    }
    let mut snapshot = owner.snapshot()?;
    assert_eq!(
        snapshot.generation,
        match phase {
            0 => 1,
            1 => 2,
            // The receipt-only empty batch leaves the owner key unchanged.
            _ => 3,
        }
    );
    assert_eq!(
        snapshot.last_counter,
        match phase {
            0 => 2,
            1 => 4,
            _ => 5,
        }
    );
    assert_eq!(snapshot.versions, expected);
    assert_eq!(owner.write(&token(100), 0, &initial_edits(first))?, first);
    if phase != 0 {
        assert_eq!(
            owner.write(&token(200), 1, &next_edits(first, second))?,
            second
        );
    }
    snapshot.versions[0].clock[0].counter = 99;
    assert_eq!(owner.snapshot()?.versions, expected);
    Ok(())
}
pub fn run(action: &str, directory: &str, mirrors: &str) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "write"
            | "read"
            | "mutate"
            | "read-advanced"
            | "blocked"
            | "cut-before"
            | "cut-after"
            | "retry-before"
            | "retry-after"
            | "following"
            | "read-following"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    let mirrored = mirrors == "mirrored";
    let options = FileLsmOptions {
        store: LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 65536,
                buffer_keys: 16,
                buffer_bytes: 32768,
                max_runs: 8,
                ..Default::default()
            },
            ..Default::default()
        },
        max_pages: 128,
        mirrored_root: mirrored,
        mirrored_banks: mirrored,
        mirrored_tables: mirrored,
    };
    if action == "blocked" {
        assert!(FileLsmJournalStore::open(directory, options).is_err());
        println!("PASS Rust stale ownership rejected");
        return Ok(());
    }
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
    let store = CutStore {
        physical: Rc::new(RefCell::new(FileLsmJournalStore::open(directory, options)?)),
        action: action.into(),
    };
    let mut owner = batches(store.clone())?;
    let session = |first, last| {
        CausalSession::new(
            batches(store.clone())?,
            first,
            last,
            CausalConflictPolicy::Reject,
            Default::default(),
        )
    };
    let first = [
        CausalVersion::advance(1, A, 0, &[], row("initial\u{feff}雪😀\0"))?,
        CausalVersion::advance(2, A, 1, &[], row("second"))?,
    ];
    let second = [
        CausalVersion::advance(1, A, 2, &first[..1], None)?,
        CausalVersion::advance(2, A, 3, &first[1..], row("selected\u{feff}雪😀\0"))?,
    ];
    let following = CausalVersion::advance(3, A, 4, &[], row("following"))?;
    if action == "write" {
        let mut tx = session(1, 2)?;
        assert_eq!(tx.create_link(2, 3)?, 1);
        tx.set_name(1, first[0].payload.as_ref().unwrap().name.clone())?;
        assert_eq!(tx.create_link(2, 3)?, 2);
        tx.set_name(2, Some("second".into()))?;
        assert_eq!(tx.commit(&token(100))?, first);
        drop(tx);
        store.physical.borrow_mut().flush()?;
        store.physical.borrow_mut().compact()?;
    }
    check(
        &mut owner,
        match action {
            "read-advanced" | "retry-after" | "following" => 1,
            "read-following" => 2,
            _ => 0,
        },
        &first,
        &second,
        &following,
    )?;
    if matches!(
        action,
        "mutate" | "cut-before" | "cut-after" | "retry-before" | "retry-after"
    ) {
        if action == "retry-after" {
            // The exact pre-exit request is retained independently of the new snapshot.
            assert_eq!(
                owner.write(&token(200), 1, &next_edits(&first, &second))?,
                second
            );
        } else {
            let mut stale = session(8, 9)?;
            stale.set_name(1, Some("stale".into()))?;
            let mut tx = session(3, 3)?;
            tx.delete_link(1)?;
            tx.set_name(2, second[1].payload.as_ref().unwrap().name.clone())?;
            assert_eq!(tx.commit(&token(200))?, second);
            let keys = ["replica".into(), "batches".into()];
            let before = store.physical.borrow_mut().read(&keys)?;
            let bytes = physical_bytes(directory)?;
            assert!(stale.commit(&token(500)).is_err());
            assert_eq!(store.physical.borrow_mut().read(&keys)?, before);
            assert_eq!(physical_bytes(directory)?, bytes);
        }
        check(&mut owner, 1, &first, &second, &following)?;
    }
    if action == "following" {
        let mut tx = session(3, 3)?;
        assert_eq!(tx.create_link(2, 3)?, 3);
        tx.set_name(3, Some("following".into()))?;
        assert_eq!(
            tx.commit(&token(300))?.as_slice(),
            std::slice::from_ref(&following)
        );
        let mut empty = session(4, 4)?;
        assert!(empty.commit(&token(400))?.is_empty());
        check(&mut owner, 2, &first, &second, &following)?;
    }
    if matches!(action, "mutate" | "retry-before" | "following") {
        store.physical.borrow_mut().flush()?;
        store.physical.borrow_mut().compact()?;
    }
    store.physical.borrow_mut().close()?;
    println!("PASS Rust causal session native LSM {action} {mirrors}");
    Ok(())
}
