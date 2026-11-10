//! Finite named-reply amplification controls for checked causal protocols.
#![cfg(feature = "causal-sessions")]
use link_cli::engine::{
    causal_projection::CausalConflictPolicy,
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::protocol::{error_message, execute_request_for, query_document};
use link_cli::{GenericQueryProcessor, LinksStorage};

fn workspace() -> CausalWorkspace {
    let mut workspace = CausalWorkspace::new(
        &[],
        1,
        32,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    for id in 1..=20 {
        assert_eq!(workspace.create_link(1, id).unwrap(), id);
    }
    workspace.set_name(1, Some("n".repeat(70_000))).unwrap();
    workspace
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn oversized_named_query_reply_rejects_before_changing_private_rows() {
    let mut workspace = workspace();
    let before = workspace.rows();
    let reply = execute_request_for(
        &mut workspace.named_links(),
        &GenericQueryProcessor::<u128>::new(false),
        &query_document("(($i: $s $t)) (($i: $t $s))"),
    );
    assert!(error_message(&reply).is_some_and(|message| message.contains("reply byte limit")));
    assert_eq!(workspace.rows(), before);
    assert_eq!(workspace.create_link(0, 0).unwrap(), 21);
    workspace.set_name(1, None).unwrap();
    let reply = execute_request_for(
        &mut workspace.named_links(),
        &GenericQueryProcessor::<u128>::new(false),
        &query_document("(($i: $s $t)) (($i: $t $s))"),
    );
    assert!(error_message(&reply).is_none());
    assert_eq!(workspace.get_link(2).unwrap().source, 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn oversized_named_export_rejects_before_accumulating_the_complete_reply() {
    let mut workspace = workspace();
    let before = workspace.rows();
    let reply = execute_request_for(
        &mut workspace.named_links(),
        &GenericQueryProcessor::<u128>::new(false),
        &[],
    );
    assert!(error_message(&reply).is_some_and(|message| message.contains("reply byte limit")));
    assert_eq!(workspace.rows(), before);
    for id in 2..=20 {
        workspace.delete_link(id).unwrap();
    }
    let reply = execute_request_for(
        &mut workspace.named_links(),
        &GenericQueryProcessor::<u128>::new(false),
        &[],
    );
    assert!(error_message(&reply).is_none());
    assert_eq!(reply.len(), 1);
}
