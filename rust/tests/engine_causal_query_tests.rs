//! The checked query adapter must preserve the complete stored u64 domain.
#![cfg(feature = "causal-sessions")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_projection::CausalConflictPolicy,
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::{GenericLink, GenericQueryProcessor, LinksStorage, NamedTypeLinks};

fn point(id: u64) -> CausalVersion {
    CausalVersion::advance(
        id,
        &format!("{id:032x}"),
        0,
        &[],
        Some(CausalPayload {
            source: id,
            target: id,
            name: None,
        }),
    )
    .unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn owned_query_reservations_survive_explicit_ensure_and_range_exhaustion() {
    let mut workspace = CausalWorkspace::new(
        &[],
        1,
        2,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let query = GenericQueryProcessor::<u128>::new(false);
    assert!(query
        .process_query(&mut workspace.named_links(), "() ((9 9))")
        .is_err());
    assert!(workspace.rows().is_empty());
    workspace.ensure_link_created(2).unwrap();
    workspace.update_link(2, 2, 2).unwrap();
    assert_eq!(workspace.create_link(1, 1).unwrap(), 1);
    assert!(workspace.create_link(0, 0).is_err());
    assert_eq!(workspace.finish().unwrap().len(), 2);

    let maximum = u64::MAX;
    let mut workspace = CausalWorkspace::new(
        &[],
        maximum - 1,
        maximum,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    query
        .process_query(
            &mut workspace.named_links(),
            &format!("() (({} {maximum}) ({maximum} {maximum}))", maximum - 1),
        )
        .unwrap();
    assert_eq!(
        workspace.all_links(),
        vec![
            GenericLink::new(maximum - 1, maximum - 1, maximum),
            GenericLink::new(maximum, maximum, maximum)
        ]
    );
    assert!(workspace.create_link(0, 0).is_err());
    assert_eq!(workspace.finish().unwrap().len(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cartesian_binding_memory_is_refused_before_result_count() {
    let retained = (100..108).map(point).collect::<Vec<_>>();
    let mut workspace = CausalWorkspace::new(
        &retained,
        1,
        8,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let before = workspace.rows();
    let patterns = "($a $b $c $d $e)";
    let error = GenericQueryProcessor::<u128>::new(false)
        .process_query(
            &mut workspace.named_links(),
            &format!("{patterns} {patterns}"),
        )
        .unwrap_err();
    assert!(error.to_string().contains("binding"), "{error}");
    assert_eq!(workspace.rows(), before);
    assert_eq!(workspace.create_link(0, 0).unwrap(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn long_variable_names_are_bounded_by_owned_binding_bytes() {
    let retained = (100..165).map(point).collect::<Vec<_>>();
    let mut workspace = CausalWorkspace::new(
        &retained,
        1,
        8,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    // A finite 128 KiB request would otherwise clone its 64 KiB name 65 times.
    let variable = format!("${}", "v".repeat(65_536));
    let error = GenericQueryProcessor::<u128>::new(false)
        .process_query(
            &mut workspace.named_links(),
            &format!("({variable}) ({variable})"),
        )
        .unwrap_err();
    assert!(error.to_string().contains("binding"), "{error}");
    assert_eq!(workspace.rows().len(), retained.len());
    assert_eq!(workspace.create_link(0, 0).unwrap(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn standard_queries_preserve_full_width_identities_and_keep_work_private() {
    let maximum = u64::MAX;
    let retained = CausalVersion::advance(
        maximum,
        "00000000000000000000000000000001",
        0,
        &[],
        Some(CausalPayload {
            source: maximum,
            target: maximum,
            name: None,
        }),
    )
    .unwrap();
    let mut workspace = CausalWorkspace::new(
        &[retained],
        1,
        8,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let processor = GenericQueryProcessor::<u128>::new(false);
    let created = processor
        .process_query(
            &mut workspace.named_links(),
            &format!("(() ((1: {maximum} {maximum})))"),
        )
        .unwrap();
    assert!(created
        .iter()
        .any(|(_, after)| *after == Some(GenericLink::new(1, maximum as u128, maximum as u128))));
    let matched = processor
        .process_query(
            &mut workspace.named_links(),
            &format!("(({maximum}: $s $t)) (({maximum}: $s $t))"),
        )
        .unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].0.unwrap().index, maximum as u128);
    let all = processor
        .process_query(&mut workspace.named_links(), "((*: $s $t)) ((*: $s $t))")
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(
        workspace.get_link(1),
        Some(GenericLink::new(1, maximum, maximum))
    );
    let edits = workspace.finish().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].address, 1);
    assert_eq!(edits[0].payload.as_ref().unwrap().source, maximum);
    assert!(processor
        .process_query(&mut workspace.named_links(), "(() ((2: 2 2)))")
        .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checked_query_refuses_numeric_overflow_before_an_earlier_definition() {
    let mut workspace = CausalWorkspace::new(
        &[],
        1,
        100,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let query = GenericQueryProcessor::<u128>::new(false);
    for numeric in [
        "18446744073709551616",
        "340282366920938463463374607431768211455",
        "340282366920938463463374607431768211456",
        "-1",
        "+18446744073709551616",
    ] {
        assert!(
            query
                .process_query(
                    &mut workspace.named_links(),
                    &format!("() ((1: 1 1) ({numeric}: 1 1))")
                )
                .is_err(),
            "{numeric}"
        );
        assert!(workspace.rows().is_empty(), "{numeric}");
    }
    // A quoted decimal is an ordinary name, including one beyond u64.
    query
        .process_query(
            &mut workspace.named_links(),
            "() ((\"18446744073709551616\": \"18446744073709551616\" \"18446744073709551616\"))",
        )
        .unwrap();
    assert_eq!(workspace.get_by_name("18446744073709551616"), Some(1));
    assert_eq!(workspace.get_link(1), Some(GenericLink::new(1, 1, 1)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checked_query_bounds_bytes_depth_nodes_and_cartesian_expansion() {
    let versions: Vec<_> = (100..165).map(point).collect();
    let mut workspace = CausalWorkspace::new(
        &versions,
        1,
        99,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let before = workspace.rows();
    let query = GenericQueryProcessor::<u128>::new(false);
    for text in [
        " ".repeat(1024 * 1024 + 1),
        format!("{}1{}", "(".repeat(65), ")".repeat(65)),
        "() ".repeat(4097),
        "(($a: $s $t) ($b: $x $y)) (($a: $s $t) ($b: $x $y))".to_string(),
    ] {
        assert!(query
            .process_query(&mut workspace.named_links(), &text)
            .is_err());
        assert_eq!(workspace.rows(), before);
    }
    assert!(workspace.finish().unwrap().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn checked_named_output_can_be_replayed_with_numeric_and_control_names() {
    let query = GenericQueryProcessor::<u128>::new(false);
    for name in [
        "18446744073709551615",
        "18446744073709551616",
        "*",
        "$x",
        "",
        "# comment",
        "quoted'\"\0\n",
    ] {
        let mut workspace = CausalWorkspace::new(
            &[],
            1,
            10,
            CausalConflictPolicy::Reject,
            CausalWorkspaceOptions::default(),
        )
        .unwrap();
        workspace.ensure_link_created(1).unwrap();
        workspace.update_link(1, 1, 1).unwrap();
        workspace.set_name(1, Some(name.to_string())).unwrap();
        let text = workspace.named_links().lino_lines().unwrap().join("\n");
        assert!(!text.contains('\0'));
        let mut restored = CausalWorkspace::new(
            &[],
            1,
            10,
            CausalConflictPolicy::Reject,
            CausalWorkspaceOptions::default(),
        )
        .unwrap();
        query
            .process_query(&mut restored.named_links(), &format!("() ({text})"))
            .unwrap_or_else(|error| panic!("name={name:?}, output={text:?}: {error}"));
        assert_eq!(restored.get_by_name(name), Some(1), "{name:?}, {text:?}");
        assert_eq!(restored.get_link(1), Some(GenericLink::new(1, 1, 1)));
    }
}
