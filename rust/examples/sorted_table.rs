//! Immutable physical table interoperability. Uses native pages without SQL.
use link_cli::engine::paged::{FilePageDevice, SortedJournalEntry, SortedJournalTable};
fn rows() -> Vec<SortedJournalEntry> {
    (0..80)
        .map(|i| SortedJournalEntry {
            key: format!("k{i:03}"),
            generation: if i == 1 { u64::MAX } else { i + 1 },
            entries: match i {
                1 => None,
                2 => Some(vec![]),
                _ => Some(vec![format!("\u{feff}雪\0😀\n{}", "x".repeat(500))]),
            },
        })
        .collect()
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3 && matches!(args[2].as_str(), "create" | "read"),
        "Provide <dedicated-file> create|read"
    );
    let device = FilePageDevice::open(&args[1])?;
    let mut table = if args[2] == "create" {
        SortedJournalTable::create(device, &rows(), Default::default())?
    } else {
        SortedJournalTable::open(device, Default::default())?
    };
    assert_eq!(table.entries()?, rows());
    for row in rows() {
        assert_eq!(table.get(&row.key)?, Some(row));
    }
    assert!(table.get("missing")?.is_none());
    println!("Rust native sorted table {}: {} exact rows, tombstone/empty/Unicode/u64 and indexed points; digest {}", args[2],table.len(),table.digest());
    table.into_device().close()?;
    Ok(())
}
