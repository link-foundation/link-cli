//! Store exact document labels in the native legacy text store without SQL.
//! cargo run --manifest-path rust/Cargo.toml --example text_names -- <new-file>
use anyhow::{anyhow, ensure, Context, Result};
use link_cli::{lock_file_path, FileLock, LinkStorage, LinksStorage, LockMode};
use std::path::PathBuf;

fn main() -> Result<()> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("provide a new database path")?,
    );
    let _owner = FileLock::try_acquire(lock_file_path(&path), LockMode::Exclusive)?
        .ok_or_else(|| anyhow!("database writer is busy"))?;
    ensure!(!path.exists(), "choose an absent path for this example");
    let mut store = LinkStorage::new(&path, false)?;
    let document = store.create(0, 0);
    let heading = store.create(document, 0);
    let label = "Document \"heading\" 雪\nsecond line";
    store.set_name(document, label);
    store.set_name(heading, "article title 雪");
    store.set_name(0, "dataset\0label"); // A name does not allocate a row.
    LinksStorage::flush(&mut store)?;
    drop(store);
    let reopened = LinkStorage::new(&path, false)?;
    ensure!(reopened.all().len() == 2, "unexpected row count");
    ensure!(reopened.get_name(document).map(String::as_str) == Some(label));
    ensure!(reopened.get_by_name("dataset\0label") == Some(0));
    println!("{}", reopened.format_structure(heading)?);
    Ok(())
}
