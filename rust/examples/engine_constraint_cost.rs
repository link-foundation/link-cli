//! Finite allocation comparison for repeatedly renaming one constrained row.
//! cargo run --manifest-path rust/Cargo.toml --example engine_constraint_cost
//! Counts allocation requests during staging/commit, excluding setup/recovery.
use link_cli::engine::{
    Constraint, ConstraintValidation, Engine, EngineOptions, MemoryLog, RevisionEncoding,
    StateStructure,
};
use link_cli::LinksStorage;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

struct CountingAllocator;
static COUNT: AtomicBool = AtomicBool::new(false);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: Every allocation operation delegates unchanged to System. Counters
// use atomics and never allocate; this example measures one application thread.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.load(Relaxed) {
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNT.load(Relaxed) {
            BYTES.fetch_add(new_size, Relaxed);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn run(size: u32, validation: ConstraintValidation) -> anyhow::Result<usize> {
    let db = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            constraints: vec![
                Constraint::References,
                Constraint::UniquePairs,
                Constraint::UniqueNames,
            ],
            constraint_validation: validation,
            state_structure: StateStructure::Shared,
            revision_encoding: RevisionEncoding::Changes,
            ..Default::default()
        },
    )?;
    let mut seed = db.begin()?;
    for id in 1..=size {
        seed.ensure_link_created(id)?;
        seed.update_link(id, id, id)?;
        seed.set_name(id, Some(format!("row {id}")))?;
    }
    seed.commit()?;
    BYTES.store(0, Relaxed);
    COUNT.store(true, Relaxed);
    for edit in 0..8 {
        let mut tx = db.begin()?;
        tx.set_name(1, Some(format!("edited {edit}")))?;
        tx.commit()?;
    }
    COUNT.store(false, Relaxed);
    let bytes = BYTES.load(Relaxed);
    assert_eq!(
        db.snapshot()?.get(1).unwrap().name.as_deref(),
        Some("edited 7")
    );
    println!("rows={size} edits=8 validation={validation:?} allocation-request-bytes={bytes}");
    Ok(bytes)
}

fn main() -> anyhow::Result<()> {
    for size in [128, 1024] {
        let scan = run(size, ConstraintValidation::Scan)?;
        let indexed = run(size, ConstraintValidation::Indexed)?;
        assert!(
            indexed < scan,
            "indexed validation must avoid full scan allocations"
        );
    }
    Ok(())
}
