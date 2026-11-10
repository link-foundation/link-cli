//! The HTTP and WebSocket edges of the GraphQL server. `graphql_server.rs` drives
//! the installed command; this suite runs the server inside the test binary, on
//! threads that end with it, so odd bytes can be sent at every step.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::channel;
use std::time::Duration;

use link_cli::graphql::{DeepStore, GraphqlServer, Json, Settings};
use link_cli::NamedTypesDecorator;

const INSERT: &str = "mutation { insert_links(objects: {type_id: 1}) { returning { id } } }";
const WATCH: &str =
    "subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }";

fn start(admin: Option<&str>) -> String {
    let admin = admin.map(str::to_string);
    let (ready, address) = channel();
    std::thread::spawn(move || {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let mut links = NamedTypesDecorator::new(dir.path().join("db.links"), false)
            .unwrap()
            .with_indexed_lookups();
        let mut store = DeepStore::open(&mut links, None).unwrap();
        let package = include_str!("../../docs/protocol/graphql/deep-core.json");
        store.load_package(&Json::parse(package).unwrap()).unwrap();
        let server = GraphqlServer::bind("127.0.0.1:0").unwrap();
        ready
            .send(server.local_addr().unwrap().to_string())
            .unwrap();
        server
            .serve(&mut store, Settings::new(b"s3".to_vec(), admin))
            .unwrap();
    });
    address.recv().unwrap()
}

fn connect(address: &str) -> BufReader<TcpStream> {
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    BufReader::new(stream)
}

/// Sends text on an open connection and reads one response: its head and body.
fn exchange(connection: &mut BufReader<TcpStream>, request: &str) -> (String, String) {
    connection.get_mut().write_all(request.as_bytes()).unwrap();
    let (mut head, mut length) = (String::new(), 0);
    loop {
        let mut line = String::new();
        connection.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
        head.push_str(&line);
    }
    let mut body = vec![0; length];
    connection.read_exact(&mut body).unwrap();
    (head, String::from_utf8(body).unwrap())
}

