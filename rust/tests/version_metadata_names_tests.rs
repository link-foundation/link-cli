//! Accepted branch/tag names must survive native sidecar persistence exactly.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypes, NamedTypesDecorator, TransactionsDecorator,
    TransitionLogStore, VersionControlDecorator,
};
use std::path::Path;

fn open(directory: &Path) -> Result<VersionControlDecorator> {
    VersionControlDecorator::new(
        TransactionsDecorator::new(
            NamedTypesDecorator::new(directory.join("data.links"), false)?,
            NamedTypesDecorator::new(directory.join("log.links"), false)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?,
        NamedTypesDecorator::new(directory.join("branches.links"), false)?,
        false,
    )
}

fn verify_names(branch: &str, tag: &str, legacy_markers: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut vc = open(directory.path())?;
    let baseline = vc.create_and_update(11, 12)?;
    let baseline_head = vc.current_sequence();
    vc.branch(branch, None)?;
    vc.switch_branch(branch)?;
    let created = vc.create_and_update(21, 22)?;
    let head = vc.current_sequence();
    vc.tag(tag, None)?;
    vc.branch("child", Some(head))?;
    vc.tag("plain", None)?;
    let branches = vc.list_branches();
    vc.save()?;
    drop(vc);
    if legacy_markers {
        let mut metadata =
            NamedTypesDecorator::new(directory.path().join("branches.links"), false)?;
        let hex = |text: &str| {
            text.encode_utf16()
                .map(|unit| format!("{unit:04X}"))
                .collect::<String>()
        };
        for info in branches {
            let marker = format!(
                "__vc:branch:2|{}|{}|{}|{}",
                hex(&info.name),
                info.parent
                    .as_deref()
                    .map(hex)
                    .unwrap_or_else(|| "-".into()),
                info.fork_seq,
                info.head
            );
            let link = metadata
                .get_by_name(&marker)?
                .expect("version-two branch marker");
            metadata.set_name(
                link,
                &format!(
                    "__vc:branch:{}:parent={}:fork={}:head={}",
                    info.name,
                    info.parent.as_deref().unwrap_or(""),
                    info.fork_seq,
                    info.head
                ),
            )?;
        }
        metadata.save()?;
    }
    let mut recovered = open(directory.path())?;
    assert_eq!(recovered.current_branch(), branch);
    assert_eq!(recovered.current_sequence(), head);
    assert!(recovered
        .list_branches()
        .iter()
        .any(|info| info.name == branch
            && info.parent.as_deref() == Some("main")
            && info.head == head));
    assert!(recovered
        .list_branches()
        .iter()
        .any(|info| info.name == "child"
            && info.parent.as_deref() == Some(branch)
            && info.fork_seq == head));
    assert_eq!(recovered.try_get_tag(tag), Some(head));
    assert_eq!(recovered.try_get_tag("plain"), Some(head));
    recovered.switch_branch("main")?;
    assert_eq!(recovered.current_sequence(), baseline_head);
    assert!(recovered.exists(baseline));
    assert!(!recovered.exists(created));
    recovered.switch_branch("child")?;
    assert_eq!(recovered.current_sequence(), head);
    assert!(recovered.exists(created));
    recovered.checkout(baseline_head)?;
    assert!(!recovered.exists(created));
    recovered.checkout(head)?;
    assert!(recovered.exists(created));
    Ok(())
}

#[test]
fn delimiter_branch_name_survives_reopening() -> Result<()> {
    verify_names("topic:parent=decoy:fork=7:head=9/😀", "plain", false)
}

#[test]
fn quoted_parent_name_survives_reopening() -> Result<()> {
    verify_names("{\"name\":\"quoted\"}:parent=:", "\"quoted\"=value", false)
}

#[test]
fn delimiter_tag_name_survives_reopening() -> Result<()> {
    verify_names("safe-branch", "release=version=😀", false)
}

#[test]
fn nul_and_newline_names_survive_reopening() -> Result<()> {
    verify_names("zero\0line\n:parent=😀", "zero\0=\n", false)
}

#[test]
fn legacy_markers_preserve_branch_ancestry_and_checkout() -> Result<()> {
    verify_names("legacy", "old=tag", true)
}

#[test]
fn legacy_names_starting_with_version_two_prefix_survive_reopening() -> Result<()> {
    verify_names("2|0041|-|0|0", "prefix=tag", true)
}

#[test]
fn shared_legacy_tag_vectors_recover_entire_names() -> Result<()> {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/version-metadata-vectors.json"
    ))?;
    let directory = tempfile::tempdir()?;
    let mut metadata = NamedTypesDecorator::new(directory.path().join("branches.links"), false)?;
    for value in vectors["tags"].as_array().unwrap() {
        metadata.append_log_entry(value["marker"].as_str().unwrap())?;
    }
    for marker in vectors["invalidTags"].as_array().unwrap() {
        metadata.append_log_entry(marker.as_str().unwrap())?;
    }
    metadata.save()?;
    drop(metadata);
    let recovered = open(directory.path())?;
    assert_eq!(
        recovered.list_tags().len(),
        vectors["tags"].as_array().unwrap().len()
    );
    for value in vectors["tags"].as_array().unwrap() {
        assert_eq!(
            recovered.try_get_tag(value["name"].as_str().unwrap()),
            value["sequence"].as_i64()
        );
    }
    Ok(())
}

/// Runs `edit` over the version metadata of a repository with one commit.
fn reopened_after(edit: impl FnOnce(&mut NamedTypesDecorator) -> Result<()>) -> Result<i64> {
    let directory = tempfile::tempdir()?;
    let mut vc = open(directory.path())?;
    vc.create_and_update(11, 12)?;
    let head = vc.current_sequence();
    vc.save()?;
    drop(vc);
    {
        let mut metadata =
            NamedTypesDecorator::new(directory.path().join("branches.links"), false)?;
        edit(&mut metadata)?;
        metadata.save()?;
    }
    let reopened = open(directory.path())?;
    assert_eq!(reopened.current_branch(), "main");
    assert!(reopened.list_branches().iter().any(|b| b.name == "main"));
    assert_eq!(reopened.current_sequence(), head);
    Ok(head)
}

fn remove_marker(metadata: &mut NamedTypesDecorator, prefix: &str) -> Result<()> {
    let indexes: Vec<u32> = metadata.all().iter().map(|link| link.index).collect();
    for index in indexes {
        if metadata
            .get_name(index)?
            .is_some_and(|name| name.starts_with(prefix))
        {
            metadata.remove_name(index)?;
            return Ok(());
        }
    }
    panic!("no marker starts with {prefix}");
}

#[test]
fn a_lost_current_branch_marker_is_written_again() -> Result<()> {
    reopened_after(|metadata| remove_marker(metadata, "__vc:current="))?;
    Ok(())
}

#[test]
fn a_lost_default_branch_marker_keeps_the_transitions_it_owned() -> Result<()> {
    reopened_after(|metadata| remove_marker(metadata, "__vc:branch:"))?;
    Ok(())
}

#[test]
fn a_transition_marker_that_cannot_be_read_is_ignored_while_nothing_moves() -> Result<()> {
    reopened_after(|metadata| {
        for (index, name) in [
            (40, "__vc:trans:not-a-number:branch=main"),
            (41, "__vc:trans:no-branch-here"),
        ] {
            metadata.ensure_created(index);
            metadata.set_name(index, name)?;
        }
        Ok(())
    })?;
    Ok(())
}
