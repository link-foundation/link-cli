//! Finite before/after allocation identity, independent of wall clocks.
use link_cli::{
    engine::{Engine, EngineOptions, MemoryLog, StateStructure},
    LinksStorage,
};
fn main() {
    const ROWS: u64 = 256;
    const COMMITS: u64 = 16;
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        let db = Engine::<u64, _>::open(
            MemoryLog::default(),
            EngineOptions {
                state_structure: structure,
                ..Default::default()
            },
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        for id in 1..=ROWS {
            seed.ensure_link_created(id).unwrap();
            seed.set_name(id, Some(format!("untouched immutable row {id}")))
                .unwrap();
        }
        seed.commit().unwrap();
        let original = db.snapshot().unwrap();
        let mut previous = original.clone();
        let mut shared = 0;
        for revision in 1..=COMMITS {
            let mut tx = db.begin().unwrap();
            tx.update_link(1, revision, 0).unwrap();
            tx.commit().unwrap();
            let current = db.snapshot().unwrap();
            for id in 2..=ROWS {
                assert_eq!(current.get(id), previous.get(id));
                shared += usize::from(std::ptr::eq(
                    current.get(id).unwrap(),
                    previous.get(id).unwrap(),
                ));
            }
            assert_eq!(original.get(1).unwrap().link.source, 0);
            previous = current;
        }
        println!("structure={structure:?} rows={ROWS} commits={COMMITS} unchanged_row_identity_shared={shared}/{}", (ROWS - 1) * COMMITS);
        assert_eq!(
            shared,
            if structure == StateStructure::Shared {
                ((ROWS - 1) * COMMITS) as usize
            } else {
                0
            }
        );
    }
}
