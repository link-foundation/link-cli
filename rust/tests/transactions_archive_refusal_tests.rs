//! An archive that cannot take a chunk keeps the commit open until it can.
#![cfg(unix)]
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;

use anyhow::Result;
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
    NamedTypesDecorator,
};

#[test]
fn a_commit_the_archive_refuses_is_finished_by_trying_again() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let archive = root.join("archive");
    fs::create_dir(&archive)?;
    fs::set_permissions(&archive, Permissions::from_mode(0o500))?;
    if fs::File::create(archive.join("probe")).is_ok() {
        // The superuser ignores permissions, so nothing can refuse it here.
        return Ok(());
    }

    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        NamedTypesDecorator::new(root.join("data.links"), false)?,
        FileTransitionLog::open(root.join("journal"))?,
        LogRetentionPolicy::Chunked {
            chunk_size: 1,
            archive_directory: archive.clone(),
        },
        CommitMode::Sync,
        false,
    )?;
    tx.begin_transaction()?;
    let first = tx.create(1, 1)?;
    let refused = tx.commit().unwrap_err().to_string();
    assert!(
        refused.contains("failed to create archive file"),
        "{refused}"
    );
    assert_eq!(fs::read_dir(&archive)?.count(), 0);

    // The attempted commit is finished once the archive takes the chunk.
    fs::set_permissions(&archive, Permissions::from_mode(0o700))?;
    tx.commit()?;
    assert_eq!(fs::read_dir(&archive)?.count(), 1);
    assert_eq!(tx.get(first).unwrap().source, 1);
    Ok(())
}
