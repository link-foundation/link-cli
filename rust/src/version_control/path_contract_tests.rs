//! A shallow recursive oracle checks compatibility independently of traversal.
use super::*;
use crate::LinkStorage;

fn reference(vc: &VersionControlDecorator, name: &str, visited: &mut HashSet<String>) -> Vec<i64> {
    let Some(info) = vc.branches.get(name) else {
        return vec![];
    };
    if !visited.insert(name.to_owned()) {
        return vec![];
    }
    let mut inherited = info
        .parent
        .as_deref()
        .map(|parent| reference(vc, parent, visited))
        .unwrap_or_default();
    inherited.retain(|sequence| *sequence <= info.fork_seq);
    let mut own: Vec<_> = vc
        .transition_branches
        .iter()
        .filter(|(sequence, owner)| owner.as_str() == name && **sequence <= info.head)
        .map(|(sequence, _)| *sequence)
        .collect();
    own.sort_unstable();
    inherited.extend(own);
    inherited
}

#[test]
fn iterative_paths_match_shallow_reference_for_cutoffs_order_and_broken_chains() -> Result<()> {
    let named = || {
        NamedTypesDecorator::from_link_storages(
            LinkStorage::in_memory(false),
            LinkStorage::in_memory(false),
        )
    };
    let tx = TransactionsDecorator::new(
        named(),
        named(),
        crate::LogRetentionPolicy::Infinite,
        crate::CommitMode::Sync,
        false,
    )?;
    let mut vc = VersionControlDecorator::new(tx, named(), false)?;
    let names = ["root", "middle", "leaf", "unrelated", "missing"];
    let mut state = 0x138u64;
    for case in 0..512 {
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            state >> 32
        };
        vc.branches.clear();
        for (name, parent) in [
            ("root", None),
            ("middle", Some("root")),
            ("leaf", Some("middle")),
            ("unrelated", None),
        ] {
            let parent = match (case % 4, name) {
                (1, "root") => Some("leaf"), // Legacy cycle truncation, not movement acceptance.
                (2, "middle") => Some("missing"),
                _ => parent,
            };
            vc.branches.insert(
                name.into(),
                BranchInfo::new(
                    name.into(),
                    parent.map(str::to_owned),
                    (next() % 14) as i64,
                    (next() % 14) as i64,
                ),
            );
        }
        vc.transition_branches.clear();
        for sequence in 1..=12 {
            vc.transition_branches
                .insert(sequence, names[(next() as usize) % names.len()].into());
        }
        for target in names {
            assert_eq!(
                vc.build_branch_seqs(target),
                reference(&vc, target, &mut HashSet::new()),
                "case {case}, target {target}"
            );
        }
    }
    Ok(())
}
