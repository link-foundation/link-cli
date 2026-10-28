//! The selected local owner composed with a SQL-free physical native directory.
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_replica::{CausalReplica, CausalReplicaOptions},
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::LinkError;
const A: &str = "00000000000000000000000000000001";
const B: &str = "00000000000000000000000000000002";
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
fn expected() -> Result<Vec<CausalVersion>, LinkError> {
    let initial = CausalVersion::advance(1, A, 0, &[], row("initial\u{feff}雪😀\0"))?;
    let deletion = CausalVersion::advance(1, B, 0, std::slice::from_ref(&initial), None)?;
    let selected = CausalVersion::advance(
        1,
        A,
        1,
        std::slice::from_ref(&deletion),
        row("selected\u{feff}雪😀\0"),
    )?;
    let other = CausalVersion::advance(2, A, 2, &[], None)?;
    let advanced = CausalVersion::advance(1, A, 3, std::slice::from_ref(&selected), None)?;
    let following = CausalVersion::advance(3, A, 4, &[], row("following"))?;
    Ok(vec![
        initial, deletion, selected, other, advanced, following,
    ])
}
fn check(
    owner: &mut CausalReplica<CutStore>,
    phase: usize,
    values: &[CausalVersion],
) -> Result<(), LinkError> {
    let initial = &values[0];
    let deletion = &values[1];
    let selected = &values[2];
    let other = &values[3];
    let advanced = &values[4];
    let following = &values[5];
    let mut rows = vec![
        if phase == 0 {
            selected.clone()
        } else {
            advanced.clone()
        },
        other.clone(),
    ];
    if phase == 2 {
        rows.push(following.clone());
    }
    let mut snapshot = owner.snapshot()?;
    assert_eq!(snapshot.last_counter, 3 + phase as u64);
    assert_eq!(snapshot.versions, rows);
    assert_eq!(
        owner.write(&token(1), 1, &[], initial.payload.clone())?,
        *initial
    );
    assert!(!owner.receive(&[initial.clone(), deletion.clone()])?);
    snapshot.versions[0].clock[0].counter = 99;
    assert_eq!(
        owner.read(1)?,
        vec![if phase == 0 {
            selected.clone()
        } else {
            advanced.clone()
        }]
    );
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
    let mut owner = CausalReplica::new(
        store,
        "replica",
        A,
        CausalReplicaOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?;
    let values = expected()?;
    if action == "write" {
        owner.write(&token(1), 1, &[], values[0].payload.clone())?;
        owner.receive(&values[1..2])?;
        owner.write(&token(2), 1, &values[1..2], values[2].payload.clone())?;
        owner.write(&token(3), 2, &[], None)?;
        // Return the physical owner for explicitly selected flush/compaction.
        let mut store = owner.into_store();
        store.physical.flush()?;
        store.physical.compact()?;
        owner = CausalReplica::new(
            store,
            "replica",
            A,
            CausalReplicaOptions {
                require_durable_flush: true,
                ..Default::default()
            },
        )?;
    }
    check(
        &mut owner,
        match action {
            "read-advanced" | "retry-after" | "following" => 1,
            "read-following" => 2,
            _ => 0,
        },
        &values,
    )?;
    if matches!(
        action,
        "mutate" | "cut-before" | "cut-after" | "retry-before" | "retry-after"
    ) {
        assert_eq!(owner.write(&token(4), 1, &values[2..3], None)?, values[4]);
        check(&mut owner, 1, &values)?;
    }
    if action == "following" {
        owner.write(&token(5), 3, &[], values[5].payload.clone())?;
        assert_eq!(owner.write(&token(4), 1, &values[2..3], None)?, values[4]);
        check(&mut owner, 2, &values)?;
    }
    let mut store = owner.into_store();
    if matches!(action, "mutate" | "retry-before" | "following") {
        store.physical.flush()?;
        store.physical.compact()?;
    }
    store.physical.close()?;
    println!("PASS Rust causal native LSM {action} {mirrors}");
    Ok(())
}
