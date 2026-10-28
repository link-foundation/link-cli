//! Real SQL-free author takeover, retained retries and finite process exits.
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
struct CutStore {
    physical: FileLsmJournalStore,
    after: bool,
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
        assert_eq!(keys, &["fence"]);
        if !self.after {
            std::process::exit(73);
        }
        let committed = self.physical.compare_exchange(keys, expected, updates)?;
        assert!(committed);
        std::process::exit(73);
    }
}
fn owner<S: AtomicJournalStore>(store: S) -> Result<AuthorFence<S>, LinkError> {
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
    } else if phase == 2 {
        vec![
            Some(vec!["takeover-a".into()]),
            Some(vec!["takeover-b".into()]),
        ]
    } else {
        vec![
            Some(vec!["following-a".into()]),
            Some(vec!["following-b".into()]),
        ]
    }
}
fn check(
    mut fence: AuthorFence<FileLsmJournalStore>,
    phase: u64,
    data_phase: u64,
) -> Result<FileLsmJournalStore, LinkError> {
    let state = fence.snapshot()?;
    assert_eq!(state.epoch, phase);
    assert_eq!(state.claims, (1..=phase).map(claim).collect::<Vec<_>>());
    assert_eq!(fence.claim(phase - 1, &claim(phase).token)?, claim(phase));
    let mut physical = fence.into_store();
    let before = physical.read(&["fence".into(), "a".into(), "b".into()])?;
    let mut fence = owner(physical)?;
    for old_epoch in 1..phase {
        assert!(fence
            .claim(old_epoch - 1, &claim(old_epoch).token)
            .unwrap_err()
            .to_string()
            .contains("fenced"));
        let mut stale = fence.bind(claim(old_epoch))?;
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
    for (value, row) in values.iter().zip(rows(data_phase)) {
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
        "write"
            | "read"
            | "mutate"
            | "read-advanced"
            | "blocked"
            | "cut-before"
            | "cut-after"
            | "retry-before"
            | "retry-after"
            | "read-claimed"
            | "following"
            | "read-following"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
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
        println!("PASS Rust stale author ownership marker rejected");
        return Ok(());
    }
    let physical = FileLsmJournalStore::open(directory, options)?;
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
    let (phase, data_phase) = match action {
        "read-advanced" => (2, 2),
        "retry-after" | "read-claimed" | "following" => (2, 1),
        "read-following" => (3, 3),
        _ => (1, 1),
    };
    let mut physical = check(fence, phase, data_phase)?;
    if matches!(action, "cut-before" | "cut-after") {
        let mut fence = owner(CutStore {
            physical,
            after: action == "cut-after",
        })?;
        fence.claim(1, &claim(2).token)?;
        unreachable!("Selected takeover CAS did not exit");
    }
    if matches!(action, "retry-before" | "retry-after") {
        let mut fence = owner(physical)?;
        assert_eq!(fence.claim(1, &claim(2).token)?, claim(2));
        physical = check(fence, 2, 1)?;
    }
    if matches!(action, "mutate" | "following") {
        let next = if action == "following" { 3 } else { 2 };
        let mut fence = owner(physical)?;
        assert_eq!(fence.claim(next - 1, &claim(next).token)?, claim(next));
        let generation = fence.snapshot()?.generation;
        let mut current = fence.bind(claim(next))?;
        let values = current.read(&keys())?;
        assert!(current.compare_exchange(&keys(), &values, &rows(next))?);
        let mut fence = owner(current.into_store())?;
        assert_eq!(fence.snapshot()?.generation, generation);
        physical = check(fence, next, next)?;
    }
    if matches!(action, "mutate" | "retry-before" | "following") {
        physical.flush()?;
        physical.compact()?;
    }
    physical.close()?;
    println!("PASS Rust author fencing native LSM {action} {mirrors}");
    Ok(())
}
