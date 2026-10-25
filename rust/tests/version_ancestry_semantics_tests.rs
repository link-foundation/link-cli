//! Public histories retain nested fork cutoffs, own heads and unrelated branches.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, TransactionsDecorator, VersionControlDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;
#[cfg(target_arch = "wasm32")]
#[path = "common/flat_version_metadata.rs"]
mod flat_version_metadata;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn finite_deep_ancestry_and_following_writes_in_actual_wasm() -> Result<()> {
    let mut vc = flat_version_metadata::FlatMetadata::deep_version(2048)?;
    assert_eq!(vc.list_branches().len(), 2049);
    vc.checkout(0)?;
    vc.switch_branch("main")?;
    vc.switch_branch("branch-2048")?;
    assert!(vc.branch("following", Some(1)).is_err());
    vc.branch("following", Some(0))?;
    let id = vc.create_and_update(11, 12)?;
    let sequence = vc.current_sequence();
    assert!(sequence > 0);
    vc.checkout(0)?;
    vc.checkout(sequence)?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 11);
    Ok(())
}

fn exercise(vc: &mut VersionControlDecorator, id: u32) -> Result<()> {
    vc.update(id, 21, 22)?; // main: 1, 2; its later 8 must not enter descendants.
    vc.update(id, 31, 32)?;
    vc.branch("middle", None)?;
    vc.switch_branch("middle")?;
    vc.update(id, 41, 42)?; // middle: 3, 4, 7; leaf forks before 4.
    vc.update(id, 51, 52)?;
    vc.branch("leaf", Some(3))?;
    vc.switch_branch("leaf")?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 41);
    vc.update(id, 61, 62)?; // leaf: 5, 6.
    vc.update(id, 71, 72)?;
    vc.switch_branch("middle")?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 51);
    vc.update(id, 81, 82)?;
    vc.switch_branch("main")?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 31);
    vc.update(id, 91, 92)?;
    vc.branch("unrelated", None)?;
    vc.switch_branch("unrelated")?;
    vc.update(id, 101, 102)?;
    vc.switch_branch("leaf")?;
    for (sequence, source) in [
        (6, 71),
        (5, 61),
        (3, 41),
        (2, 31),
        (1, 21),
        (0, 11),
        (6, 71),
    ] {
        vc.checkout(sequence)?;
        assert_eq!(vc.current_sequence(), sequence);
        assert_eq!(vc.transactions().get(id).unwrap().source, source);
    }
    let heads = vc.list_branches();
    for missing in [4, 7, 8, 9] {
        assert!(vc.checkout(missing).is_err(), "unreachable {missing}");
        assert_eq!(vc.current_sequence(), 6);
        assert_eq!(vc.transactions().get(id).unwrap().source, 71);
        assert_eq!(vc.list_branches(), heads);
    }
    vc.branch("cut", Some(5))?;
    vc.switch_branch("cut")?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 61);
    assert!(vc.checkout(6).is_err());
    vc.branch("empty", Some(0))?;
    vc.switch_branch("empty")?;
    assert_eq!(vc.transactions().get(id).unwrap().source, 11);
    vc.update(id, 111, 112)?;
    assert_eq!(vc.current_sequence(), 10);
    vc.save()?;
    Ok(())
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn nested_ancestry_preserves_cutoffs_order_and_following_writes() -> Result<()> {
    let (data_files, mut data) = test_store::named()?;
    let (log_files, log) = test_store::named()?;
    let (branch_files, branches) = test_store::named()?;
    let id = data.try_create(11, 12)?;
    let tx = TransactionsDecorator::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?;
    let mut vc = VersionControlDecorator::new(tx, branches, false)?;
    exercise(&mut vc, id)?;
    drop(vc);
    #[cfg(not(target_arch = "wasm32"))]
    for _ in 0..2 {
        let open = |files: &test_store::NamedFiles| {
            link_cli::NamedTypesDecorator::with_names_database_path(
                files.0.path(),
                files.1.path(),
                false,
            )
        };
        let tx = TransactionsDecorator::new(
            open(&data_files)?,
            open(&log_files)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        let mut reopened = VersionControlDecorator::new(tx, open(&branch_files)?, false)?;
        assert_eq!(reopened.current_branch(), "empty");
        assert_eq!(reopened.current_sequence(), 10);
        assert_eq!(reopened.transactions().get(id).unwrap().source, 111);
        let expected = [
            ("main", 8),
            ("middle", 7),
            ("leaf", 6),
            ("unrelated", 9),
            ("cut", 5),
            ("empty", 10),
        ];
        for (name, head) in expected {
            assert_eq!(
                reopened
                    .list_branches()
                    .iter()
                    .find(|b| b.name == name)
                    .unwrap()
                    .head,
                head
            );
        }
        reopened.switch_branch("leaf")?;
        reopened.checkout(3)?;
        assert_eq!(reopened.transactions().get(id).unwrap().source, 41);
        reopened.switch_branch("empty")?;
        reopened.save()?;
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (data_files, log_files, branch_files);
    Ok(())
}
