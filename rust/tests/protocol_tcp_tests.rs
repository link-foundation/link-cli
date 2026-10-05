//! LiNo substitution operations over TCP (issue #105).

mod common;

use common::{protocols, RunningServer};
use link_cli::protocol::{
    error_message, parse_document, AcceptedProtocols, ArityRange, BinaryLinoOptions,
    BinaryLinoProtocol, LinksClient, ProtocolError, ServerOptions, TextLinoProtocol,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

#[test]
fn crud_works_over_every_protocol() {
    for protocol in protocols() {
        let name = format!("{protocol:?}");
        let server = RunningServer::start(ServerOptions::default(), false);
        let mut client = server.client(protocol);

        // Create.
        assert_eq!(client.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
        assert_eq!(client.query_text("() ((2 2))").unwrap(), "() ((2: 2 2))");
        // Read.
        assert_eq!(
            client.query_text("").unwrap(),
            "(1: 1 1)\n(2: 2 2)",
            "{name}"
        );
        assert_eq!(
            client.query_text("((1: 1 1)) ((1: 1 1))").unwrap(),
            "((1: 1 1)) ((1: 1 1))",
            "{name}"
        );
        // A query that creates and deletes nothing changes nothing.
        assert_eq!(client.query_text("() ()").unwrap(), "", "{name}");
        // Update.
        assert_eq!(
            client.query_text("((1: 1 1)) ((1: 1 2))").unwrap(),
            "((1: 1 1)) ((1: 1 2))",
            "{name}"
        );
        // Delete.
        assert_eq!(
            client.query_text("((1: 1 2)) ()").unwrap(),
            "((1: 1 2)) ()",
            "{name}"
        );
        assert_eq!(client.query_text("").unwrap(), "(2: 2 2)", "{name}");
    }
}

#[test]
fn text_and_binary_replies_are_identical_documents() {
    let options = ServerOptions {
        auto_create_missing_references: true,
        ..ServerOptions::default()
    };
    let server = RunningServer::start(options, true);
    let mut text = server.client(TextLinoProtocol::new());
    let mut binary = server.client(BinaryLinoProtocol::with_options(
        BinaryLinoOptions::default()
            .with_external_references(true)
            .with_arity(ArityRange::at_least(1)),
    ));
    text.query("() ((child: father mother))").unwrap();
    binary.query("() (('two words': 'it''s' \"x\"))").unwrap();
    let from_text = text.query("").unwrap();
    let from_binary = binary.query("").unwrap();
    assert_eq!(from_text, from_binary);
    let listing = text.query_text("").unwrap();
    assert!(listing.contains("(child: father mother)"), "{listing}");
    assert!(listing.contains("('two words': \"it's\" x)"), "{listing}");
}

#[test]
fn query_errors_come_back_as_remote_errors() {
    let server = RunningServer::start(ServerOptions::default(), false);
    for protocol in protocols() {
        let mut client = server.client(protocol);
        match client.query("((99: 1 1)) ()") {
            Err(ProtocolError::Remote(message)) => assert!(!message.is_empty()),
            other => panic!("expected a remote error, got {other:?}"),
        }
        // The connection stays usable after an error reply.
        assert_eq!(client.query_text("").unwrap(), "");
    }
}

#[test]
fn a_raw_text_session_works_with_crlf() {
    let server = RunningServer::start(ServerOptions::default(), false);
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(b"() ((1 1))\r\n.\r\n").unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut reply = String::new();
    reader.read_line(&mut reply).unwrap();
    assert_eq!(reply, "() ((1: 1 1))\n");
    reply.clear();
    reader.read_line(&mut reply).unwrap();
    assert_eq!(reply, ".\n");

    stream.write_all(b".\n").unwrap();
    reply.clear();
    reader.read_line(&mut reply).unwrap();
    assert_eq!(reply, "(1: 1 1)\n");
}

#[test]
fn malformed_messages_get_an_error_and_the_connection_closes() {
    let server = RunningServer::start(ServerOptions::default(), false);
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    // A binary header announcing more links than the default limit allows.
    stream
        .write_all(&[0x10, 0xff, 0xff, 0xff, 0xff, 0x0f])
        .unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("(error: "), "{reply}");
    assert!(reply.ends_with("\n.\n"), "{reply}");
}

#[test]
fn a_stopped_server_executes_nothing() {
    let mut server = RunningServer::start(ServerOptions::default(), false);
    let mut client = server.client(TextLinoProtocol::new());
    assert_eq!(client.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
    server.stop();
    // The connection outlives `serve`, but the store is gone.
    assert_eq!(
        client.query_text("").unwrap_err().to_string(),
        "server error: server is shutting down"
    );
}

#[test]
fn servers_can_restrict_the_accepted_protocol() {
    for accept in [AcceptedProtocols::Binary, AcceptedProtocols::Text] {
        let options = ServerOptions {
            accept,
            ..ServerOptions::default()
        };
        let server = RunningServer::start(options, false);
        let (mut refused, mut accepted) = if accept == AcceptedProtocols::Binary {
            (
                server.client(TextLinoProtocol::new()),
                server.client(BinaryLinoProtocol::new()),
            )
        } else {
            (
                server.client(BinaryLinoProtocol::new()),
                server.client(TextLinoProtocol::new()),
            )
        };
        assert_eq!(
            refused.query("() ((1 1))").unwrap_err().to_string(),
            "server error: this server does not accept this protocol"
        );
        assert_eq!(accepted.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
    }
}

#[test]
fn only_error_documents_carry_an_error_message() {
    let message = |text: &str| error_message(&parse_document(text).unwrap()).map(str::to_string);
    assert_eq!(
        message("(error: 'it failed')").as_deref(),
        Some("it failed")
    );
    for other in [
        "",
        "(error: a b)",
        "(error: (a b))",
        "(fault: a)",
        "(error: a) (error: b)",
    ] {
        assert_eq!(message(other), None, "{other}");
    }
}

#[test]
fn tracing_servers_keep_serving() {
    let options = ServerOptions {
        trace: true,
        ..ServerOptions::default()
    };
    let server = RunningServer::start(options, false);
    let mut client = server.client(TextLinoProtocol::new());
    assert_eq!(client.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
    drop(client);
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    stream.write_all(b"() ((1 1))\n").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert_eq!(
        reply,
        "(error: \"malformed message: stream ended before the '.' terminator line\")\n.\n"
    );
    let mut client = server.client(TextLinoProtocol::new());
    assert_eq!(client.query_text("").unwrap(), "(1: 1 1)");
}

#[test]
fn concurrent_clients_share_one_store() {
    let server = RunningServer::start(ServerOptions::default(), true);
    let address = server.address;
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            thread::spawn(move || {
                let mut client = if worker % 2 == 0 {
                    LinksClient::connect(address, TextLinoProtocol::new()).unwrap()
                } else {
                    LinksClient::connect(address, BinaryLinoProtocol::new()).unwrap()
                };
                for item in 0..5 {
                    let name = format!("w{worker}i{item}");
                    let reply = client.query_text(&format!("() (({name}: {name} {name}))"));
                    let link = format!("({name}: {name} {name})");
                    assert_eq!(reply.unwrap(), format!("() ({link})"));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let mut client = server.client(TextLinoProtocol::new());
    let listing = client.query_text("").unwrap();
    assert_eq!(listing.lines().count(), 20, "{listing}");
}

#[test]
fn shutdown_stops_the_server() {
    let mut server = RunningServer::start(ServerOptions::default(), false);
    let mut client = server.client(TextLinoProtocol::new());
    client.query("() ((1 1))").unwrap();
    server.shutdown.shutdown();
    server.thread.take().unwrap().join().unwrap();
    assert!(client.query("").is_err());
}
