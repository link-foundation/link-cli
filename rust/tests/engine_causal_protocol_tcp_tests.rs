//! Real text/binary connections use the selected full-width server path.
#![cfg(all(feature = "causal-sessions", not(target_arch = "wasm32")))]
mod common;
use common::protocols;
use link_cli::engine::{
    causal::{CausalPayload, CausalVersion},
    causal_projection::CausalConflictPolicy,
    causal_session::{CausalWorkspace, CausalWorkspaceOptions},
};
use link_cli::protocol::{
    error_message,
    links_operations::{parse_links_for, GenericLinksOperation},
    LinksClient, LinksServer, LinoProtocol, ServerOptions, TextLinoProtocol,
};
use link_cli::GenericLink;

#[test]
fn full_width_server_and_lossless_client_interoperate_in_every_existing_protocol() {
    for protocol in protocols() {
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
        let server = LinksServer::bind(
            "127.0.0.1:0",
            ServerOptions {
                trace: true,
                ..Default::default()
            },
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let stop = server.shutdown_handle().unwrap();
        let serving = std::thread::spawn(move || {
            server.serve_for(&mut workspace.named_links()).unwrap();
            workspace
        });
        // Collect the result before stopping so every failure still reaps the server.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut client = LinksClient::connect(address, protocol).unwrap();
            client
                .query_lossless("() ((1: 18446744073709551615 1))")
                .unwrap();
            for name in [
                "4294967312",
                "18446744073709551615",
                "*",
                "$x",
                "",
                "雪\0\n",
            ] {
                client
                    .request(&GenericLinksOperation::<u128>::SetName(1, name.into()).to_document())
                    .unwrap();
                let name = links_notation::encode_reference_literal(name);
                client
                    .query_lossless(&format!("(({name}: $s $t)) (({name}: $t $s))"))
                    .unwrap();
            }
            let rows = client
                .request(&GenericLinksOperation::<u128>::Each(vec![]).to_document())
                .unwrap();
            assert_eq!(
                parse_links_for::<u128>(&rows).unwrap(),
                vec![
                    GenericLink::new(1, maximum.into(), 1),
                    GenericLink::new(maximum.into(), maximum.into(), maximum.into())
                ]
            );
            assert!(client
                .query_lossless("() ((18446744073709551616: 0 0))")
                .is_err());
            assert_eq!(
                parse_links_for::<u128>(
                    &client
                        .request(
                            &GenericLinksOperation::<u128>::Each(vec![Some(maximum.into())])
                                .to_document()
                        )
                        .unwrap()
                )
                .unwrap()
                .len(),
                1
            );
        }));
        stop.shutdown();
        let mut workspace = serving.join().unwrap();
        assert_eq!(workspace.finish().unwrap().len(), 1);
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}

#[test]
fn checked_server_admits_raw_text_bytes_before_discarding_comments() {
    let mut workspace = CausalWorkspace::new(
        &[],
        1,
        8,
        CausalConflictPolicy::Reject,
        CausalWorkspaceOptions::default(),
    )
    .unwrap();
    let server = LinksServer::bind("127.0.0.1:0", ServerOptions::default()).unwrap();
    let address = server.local_addr().unwrap();
    let stop = server.shutdown_handle().unwrap();
    let serving = std::thread::spawn(move || {
        server.serve_for(&mut workspace.named_links()).unwrap();
        workspace
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        // Finite input just beyond the checked policy; the old parser discards
        // the whole comment and mistakenly accepts this as an empty export.
        TextLinoProtocol::write_text(&mut stream, &format!("#{}", "x".repeat(1 << 20))).unwrap();
        let reply = TextLinoProtocol::new()
            .read_document(&mut std::io::BufReader::new(stream))
            .unwrap()
            .unwrap();
        assert!(error_message(&reply).is_some_and(|message| message.contains("limit exceeded")));
    }));
    stop.shutdown();
    let mut workspace = serving.join().unwrap();
    assert!(workspace.finish().unwrap().is_empty());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