fn post_text(address: &str, body: &str, headers: &str) -> String {
    let request = format!(
        "POST /v1/graphql HTTP/1.1\r\nHost: x\r\n{headers}Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    exchange(&mut connect(address), &request).1
}

fn post(address: &str, query: &str, headers: &str) -> Json {
    let body = Json::object(vec![("query", Json::str(query))]).write();
    Json::parse(&post_text(address, &body, headers)).unwrap()
}

fn guest_token(address: &str) -> String {
    let answer = post(address, "query { guest { token } }", "");
    let guest = answer.get("data").and_then(|data| data.get("guest"));
    let token = guest.and_then(|guest| guest.get("token"));
    token.and_then(Json::as_str).unwrap().to_string()
}

fn frame(fin_and_opcode: u8, payload: &[u8], masked: bool) -> Vec<u8> {
    let mask_bit = if masked { 0x80 } else { 0 };
    let mut bytes = vec![fin_and_opcode];
    match payload.len() {
        length @ 0..=125 => bytes.push(mask_bit | length as u8),
        length @ 126..=65535 => {
            bytes.push(mask_bit | 126);
            bytes.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            bytes.push(mask_bit | 127);
            bytes.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    if masked {
        let mask = [9u8, 8, 7, 6];
        bytes.extend_from_slice(&mask);
        bytes.extend(payload.iter().enumerate().map(|(at, b)| b ^ mask[at % 4]));
    } else {
        bytes.extend_from_slice(payload);
    }
    bytes
}

struct Socket(BufReader<TcpStream>);

impl Socket {
    /// Upgrades on `path`; the extra lines are added to the request as they are.
    fn open(address: &str, path: &str, lines: &str) -> (Self, String) {
        let mut connection = connect(address);
        let (head, _) = exchange(
            &mut connection,
            &format!(
                "GET {path} HTTP/1.1\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n{lines}\r\n"
            ),
        );
        assert!(head.starts_with("HTTP/1.1 101"), "{head}");
        (Self(connection), head)
    }

    fn modern(address: &str) -> Self {
        let lines = "Sec-WebSocket-Protocol: graphql-transport-ws\r\n";
        Self::open(address, "/v1/graphql", lines).0
    }

    fn legacy(address: &str) -> Self {
        Self::open(address, "/v1/graphql", "").0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.get_mut().write_all(bytes).unwrap();
    }

    fn send(&mut self, text: &str) {
        self.write(&frame(0x81, text.as_bytes(), true));
    }

    fn init(&mut self) {
        self.send(r#"{"type":"connection_init"}"#);
        assert_eq!(self.kind(), "connection_ack");
    }

    fn subscribe(&mut self, kind: &str, id: &str, query: &str) {
        let start = Json::object(vec![
            ("id", Json::str(id)),
            ("type", Json::str(kind)),
            ("payload", Json::object(vec![("query", Json::str(query))])),
        ]);
        self.send(&start.write());
    }

    /// One frame: its opcode and payload.
    fn raw(&mut self) -> (u8, Vec<u8>) {
        let mut head = [0u8; 2];
        self.0.read_exact(&mut head).unwrap();
        let length = match head[1] & 0x7f {
            126 => {
                let mut size = [0u8; 2];
                self.0.read_exact(&mut size).unwrap();
                usize::from(u16::from_be_bytes(size))
            }
            127 => {
                let mut size = [0u8; 8];
                self.0.read_exact(&mut size).unwrap();
                u64::from_be_bytes(size) as usize
            }
            short => usize::from(short),
        };
        let mut payload = vec![0; length];
        self.0.read_exact(&mut payload).unwrap();
        (head[0] & 0x0f, payload)
    }

    fn message(&mut self) -> Json {
        let (opcode, payload) = self.raw();
        assert_eq!(opcode, 1);
        Json::parse(std::str::from_utf8(&payload).unwrap()).unwrap()
    }

    fn kind(&mut self) -> String {
        let message = self.message();
        let kind = message.get("type").and_then(Json::as_str);
        kind.unwrap().to_string()
    }

    /// The server hung up: nothing more can be read.
    fn assert_closed(&mut self) {
        let mut byte = [0u8; 1];
        assert!(!matches!(self.0.read(&mut byte), Ok(1)), "still talking");
    }
}

#[test]
fn routes_share_one_connection() {
    let address = start(None);
    let mut connection = connect(&address);
    for path in ["/healthz", "/api/healthz", "/v1/healthz", "/healthz?full=1"] {
        let (head, body) = exchange(&mut connection, &format!("GET {path} HTTP/1.1\r\n\r\n"));
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(body, r#"{"status":"ok"}"#);
    }
    for path in ["/v1/version", "/v1/version/"] {
        let request = format!("GET {path} HTTP/1.1\r\nno colon on this line\r\n\r\n");
        assert_eq!(
            exchange(&mut connection, &request).1,
            r#"{"version":"clink"}"#
        );
    }
    for request in [
        "GET /missing HTTP/1.1\r\n\r\n",
        "POST /missing HTTP/1.1\r\n\r\n",
        "GET /v1/graphql HTTP/1.1\r\n\r\n",
        // Only GraphQL paths upgrade.
        "GET /missing HTTP/1.1\r\nUpgrade: websocket\r\n\r\n",
        // A blank request line is a request for nothing.
        "\r\n\r\n",
    ] {
        let (head, _) = exchange(&mut connection, request);
        assert!(head.starts_with("HTTP/1.1 404"), "{request}: {head}");
    }
}

#[test]
fn preflight_echoes_what_it_is_asked_about() {
    let address = start(None);
    let mut connection = connect(&address);
    let (head, _) = exchange(
        &mut connection,
        "OPTIONS /v1/graphql HTTP/1.1\r\nAccess-Control-Request-Headers: x-mine\r\n\r\n",
    );
    assert!(head.starts_with("HTTP/1.1 204"), "{head}");
    assert!(head.contains("Allow-Headers: x-mine"), "{head}");
    let (head, _) = exchange(&mut connection, "OPTIONS /other HTTP/1.1\r\n\r\n");
    assert!(head.contains("x-hasura-admin-secret"), "{head}");
}

#[test]
fn requests_choose_operations_and_variables() {
    let address = start(None);
    let body = r#"{"operationName":"Second","variables":{"id":1},"query":"query First { links(limit: 0) { id } } query Second($id: bigint) { links(where: {id: {_eq: $id}}) { id } }"}"#;
    assert_eq!(
        post_text(&address, body, ""),
        r#"{"data":{"links":[{"id":1}]}}"#
    );
    // Fields of the wrong kind count as absent, and a missing query is refused.
    let wrong = post_text(&address, r#"{"query":1,"operationName":2}"#, "");
    assert!(wrong.contains("errors"), "{wrong}");
}

#[test]
fn bad_requests_get_an_error_and_keep_the_server() {
    let address = start(Some("root"));
    assert!(post_text(&address, "{not json", "").contains("errors"));

    let forged = post(
        &address,
        "query { guest { token } }",
        "Authorization: Bearer a.b.c\r\n",
    );
    assert!(forged.write().contains("invalid-jwt"), "{}", forged.write());
    let wrong = "x-hasura-admin-secret: wrong\r\n";
    assert!(post(&address, "query { guest { token } }", wrong)
        .get("errors")
        .is_some());
    let right = "x-hasura-admin-secret: root\r\n";
    assert!(post(&address, "query { guest { token } }", right)
        .get("data")
        .is_some());

    // A length that is not a number means no body.
    let request = "POST /v1/graphql HTTP/1.1\r\nContent-Length: many\r\n\r\n";
    assert!(exchange(&mut connect(&address), request)
        .1
        .contains("errors"));

    let request = format!(
        "POST /v1/graphql HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        u64::MAX / 2
    );
    let (head, _) = exchange(&mut connect(&address), &request);
    assert!(head.starts_with("HTTP/1.1 413"), "{head}");

    // Clients that vanish at each step of a request.
    for partial in [
        "",
        "GET /healthz HTTP/1.1\r\n",
        "POST /v1/graphql HTTP/1.1\r\nContent-Length: 5\r\n\r\nab",
    ] {
        let mut connection = connect(&address);
        connection.get_mut().write_all(partial.as_bytes()).unwrap();
        connection.get_mut().shutdown(Shutdown::Write).unwrap();
        let mut rest = Vec::new();
        connection.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "{partial}");
    }
    assert!(post(&address, INSERT, "").get("data").is_some());
}

#[test]
fn every_graphql_path_upgrades_and_the_protocol_is_negotiated() {
    let address = start(None);
    for path in ["/v1/graphql", "/gql", "/v1alpha1/graphql"] {
        let (_, head) = Socket::open(&address, path, "");
        assert!(
            head.contains("Sec-WebSocket-Protocol: graphql-ws\r\n"),
            "{head}"
        );
        assert!(head.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="), "{head}");
    }
    let lines = "Sec-WebSocket-Protocol: other, graphql-transport-ws\r\n";
    let (_, head) = Socket::open(&address, "/gql", lines);
    assert!(
        head.contains("Protocol: graphql-transport-ws\r\n"),
        "{head}"
    );
    // A missing key still gets an answer.
    let mut connection = connect(&address);
    let request = "GET /gql HTTP/1.1\r\nUpgrade: websocket\r\n\r\n";
    assert!(exchange(&mut connection, request)
        .0
        .starts_with("HTTP/1.1 101"));
}

#[test]
fn frames_join_and_control_frames_interleave() {
    let address = start(None);
    let mut socket = Socket::modern(&address);
    let ping = r#"{"type":"ping"}"#.as_bytes();

    // Split over a fragment, with a ping and a pong between the parts.
    let (first, second) = ping.split_at(5);
    socket.write(&frame(0x01, first, true));
    socket.write(&frame(0x89, b"hello", true));
    assert_eq!(socket.raw(), (10, b"hello".to_vec()));
    socket.write(&frame(0x8a, b"late", true));
    socket.write(&frame(0x80, second, true));
    assert_eq!(socket.kind(), "pong");

    // Without a mask, and with each length encoding.
    socket.write(&frame(0x81, ping, false));
    assert_eq!(socket.kind(), "pong");
    let spaced = |size| format!(r#"{{"type":"ping"{}}}"#, " ".repeat(size));
    for size in [200, 70_000] {
        socket.send(&spaced(size));
        assert_eq!(socket.kind(), "pong", "{size}");
    }

    // What cannot be understood is passed over.
    for text in ["not json", "{}", r#"{"type":"unknown"}"#, r#"{"type":1}"#] {
        socket.send(text);
    }
    socket.send(r#"{"type":"ping"}"#);
    assert_eq!(socket.kind(), "pong");

    // A close is echoed, and ends the connection.
    socket.write(&frame(0x88, b"bye", true));
    assert_eq!(socket.raw(), (8, b"bye".to_vec()));
    socket.assert_closed();
}

#[test]
fn a_frame_over_the_limit_or_cut_short_ends_the_connection() {
    let address = start(None);
    let mut socket = Socket::modern(&address);
    let mut oversized = vec![0x81, 0x7f];
    oversized.extend_from_slice(&u64::MAX.to_be_bytes());
    socket.write(&oversized);
    socket.assert_closed();

    let mut socket = Socket::modern(&address);
    socket.write(&[0x81]);
    socket.0.get_mut().shutdown(Shutdown::Write).unwrap();
    socket.assert_closed();
}

#[test]
fn connection_init_carries_credentials() {
    let address = start(Some("root"));
    let token = guest_token(&address);
    for payload in [
        format!(r#"{{"Authorization":"Bearer {token}"}}"#),
        format!(r#"{{"authorization":"Bearer {token}"}}"#),
        format!(r#"{{"headers":{{"Authorization":"Bearer {token}"}}}}"#),
        r#"{"headers":{"x-hasura-admin-secret":"root"}}"#.to_string(),
        "{}".to_string(),
    ] {
        let mut socket = Socket::modern(&address);
        socket.send(&format!(
            r#"{{"type":"connection_init","payload":{payload}}}"#
        ));
        assert_eq!(socket.kind(), "connection_ack", "{payload}");
    }

    // The legacy protocol also starts keeping the connection alive.
    let mut socket = Socket::legacy(&address);
    socket.init();
    assert_eq!(socket.kind(), "ka");

    let mut socket = Socket::modern(&address);
    socket.send(r#"{"type":"connection_init","payload":{"Authorization":"Bearer a.b.c"}}"#);
    let refused = socket.message();
    assert_eq!(
        refused.get("type").and_then(Json::as_str),
        Some("connection_error")
    );
    assert!(refused
        .get("payload")
        .and_then(|p| p.get("message"))
        .is_some());
    socket.assert_closed();

    // A bad token on the upgrade is only a guest until something better arrives.
    let lines = "Authorization: Bearer a.b.c\r\nSec-WebSocket-Protocol: graphql-transport-ws\r\n";
    let (mut socket, _) = Socket::open(&address, "/gql", lines);
    socket.init();
    let mut socket_terminated = Socket::modern(&address);
    socket_terminated.send(r#"{"type":"connection_terminate"}"#);
    socket_terminated.assert_closed();
}

#[test]
fn subscriptions_report_errors_in_the_form_of_their_protocol() {
    let address = start(None);

    let mut modern = Socket::modern(&address);
    modern.init();
    modern.subscribe("subscribe", "1", "subscription { nothing_here { id } }");
    let refused = modern.message();
    assert_eq!(refused.get("type").and_then(Json::as_str), Some("error"));
    assert_eq!(refused.get("id").and_then(Json::as_str), Some("1"));
    assert!(!refused.get("payload").unwrap().as_list().is_empty());

    let mut legacy = Socket::legacy(&address);
    legacy.init();
    assert_eq!(legacy.kind(), "ka");
    legacy.subscribe("start", "2", "subscription { nothing_here { id } }");
    let refused = legacy.message();
    assert_eq!(refused.get("type").and_then(Json::as_str), Some("data"));
    assert!(refused.get("payload").unwrap().get("errors").is_some());

    // An id is optional; the reply then names an empty one.
    modern.send(&format!(
        r#"{{"type":"subscribe","payload":{}}}"#,
        Json::object(vec![("query", Json::str(WATCH))]).write()
    ));
    let first = modern.message();
    assert_eq!(first.get("id").and_then(Json::as_str), Some(""));
}

#[test]
fn stopping_ends_only_the_named_subscription() {
    let address = start(None);
    let mut socket = Socket::modern(&address);
    socket.init();
    socket.subscribe("subscribe", "kept", WATCH);
    assert_eq!(socket.kind(), "next");
    socket.subscribe("subscribe", "ended", WATCH);
    assert_eq!(socket.kind(), "next");
    socket.send(r#"{"type":"complete","id":"ended"}"#);
    // Answered after the stop, so the stop has been queued.
    socket.send(r#"{"type":"ping"}"#);
    assert_eq!(socket.kind(), "pong");

    post(&address, INSERT, "");
    let update = socket.message();
    assert_eq!(update.get("id").and_then(Json::as_str), Some("kept"));
    socket.send(r#"{"type":"ping"}"#);
    assert_eq!(socket.kind(), "pong", "the ended one stays silent");

    // Without an id every subscription of the connection ends.
    let mut watcher = Socket::modern(&address);
    watcher.init();
    watcher.subscribe("subscribe", "a", WATCH);
    assert_eq!(watcher.kind(), "next");
    watcher.send(r#"{"type":"complete"}"#);
    watcher.send(r#"{"type":"ping"}"#);
    assert_eq!(watcher.kind(), "pong");
    post(&address, INSERT, "");
    assert_eq!(
        socket.message().get("id").and_then(Json::as_str),
        Some("kept")
    );
    watcher.send(r#"{"type":"ping"}"#);
    assert_eq!(watcher.kind(), "pong");

    // The legacy protocol confirms a stop.
    let mut legacy = Socket::legacy(&address);
    legacy.init();
    assert_eq!(legacy.kind(), "ka");
    legacy.subscribe("start", "old", WATCH);
    assert_eq!(legacy.kind(), "data");
    legacy.send(r#"{"type":"stop","id":"old"}"#);
    let done = legacy.message();
    assert_eq!(done.get("type").and_then(Json::as_str), Some("complete"));
    assert_eq!(done.get("id").and_then(Json::as_str), Some("old"));
}

#[test]
fn a_write_reaches_only_subscriptions_whose_answer_changed() {
    let address = start(None);
    let mut watcher = Socket::modern(&address);
    watcher.init();
    watcher.subscribe(
        "subscribe",
        "same",
        "subscription { links(where: {id: {_eq: 1}}) { id } }",
    );
    assert_eq!(watcher.kind(), "next");
    watcher.subscribe("subscribe", "moves", WATCH);
    assert_eq!(watcher.kind(), "next");

    // A mutation run through a subscription start counts as a write too.
    let mut writer = Socket::modern(&address);
    writer.init();
    writer.subscribe("subscribe", "write", INSERT);
    assert_eq!(writer.kind(), "next");

    let update = watcher.message();
    assert_eq!(update.get("id").and_then(Json::as_str), Some("moves"));
    watcher.send(r#"{"type":"ping"}"#);
    assert_eq!(watcher.kind(), "pong");
}

#[test]
fn long_answers_use_the_longer_frame_lengths() {
    let address = start(None);
    let mut socket = Socket::modern(&address);
    socket.init();
    for (size, id) in [(200, "medium"), (70_000, "long")] {
        let text = "x".repeat(size);
        let insert = format!(
            r#"mutation {{ insert_links(objects: {{type_id: 1, string: {{data: {{value: "{text}"}}}}}}) {{ returning {{ id }} }} }}"#
        );
        let answer = post(&address, &insert, "");
        let row = answer.get("data").and_then(|d| d.get("insert_links"));
        let row = row.and_then(|i| i.get("returning")).unwrap().as_list()[0].clone();
        let query = format!(
            "subscription {{ links(where: {{id: {{_eq: {}}}}}) {{ value }} }}",
            row.get("id").unwrap().as_i64().unwrap()
        );
        socket.subscribe("subscribe", id, &query);
        let reply = socket.message();
        assert!(reply.write().contains(&text), "{id}");
    }
}
