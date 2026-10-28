//! Install as rust/tests/engine_causal_query_tests.rs after preserving the
//! independently validated raw-session implementation.
#![cfg(feature = "causal-sessions")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_projection::CausalConflictPolicy,
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::{GenericLink, GenericQueryProcessor, LinksStorage};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn standard_queries_preserve_full_width_identities_and_keep_work_private() {
    let maximum = u64::MAX;
    let retained = CausalVersion::advance(
        maximum,
        "00000000000000000000000000000001",
        0,
        &[],
        Some(CausalPayload { source: maximum, target: maximum, name: None }),
    ).unwrap();
    let mut workspace = CausalWorkspace::new(
        &[retained], 1, 8, CausalConflictPolicy::Reject, CausalWorkspaceOptions::default(),
    ).unwrap();
    let processor = GenericQueryProcessor::<u128>::new(false);
    let created = processor.process_query(
        &mut workspace.named_links(), &format!("(() ((1: {maximum} {maximum})))"),
    ).unwrap();
    assert!(created.iter().any(|(_, after)| *after == Some(GenericLink::new(1, maximum as u128, maximum as u128))));
    let matched = processor.process_query(
        &mut workspace.named_links(), &format!("(({maximum}: $s $t)) (({maximum}: $s $t))"),
    ).unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].0.unwrap().index, maximum as u128);
    let all = processor.process_query(
        &mut workspace.named_links(), "((*: $s $t)) ((*: $s $t))",
    ).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(workspace.get_link(1), Some(GenericLink::new(1, maximum, maximum)));
    let edits = workspace.finish().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].address, 1);
    assert_eq!(edits[0].payload.as_ref().unwrap().source, maximum);
    assert!(processor.process_query(&mut workspace.named_links(), "(() ((2: 2 2)))").is_err());
}
