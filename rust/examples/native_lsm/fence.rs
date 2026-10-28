//! Real SQL-free author takeover; every read/retry preserves physical bytes.
use link_cli::engine::{
    author_fence::{AuthorClaim, AuthorFence, AuthorFenceOptions},
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::LinkError;
const A: &str = "00000000000000000000000000000001";
fn claim(epoch: u64) -> AuthorClaim {
    AuthorClaim {
        author: A.into(),
        token: format!("{:032x}", epoch * 10),
        epoch,
    }
}
fn owner(store: FileLsmJournalStore) -> Result<AuthorFence<FileLsmJournalStore>, LinkError> {
    AuthorFence::new(
        store,
        "fence",
        A,
        AuthorFenceOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )
}
fn keys() -> Vec<String> {
    ["a", "b"].map(String::from).to_vec()
}
fn rows(phase: u64) -> Vec<Option<Vec<String>>> {
    if phase == 1 {
        vec![
            Some(vec!["seed-a\0雪".into()]),
            Some(vec!["seed-b\u{feff}😀".into()]),
        ]
    } else {
        vec![
            Some(vec!["takeover-a".into()]),
            Some(vec!["takeover-b".into()]),
        ]
    }
}
fn check(
    mut fence: AuthorFence<FileLsmJournalStore>,
    phase: u64,
) -> Result<FileLsmJournalStore, LinkError> {
    let state = fence.snapshot()?;
    assert_eq!(state.epoch, phase);
    assert_eq!(state.claims, (1..=phase).map(claim).collect::<Vec<_>>());
    assert_eq!(fence.claim(phase - 1, &claim(phase).token)?, claim(phase));
    let mut physical = fence.into_store();
    let before = physical.read(&["fence".into(), "a".into(), "b".into()])?;
    let mut fence = owner(physical)?;
    if phase == 2 {
        assert!(fence
            .claim(0, &claim(1).token)
            .unwrap_err()
            .to_string()
            .contains("fenced"));
        let mut stale = fence.bind(claim(1))?;
        assert!(stale
            .read(&keys())
            .unwrap_err()
            .to_string()
            .contains("fenced"));
        assert!(stale
            .compare_exchange(&keys(), &before[1..], &rows(1))
            .unwrap_err()
            .to_string()
            .contains("fenced"));
        fence = owner(stale.into_store())?;
    }
    let mut guarded = fence.bind(claim(phase))?;
    let values = guarded.read(&keys())?;
    for (value, row) in values.iter().zip(rows(phase)) {
        assert_eq!(Some(&value.entries), row.as_ref());
    }
    assert!(guarded.compare_exchange(&keys(), &values, &[None, None])?);
    let mut physical = guarded.into_store();
    assert_eq!(
        physical.read(&["fence".into(), "a".into(), "b".into()])?,
        before
    );
    Ok(physical)
}
pub fn run(action: &str, directory: &str, mirrors: &str) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "write" | "read" | "mutate" | "read-advanced"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
    let mirrored = mirrors == "mirrored";
    let physical = FileLsmJournalStore::open(
        directory,
        FileLsmOptions {
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
        },
    )?;
    let mut fence = owner(physical)?;
    if action == "write" {
        assert_eq!(fence.claim(0, &claim(1).token)?, claim(1));
        let mut guarded = fence.bind(claim(1))?;
        assert!(guarded.compare_exchange(
            &keys(),
            &[JournalVersion::default(), JournalVersion::default()],
            &rows(1)
        )?);
        let mut physical = guarded.into_store();
        physical.flush()?;
        physical.compact()?;
        fence = owner(physical)?;
    }
    let mut physical = check(fence, if action == "read-advanced" { 2 } else { 1 })?;
    if action == "mutate" {
        let mut fence = owner(physical)?;
        assert_eq!(fence.claim(1, &claim(2).token)?, claim(2));
        let generation = fence.snapshot()?.generation;
        let mut current = fence.bind(claim(2))?;
        let values = current.read(&keys())?;
        assert!(current.compare_exchange(&keys(), &values, &rows(2))?);
        let mut fence = owner(current.into_store())?;
        assert_eq!(fence.snapshot()?.generation, generation);
        physical = check(fence, 2)?;
        physical.flush()?;
        physical.compact()?;
    }
    physical.close()?;
    println!("PASS Rust author fencing native LSM {action} {mirrors}");
    Ok(())
}
