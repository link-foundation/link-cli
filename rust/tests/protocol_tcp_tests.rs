//! LiNo substitution operations over TCP (issue #105).

use link_cli::protocol::{
    AcceptedProtocols, BinaryLinoOptions, BinaryLinoProtocol, LinksClient, LinksServer,
    LinoProtocol, ProtocolError, ServerOptions, TextLinoProtocol,
};
use link_cli::{LinkStorage, NamedTypesDecorator};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tempfile::NamedTempFile;

struct RunningServer {
    address: SocketAddr,
    shutdown: link_cli::protocol::ShutdownHandle,
    thread: Option<JoinHandle<()>>,
    _database: NamedTempFile,
}

impl RunningServer {
    fn start(options: ServerOptions, named: bool) -> Self {
        let database = NamedTempFile::new().unwrap();
        let path = database.path().to_path_buf();
        let server = LinksServer::bind("127.0.0.1:0", options).unwrap();
        let address = server.local_addr().unwrap();
        let shutdown = server.shutdown_handle().unwrap();
        let thread = thread::spawn(move || {
            if named {
                let mut storage = NamedTypesDecorator::new(&path, false).unwrap();
                server.serve(&mut storage).unwrap();
            } else {
                let mut storage = LinkStorage::new(&path, false).unwrap();
                server.serve(&mut storage).unwrap();
            }
        });
        Self {
            address,
            shutdown,
            thread: Some(thread),
            _database: database,
        }
    }

    fn client(&self, protocol: impl LinoProtocol + 'static) -> LinksClient {
        LinksClient::connect(self.address, protocol).unwrap()
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn protocols() -> Vec<Box<dyn LinoProtocol>> {
    let mut protocols: Vec<Box<dyn LinoProtocol>> = vec![Box::new(TextLinoProtocol::new())];
    for external_references in [false, true] {
        for sequences in [false, true] {
            for progressive_widths in [false, true] {
                protocols.push(Box::new(BinaryLinoProtocol::with_options(
                    BinaryLinoOptions {
                        external_references,
                        sequences,
                        progressive_widths,
                    },
                )));
            }
        }
    }
    protocols
}

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
            .with_sequences(true),
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
fn servers_can_restrict_the_accepted_protocol() {
    let options = ServerOptions {
        accept: AcceptedProtocols::Binary,
        ..ServerOptions::default()
    };
    let server = RunningServer::start(options, false);
    let mut text = server.client(TextLinoProtocol::new());
    assert!(matches!(
        text.query("() ((1 1))"),
        Err(ProtocolError::Remote(_))
    ));
    let mut binary = server.client(BinaryLinoProtocol::new());
    assert_eq!(binary.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
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
                    assert_eq!(reply.unwrap(), format!("({link}) ({link})"));
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
