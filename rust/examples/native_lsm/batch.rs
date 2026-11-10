//! Two-key local batches and exact receipt retry in SQL-free directories.
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_batch::{CausalBatch, CausalBatchEdit},
    causal_replica::CausalReplicaOptions,
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::LinkError;
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
struct CutStore {
    physical: FileLsmJournalStore,
    action: String,
}
impl AtomicJournalStore for CutStore {
    fn supports_durable_flush(&self) -> bool {
        self.physical.supports_durable_flush()
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.physical.read(keys)
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
        let result = self.physical.compare_exchange(keys, expected, updates)?;
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
        physical: FileLsmJournalStore::open(directory, options)?,
        action: action.into(),
    };
    let mut owner = batches(store)?;
    let first = [
        CausalVersion::advance(1, A, 0, &[], row("initial\u{feff}雪😀\0"))?,
        CausalVersion::advance(2, A, 1, &[], None)?,
    ];
    let second = [
        CausalVersion::advance(1, A, 2, &first[..1], None)?,
        CausalVersion::advance(2, A, 3, &first[1..], row("selected\u{feff}雪😀\0"))?,
    ];
    let following = CausalVersion::advance(3, A, 4, &[], row("following"))?;
    if action == "write" {
        assert_eq!(owner.write(&token(100), 0, &initial_edits(&first))?, first);
        let mut store = owner.into_store();
        store.physical.flush()?;
        store.physical.compact()?;
        owner = batches(store)?;
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
        assert_eq!(
            owner.write(&token(200), 1, &next_edits(&first, &second))?,
            second
        );
        check(&mut owner, 1, &first, &second, &following)?;
    }
    if action == "following" {
        assert_eq!(
            owner
                .write(
                    &token(300),
                    2,
                    &[CausalBatchEdit {
                        address: 3,
                        observed: vec![],
                        payload: following.payload.clone()
                    }]
                )?
                .as_slice(),
            std::slice::from_ref(&following)
        );
        assert!(owner.write(&token(400), 3, &[])?.is_empty());
        check(&mut owner, 2, &first, &second, &following)?;
    }
    let mut store = owner.into_store();
    if matches!(action, "mutate" | "retry-before" | "following") {
        store.physical.flush()?;
        store.physical.compact()?;
    }
    store.physical.close()?;
    println!("PASS Rust causal batch native LSM {action} {mirrors}");
    Ok(())
}
