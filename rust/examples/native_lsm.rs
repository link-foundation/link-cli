//! Native directory facade: complete-store interoperability and real consumers.
#[cfg(feature = "causal-batches")]
#[path = "native_lsm/batch.rs"]
mod batch;
#[cfg(feature = "causal")]
#[path = "native_lsm/causal.rs"]
mod causal;
#[path = "native_lsm/chunks.rs"]
mod chunks;
#[path = "native_lsm/consumer.rs"]
mod consumer;
#[cfg(feature = "causal-merkle")]
#[path = "native_lsm/merkle.rs"]
mod merkle;
use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
    maintenance::{JournalMaintenanceStore, JournalScanOptions},
};
use link_cli::LinkError;
fn keys() -> Vec<String> {
    ["a", "b", "c", "d"].map(String::from).to_vec()
}
fn check(store: &mut FileLsmJournalStore, advanced: bool) -> Result<(), LinkError> {
    let keys = keys();
    let values = store.read(&keys)?;
    let generations = if advanced { [5, 1, 6, 3] } else { [2, 1, 4, 3] };
    let entries = [
        vec![if advanced {
            "handoff\0雪😀"
        } else {
            "selected\u{feff}雪😀"
        }
        .to_owned()],
        vec![],
        if advanced {
            vec!["revived".into()]
        } else {
            vec![]
        },
        vec!["buffer".into()],
    ];
    for index in 0..4 {
        assert_eq!(values[index].generation, generations[index]);
        assert_eq!(values[index].entries, entries[index]);
    }
    assert_eq!(
        store
            .scan_namespace(JournalScanOptions::default())?
            .into_iter()
            .map(|row| row.key)
            .collect::<Vec<_>>(),
        if advanced {
            keys
        } else {
            vec!["a".into(), "b".into(), "d".into()]
        }
    );
    assert!(!store.compare_exchange(
        &["a".into()],
        &[JournalVersion::default()],
        &[Some(vec!["lost".into()])]
    )?);
    Ok(())
}
fn files(action: &str, directory: &str, mirrors: &str) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "write" | "read" | "mutate" | "read-advanced"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
    let mirrored = mirrors == "mirrored";
    let mut store = FileLsmJournalStore::open(
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
    let keys = keys();
    if action == "write" {
        let before = store.read(&keys[..3])?;
        assert!(store.compare_exchange(
            &keys[..3],
            &before,
            &[
                Some(vec!["old".into()]),
                Some(vec![]),
                Some(vec!["removed\0雪😀".into()])
            ]
        )?);
        store.flush()?;
        let before = store.read(&keys[..2])?;
        assert!(store.compare_exchange(
            &keys[..2],
            &before,
            &[Some(vec!["selected\u{feff}雪😀".into()]), None]
        )?);
        store.flush()?;
        let before = store.read(&keys[3..])?;
        assert!(store.compare_exchange(&keys[3..], &before, &[Some(vec!["buffer".into()])])?);
        assert!(store.compare_delete("c", 1)?);
    }
    check(&mut store, action == "read-advanced")?;
    if action == "mutate" {
        let selected = vec!["a".into(), "d".into()];
        let before = store.read(&selected)?;
        assert!(store.compare_exchange(
            &selected,
            &before,
            &[Some(vec!["handoff\0雪😀".into()]), None]
        )?);
        store.flush()?;
        store.compact()?;
        let before = store.read(&["c".into()])?;
        assert!(store.compare_exchange(&["c".into()], &before, &[Some(vec!["revived".into()])])?);
        check(&mut store, true)?;
    }
    store.close()?;
    println!("PASS Rust native LSM {action} {mirrors}");
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        #[cfg(feature = "causal-batches")]
        ["batch", action, directory, mirrors] => batch::run(action, directory, mirrors)?,
        #[cfg(feature = "causal-merkle")]
        ["merkle", action, directory, mirrors, messages] => merkle::run(action, directory, mirrors, messages)?,
        #[cfg(feature = "causal")]
        ["causal", action, directory, mirrors] => causal::run(action, directory, mirrors)?,
        ["files", action, directory, mirrors] => files(action, directory, mirrors)?,
        ["chunks", action, directory, mirrors] => chunks::run(action, directory, mirrors, false)?,
        ["reclaiming-chunks", action, directory, mirrors] => chunks::run(action, directory, mirrors, true)?,
        ["cluster", mode] => consumer::run(mode)?,
        _ => anyhow::bail!("Use files|chunks|reclaiming-chunks write|read|mutate|read-advanced <directory> plain|mirrored or cluster direct|partitioned|chunks|reclaiming|reclaiming-chunks"),
    }
    Ok(())
}
