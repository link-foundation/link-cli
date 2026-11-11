//! The LiNo server and client over every inter-process communication
//! transport the platform offers (issue #140): the same queries, replies and
//! pushes as over TCP, in both the text and the binary protocol.

mod common;

use common::protocols;
use link_cli::protocol::ipc;
use link_cli::protocol::{
    LinksClient, LinksOperation, LinksServer, ProtocolError, RemoteLinks, ServerOptions,
    ShutdownHandle, TextLinoProtocol,
};
use link_cli::LinkStorage;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tempfile::{NamedTempFile, TempDir};

const PUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// A server listening on `endpoints` and serving a fresh temporary store.
struct IpcServer {
    endpoint: String,
    shutdown: ShutdownHandle,
    thread: Option<JoinHandle<()>>,
    _database: NamedTempFile,
}

impl IpcServer {
    fn start(endpoints: &[String]) -> Self {
        let database = NamedTempFile::new().unwrap();
        let path = database.path().to_path_buf();
        let server = LinksServer::listen_all(endpoints, ServerOptions::default()).unwrap();
        let endpoint = server.endpoint();
        let shutdown = server.shutdown_handle().unwrap();
        let thread = thread::spawn(move || {
            let mut storage = LinkStorage::new(&path, false).unwrap();
            server.serve(&mut storage).unwrap();
        });
        Self {
            endpoint,
            shutdown,
            thread: Some(thread),
            _database: database,
        }
    }

    /// The endpoints the server listens on.
    fn endpoints(&self) -> Vec<&str> {
        self.endpoint.split(' ').collect()
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One endpoint of every kind that can both listen and connect here, each
/// in its own temporary directory where it needs one.
fn local_endpoints(dir: &TempDir) -> Vec<String> {
    let path = |name: &str| dir.path().join(name).display().to_string();
    let mut endpoints = vec![
        "tcp:127.0.0.1:0".to_string(),
        "udp:127.0.0.1:0".to_string(),
        format!("unix:{}", path("links.sock")),
        format!("file:{}", path("file")),
        format!("mmap:{}", path("mmap")),
        format!("shm:{}", path("shm")),
        format!("signal:{}", path("signal")),
    ];
    if cfg!(unix) {
        endpoints.push(format!("fifo:{}", path("fifo")));
    } else {
        endpoints.push(format!("pipe:clink-test-{}", std::process::id()));
    }
    if cfg!(target_os = "linux") {
        endpoints.push(format!("mq:{}", path("mq")));
    }
    if cfg!(any(target_os = "linux", target_vendor = "apple")) {
        endpoints.push(format!("sysvmq:{}", path("sysvmq")));
    }
    if cfg!(windows) {
        endpoints.push(format!("mailslot:{}", path("mailslot")));
        endpoints.push(format!("copydata:{}", path("copydata")));
        if cfg!(target_pointer_width = "64") {
            endpoints.push(format!("alpc:{}", path("alpc")));
        }
    }
    endpoints
}

#[test]
fn every_local_kind_serves_queries_in_both_protocols() {
    let dir = tempfile::tempdir().unwrap();
    for endpoint in local_endpoints(&dir) {
        let server = IpcServer::start(std::slice::from_ref(&endpoint));
        let connect = server.endpoints()[0].to_string();
        for protocol in protocols() {
            let name = format!("{endpoint} {protocol:?}");
            let mut client = LinksClient::connect_endpoint(&connect, protocol).unwrap();
            assert_eq!(
                client.kind(),
                ipc::Endpoint::parse(&connect).unwrap().kind(),
                "{name}"
            );
            assert_eq!(client.query_text("() ((1 1))").unwrap(), "() ((1: 1 1))");
            assert_eq!(client.query_text("() ((2 2))").unwrap(), "() ((2: 2 2))");
            assert_eq!(
                client.query_text("").unwrap(),
                "(1: 1 1)\n(2: 2 2)",
                "{name}"
            );
            assert_eq!(
                client.query_text("((1: 1 1)) ((1: 1 2))").unwrap(),
                "((1: 1 1)) ((1: 1 2))",
                "{name}"
            );
            client.query_text("((1: 1 2)) ()").unwrap();
            client.query_text("((2: 2 2)) ()").unwrap();
            assert_eq!(client.query_text("").unwrap(), "", "{name}");
        }
    }
}

#[test]
fn clients_of_different_kinds_share_one_store_and_its_pushes() {
    let dir = tempfile::tempdir().unwrap();
    let endpoints = local_endpoints(&dir);
    let server = IpcServer::start(&endpoints);
    let listening = server.endpoints();
    assert_eq!(listening.len(), endpoints.len(), "{}", server.endpoint);

    // The first kind subscribes; every other kind writes once.
    let subscriber = RemoteLinks::connect_endpoint(listening[0], TextLinoProtocol::new()).unwrap();
    let subscription = subscriber.subscribe(&[None, None, None]).unwrap();
    for (index, endpoint) in listening.iter().enumerate().skip(1) {
        let writer = RemoteLinks::connect_endpoint(endpoint, TextLinoProtocol::new()).unwrap();
        writer
            .changes(&LinksOperation::Create {
                source: 0,
                target: 0,
            })
            .unwrap();
        let notification = subscriber
            .receive_notification(PUSH_TIMEOUT)
            .unwrap()
            .unwrap_or_else(|| panic!("no push for the write over {endpoint}"));
        assert_eq!(notification.subscription, subscription);
        assert_eq!(notification.changes.len(), 1, "{endpoint}");
        assert_eq!(notification.changes[0].1.index, index as u32, "{endpoint}");
    }
    assert_eq!(
        subscriber.count_matching(&[]).unwrap(),
        listening.len() as u32 - 1
    );
}

#[test]
fn a_server_over_a_kind_stops_on_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = format!("shm:{}", dir.path().join("links").display());
    let server = IpcServer::start(&[endpoint.clone()]);
    let mut client = LinksClient::connect_endpoint(&endpoint, TextLinoProtocol::new()).unwrap();
    client.query_text("() ((1 1))").unwrap();
    drop(server);
    match LinksClient::connect_endpoint(&endpoint, TextLinoProtocol::new()) {
        Err(ProtocolError::Io(_)) => {}
        other => panic!("connected to a stopped server: {other:?}"),
    }
}

#[test]
fn local_addr_is_an_error_without_a_tcp_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = format!("unix:{}", dir.path().join("links.sock").display());
    let server = LinksServer::listen(&endpoint, ServerOptions::default()).unwrap();
    assert_eq!(server.endpoint(), endpoint);
    assert!(server.local_addr().is_err());
}
