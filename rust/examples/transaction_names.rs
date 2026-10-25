//! cargo run --manifest-path rust/Cargo.toml --example transaction_names -- <directory>
use anyhow::{ensure, Result};
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator,
};

fn main() -> Result<()> {
    let directory = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("provide a database directory"))?;
    let root = std::path::Path::new(&directory);
    std::fs::create_dir_all(root)?;
    let mut vc = VersionControlDecorator::new(
        TransactionsDecorator::new(
            NamedTypesDecorator::new(root.join("data"), false)?,
            NamedTypesDecorator::new(root.join("log"), false)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?,
        NamedTypesDecorator::new(root.join("versions"), false)?,
        false,
    )?;
    let id = vc.create_and_update(11, 12)?;
    vc.set_name(id, "original 雪")?;
    let before = vc.current_sequence();
    vc.begin_transaction()?;
    vc.set_name(id, "temporary")?;
    vc.rollback()?;
    ensure!(vc.get_name(id)?.as_deref() == Some("original 雪"));
    vc.set_name(id, "published 雪|\0\n")?;
    let after = vc.current_sequence();
    vc.checkout(before)?;
    ensure!(vc.get_name(id)?.as_deref() == Some("original 雪"));
    vc.checkout(after)?;
    ensure!(vc.get_name(id)?.as_deref() == Some("published 雪|\0\n"));
    vc.save()?;
    println!("name rollback and versions {before} -> {after} verified for row {id}");
    Ok(())
}
