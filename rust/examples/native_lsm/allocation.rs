//! Permanent ranges: handoff, retained retries, finite process exits.
use link_cli::engine::{
    address_allocator::{AddressAllocator, AddressAllocatorOptions, AddressReservation},
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
};
use link_cli::LinkError;
use std::{cell::RefCell, rc::Rc};
const A: &str = "00000000000000000000000000000001";
const B: &str = "00000000000000000000000000000002";
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
fn grant(author: &str, token: u64, count: u64, first: u64, last: u64) -> AddressReservation {
    AddressReservation {
        author: author.into(),
        token: format!("{token:032x}"),
        count,
        first,
        last,
    }
}
fn reserve(
    owner: &mut AddressAllocator<CutStore>,
    grant: &AddressReservation,
) -> Result<(), LinkError> {
    assert_eq!(
        owner.reserve(&grant.author, &grant.token, grant.count)?,
        *grant
    );
    Ok(())
}
fn check(
    owner: &mut AddressAllocator<CutStore>,
    store: CutStore,
    grants: &[AddressReservation],
    phase: usize,
) -> Result<(), LinkError> {
    assert_eq!(owner.snapshot()?.reservations, grants[..=phase]);
    assert_eq!(owner.snapshot()?.next, Some([3, 6, 8][phase]));
    for grant in &grants[..=phase] {
        reserve(owner, grant)?;
    }
    assert!(owner.reserve(B, &grants[0].token, 2).is_err());
    assert!(owner.reserve(A, &grants[0].token, 1).is_err());
    assert!(
        AddressAllocator::new(store, "addresses", 2, 20, Default::default())?
            .snapshot()
            .is_err()
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
    let physical = Rc::new(RefCell::new(FileLsmJournalStore::open(directory, options)?));
    let store = CutStore {
        physical: physical.clone(),
        action: action.into(),
    };
    let mut owner = AddressAllocator::new(
        store.clone(),
        "addresses",
        1,
        20,
        AddressAllocatorOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?;
    let grants = [
        grant(A, 100, 2, 1, 2),
        grant(B, 200, 3, 3, 5),
        grant(A, 300, 2, 6, 7),
    ];
    if action == "write" {
        reserve(&mut owner, &grants[0])?;
        physical.borrow_mut().flush()?;
        physical.borrow_mut().compact()?;
    }
    let phase = if matches!(action, "read-advanced" | "retry-after" | "following") {
        1
    } else if action == "read-following" {
        2
    } else {
        0
    };
    check(&mut owner, store.clone(), &grants, phase)?;
    if matches!(
        action,
        "mutate" | "cut-before" | "cut-after" | "retry-before" | "retry-after"
    ) {
        reserve(&mut owner, &grants[1])?;
        check(&mut owner, store.clone(), &grants, 1)?;
    }
    if action == "following" {
        reserve(&mut owner, &grants[2])?;
        check(&mut owner, store, &grants, 2)?;
    }
    if matches!(action, "mutate" | "retry-before" | "following") {
        physical.borrow_mut().flush()?;
        physical.borrow_mut().compact()?;
    }
    physical.borrow_mut().close()?;
    println!("PASS Rust address allocation native LSM {action} {mirrors}");
    Ok(())
}
