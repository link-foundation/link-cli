//! Full-width protocol contracts over privately staged causal links.
#![cfg(feature = "causal-sessions")]
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_projection::CausalConflictPolicy,
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::protocol::{
    decode_document, encode_document, error_message, execute_request_for, format_document,
    links_operations::{
        parse_count, parse_link_reply_for, parse_links_for, parse_name, GenericLinksOperation,
    },
    parse_document, query_document, BinaryLinoOptions, DecodeLimits, LinoDocument,
};
use link_cli::{GenericLink, GenericQueryProcessor, LinksStorage};

fn workspace() -> CausalWorkspace {
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
    CausalWorkspace::new(
        &[retained],
        1,
        32,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap()
}

fn invoke(ws: &mut CausalWorkspace, operation: GenericLinksOperation<u128>) -> LinoDocument {
    let document = operation.to_document();
    execute_request_for(
        &mut ws.named_links(),
        &GenericQueryProcessor::<u128>::new(false),
        &document,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn protocol_operations_preserve_maximum_identity_and_separate_wildcards() {
    let mut ws = workspace();
    let maximum = u128::from(u64::MAX);
    let reply = invoke(
        &mut ws,
        GenericLinksOperation::<u128>::Create {
            source: maximum,
            target: 1,
        },
    );
    assert!(
        error_message(&reply).is_none(),
        "{}",
        format_document(&reply)
    );
    assert_eq!(ws.get_link(1), Some(GenericLink::new(1, u64::MAX, 1)));
    let all = invoke(
        &mut ws,
        GenericLinksOperation::<u128>::Each(vec![None, None, None]),
    );
    assert_eq!(
        parse_links_for::<u128>(&all).unwrap(),
        vec![
            GenericLink::new(1, maximum, 1),
            GenericLink::new(maximum, maximum, maximum)
        ]
    );
    let only_maximum = invoke(
        &mut ws,
        GenericLinksOperation::<u128>::Each(vec![Some(maximum)]),
    );
    assert_eq!(
        parse_links_for::<u128>(&only_maximum).unwrap(),
        vec![GenericLink::new(maximum, maximum, maximum)]
    );
    assert_eq!(
        parse_count(&invoke(
            &mut ws,
            GenericLinksOperation::<u128>::Count(vec![])
        ))
        .unwrap(),
        2
    );
    let reply = invoke(
        &mut ws,
        GenericLinksOperation::<u128>::Update {
            index: 1,
            source: 1,
            target: maximum,
        },
    );
    assert!(error_message(&reply).is_none());
    assert_eq!(ws.get_link(1), Some(GenericLink::new(1, 1, u64::MAX)));
    let reply = invoke(&mut ws, GenericLinksOperation::<u128>::Delete(maximum));
    assert!(error_message(&reply).is_none());
    // The raw engine adapter retains its established non-cascading semantics.
    assert!(ws.get_link(1).is_some());
    assert!(ws.get_link(u64::MAX).is_none());
    assert_eq!(ws.finish().unwrap().len(), 2);
    assert!(error_message(&invoke(&mut ws, GenericLinksOperation::<u128>::Delete(1))).is_some());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn full_width_protocol_names_are_owned_and_private_until_preparation() {
    let mut ws = workspace();
    let maximum = u128::from(u64::MAX);
    for name in ["", "18446744073709551615", "\u{feff}雪\0\n"] {
        let named = invoke(
            &mut ws,
            GenericLinksOperation::<u128>::SetName(maximum, name.into()),
        );
        assert_eq!(parse_link_reply_for::<u128>(&named).unwrap(), Some(maximum));
        let mut reply = invoke(&mut ws, GenericLinksOperation::<u128>::GetName(maximum));
        assert_eq!(parse_name(&reply).unwrap().as_deref(), Some(name));
        reply.clear();
        assert_eq!(ws.get_name(u64::MAX).as_deref(), Some(name));
        let found = invoke(
            &mut ws,
            GenericLinksOperation::<u128>::GetByName(name.into()),
        );
        assert_eq!(parse_link_reply_for::<u128>(&found).unwrap(), Some(maximum));
        assert!(invoke(&mut ws, GenericLinksOperation::<u128>::RemoveName(maximum)).is_empty());
        assert_eq!(
            parse_name(&invoke(
                &mut ws,
                GenericLinksOperation::<u128>::GetName(maximum)
            ))
            .unwrap(),
            None
        );
    }
    assert!(ws.finish().unwrap().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn query_envelope_keeps_numeric_wildcard_variable_and_control_names_in_text_and_binary() {
    for name in [
        "4294967312",
        "18446744073709551615",
        "*",
        "$x",
        "",
        "'\"雪\0\n",
    ] {
        let mut ws = workspace();
        ws.create_link(u64::MAX, 1).unwrap();
        invoke(
            &mut ws,
            GenericLinksOperation::<u128>::SetName(1, name.into()),
        );
        let reference = links_notation::encode_reference_literal(name);
        let text = format!("(({reference}: $s $t)) (({reference}: $t $s))");
        let document = query_document(&text);
        assert_eq!(
            parse_document(&format_document(&document)).unwrap(),
            document
        );
        let packet = encode_document(&document, BinaryLinoOptions::default()).unwrap();
        let decoded = decode_document(&packet, &DecodeLimits::default()).unwrap();
        assert_eq!(decoded, document);
        let reply = execute_request_for(
            &mut ws.named_links(),
            &GenericQueryProcessor::<u128>::new(false),
            &decoded,
        );
        assert!(
            error_message(&reply).is_none(),
            "{}",
            format_document(&reply)
        );
        assert_eq!(
            ws.get_link(1),
            Some(GenericLink::new(1, 1, u64::MAX)),
            "{name:?}"
        );
        assert_eq!(ws.get_name(1).as_deref(), Some(name));
        assert_eq!(
            ws.get_link(u64::MAX),
            Some(GenericLink::new(u64::MAX, u64::MAX, u64::MAX))
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn protocol_overflow_and_malformed_envelopes_leave_rows_and_cursor_unchanged() {
    let mut ws = workspace();
    let before = ws.rows();
    for document in [
        parse_document("(create: (18446744073709551616 1))").unwrap(),
        parse_document("(create: (-1 1))").unwrap(),
        parse_document("(query: (nested text))").unwrap(),
        query_document("() ((1: 1 1) (18446744073709551616: 0 0))"),
        query_document(&" ".repeat(1_048_577)),
    ] {
        let reply = execute_request_for(
            &mut ws.named_links(),
            &GenericQueryProcessor::<u128>::new(false),
            &document,
        );
        assert!(
            error_message(&reply).is_some(),
            "{}",
            format_document(&reply)
        );
        assert_eq!(ws.rows(), before);
    }
    assert_eq!(ws.create_link(0, 0).unwrap(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bounded_protocol_admission_precedes_formatting_and_every_operation() {
    use link_cli::protocol::links_operations::ANY;
    use links_notation::LiNo;
    let mut ws = workspace();
    let before = ws.rows();
    let mut nested = LiNo::Ref(ANY.into());
    for _ in 0..70 {
        nested = LiNo::Link {
            id: None,
            values: vec![nested],
        };
    }
    let requests = [
        vec![nested],
        vec![LiNo::Link {
            id: None,
            values: vec![LiNo::Ref("x".into()); 4097],
        }],
        GenericLinksOperation::<u128>::SetName(u128::from(u64::MAX), "n".repeat(1_048_577))
            .to_document(),
        GenericLinksOperation::<u128>::Each(vec![Some(u128::from(u64::MAX) + 1)]).to_document(),
        GenericLinksOperation::<u128>::Count(vec![Some(u128::from(u64::MAX) + 1)]).to_document(),
    ];
    for document in requests {
        let reply = execute_request_for(
            &mut ws.named_links(),
            &GenericQueryProcessor::<u128>::new(false),
            &document,
        );
        assert!(error_message(&reply).is_some());
        assert_eq!(ws.rows(), before);
    }
    assert_eq!(ws.create_link(0, 0).unwrap(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ordinary_full_width_protocol_and_original_u32_facade_share_the_same_path() {
    use link_cli::engine::{Engine, EngineOptions, MemoryLog};
    use link_cli::protocol::{execute_request, LinksOperation};
    use link_cli::QueryProcessor;
    let engine = Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    let reply = execute_request(
        &mut tx,
        &QueryProcessor::new(false),
        &LinksOperation::Create {
            source: 1,
            target: 1,
        }
        .to_document(),
    );
    assert!(error_message(&reply).is_none());
    let reply = execute_request(&mut tx, &QueryProcessor::new(false), &query_document(""));
    assert_eq!(
        parse_links_for::<u32>(&reply).unwrap(),
        vec![GenericLink::new(1, 1, 1)]
    );
    assert!(engine.snapshot().unwrap().links().is_empty());
    tx.commit().unwrap();

    let engine = Engine::<u64, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut tx = engine.begin().unwrap();
    let wide = u64::from(u32::MAX) + 17;
    tx.ensure_link_created(wide).unwrap();
    tx.update_link(wide, wide, wide).unwrap();
    let reply = execute_request_for(
        &mut tx,
        &GenericQueryProcessor::<u64>::new(false),
        &GenericLinksOperation::<u64>::Each(vec![Some(wide)]).to_document(),
    );
    assert_eq!(
        parse_links_for::<u64>(&reply).unwrap(),
        vec![GenericLink::new(wide, wide, wide)]
    );
    assert!(engine.snapshot().unwrap().links().is_empty());
    tx.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bounded_protocol_parser_admits_grammar_before_canonical_conversion() {
    use link_cli::protocol::parse_document_with_limits;
    let limits = link_cli::query_options::QueryLimits::causal();
    assert!(parse_document_with_limits(" \n", limits)
        .unwrap()
        .is_empty());
    let text = "(query: '(((:)))')";
    assert_eq!(
        parse_document_with_limits(text, limits).unwrap(),
        parse_document(text).unwrap()
    );
    for text in [
        "(".into(),
        "(".repeat(70) + "x" + &")".repeat(70),
        "x ".repeat(4097),
        "x".repeat(1_048_577),
    ] {
        assert!(parse_document_with_limits(&text, limits).is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_full_width_query_vectors_preserve_private_rows_and_allocation() {
    use serde_json::{json, Value};
    let vectors: Value =
        serde_json::from_str(include_str!("../../docs/protocol/causal-query-v1.json")).unwrap();
    assert_eq!(vectors["version"], 1);
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let number = |value: &Value| value.as_str().unwrap().parse::<u64>().unwrap();
        let retained = case["seed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let id = number(&row["index"]);
                CausalVersion::advance(
                    id,
                    &format!("{id:032x}"),
                    0,
                    &[],
                    Some(CausalPayload {
                        source: number(&row["source"]),
                        target: number(&row["target"]),
                        name: row["name"].as_str().map(str::to_owned),
                    }),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let mut ws = CausalWorkspace::new(
            &retained,
            number(&case["first"]),
            number(&case["last"]),
            CausalConflictPolicy::Reject,
            CausalWorkspaceOptions::default(),
        )
        .unwrap();
        for query in case["queries"].as_array().unwrap() {
            let before = ws.rows();
            let reply = execute_request_for(
                &mut ws.named_links(),
                &GenericQueryProcessor::<u128>::new(false),
                &query_document(query["text"].as_str().unwrap()),
            );
            assert_eq!(
                error_message(&reply).is_some(),
                query["error"].as_bool().unwrap(),
                "{case}: {reply:?}"
            );
            if query["error"] == true {
                assert_eq!(ws.rows(), before, "{case}");
            }
        }
        let rows: Vec<_> = ws
            .rows()
            .into_iter()
            .map(|row| json!({"index":row.link.index.to_string(), "source":row.link.source.to_string(), "target":row.link.target.to_string(), "name":row.name}))
            .collect();
        assert_eq!(json!(rows), case["rows"], "{case}");
        let next = ws.create_link(0, 0).unwrap();
        assert_eq!(next, number(&case["next"]), "{case}");
        ws.delete_link(next).unwrap();
        assert_eq!(ws.finish().unwrap().len() as u64, case["edits"]);
    }
}
