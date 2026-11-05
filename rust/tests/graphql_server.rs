//! The GraphQL server as a client meets it: over sockets, with tokens, across
//! a restart. `csharp/Foundation.Data.Doublets.Cli.Tests` has the same cases.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use link_cli::graphql::Json;

fn docs(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/protocol/graphql")
        .join(name)
}

fn database(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("clink-graphql-{name}-{}.links", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// The markers of the permission layer and `rule`, written without a policy.
fn write_rules(db: &PathBuf, rule: &str) {
    let query = format!(
        "() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (Doc: Type Doc) {rule})"
    );
    let written = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(db)
        .arg("--auto-create-missing-references")
        .arg(query)
        .output()
        .unwrap();
    assert!(written.status.success(), "{written:?}");
}

struct Server {
    child: Child,
    address: String,
}

impl Server {
    fn start(db: &PathBuf, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_clink"))
            .arg("--db")
            .arg(db)
            .args(["--serve", "127.0.0.1:0", "--graphql", "--jwt-secret", "s3"])
            .arg("--deep-package")
            .arg(docs("deep-core.json"))
            .args(extra)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let address = line
            .trim()
            .rsplit(' ')
            .next()
            .expect("the address")
            .to_string();
        Self { child, address }
    }

    fn post_raw(&self, query: &str, headers: &str) -> Json {
        let body = Json::object(vec![("query", Json::str(query))]).write();
        let mut stream = TcpStream::connect(&self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        write!(
            stream,
            "POST /v1/graphql HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut answer = vec![0; length];
        reader.read_exact(&mut answer).unwrap();
        Json::parse(std::str::from_utf8(&answer).unwrap()).unwrap()
    }

    fn post(&self, query: &str) -> Json {
        self.post_raw(query, "")
    }

    fn guest(&self) -> (String, i64) {
        let answer = self.post("query { guest { token linkId } }");
        let guest = answer.get("data").unwrap().get("guest").unwrap();
        (
            guest.get("token").unwrap().as_str().unwrap().to_string(),
            guest.get("linkId").unwrap().as_i64().unwrap(),
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ids(answer: &Json, table: &str) -> Vec<i64> {
    answer
        .get("data")
        .and_then(|data| data.get(table))
        .unwrap_or_else(|| panic!("no {table} in {}", answer.write()))
        .as_list()
        .iter()
        .map(|row| row.get("id").unwrap().as_i64().unwrap())
        .collect()
}

fn bearer(token: &str) -> String {
    format!("Authorization: Bearer {token}\r\n")
}

const INSERT: &str = "mutation { insert_links(objects: {type_id: 1}) { returning { id } } }";

fn inserted(answer: &Json) -> i64 {
    answer
        .get("data")
        .and_then(|data| data.get("insert_links"))
        .and_then(|insert| insert.get("returning"))
        .unwrap_or_else(|| panic!("nothing inserted: {}", answer.write()))
        .as_list()[0]
        .get("id")
        .unwrap()
        .as_i64()
        .unwrap()
}

/// A WebSocket client: frames it sends are masked, as a server demands.
struct Socket(BufReader<TcpStream>);

impl Socket {
    fn open(server: &Server, protocol: &str) -> Self {
        let mut stream = TcpStream::connect(&server.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        write!(
            stream,
            "GET /v1/graphql HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {protocol}\r\n\r\n"
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            head.push_str(&line.to_ascii_lowercase());
        }
        assert!(head.starts_with("http/1.1 101"), "{head}");
        assert!(head.contains("s3pplmbitxaq9kygzzhzrbk+xoo="), "{head}");
        assert!(
            head.contains(&format!("sec-websocket-protocol: {protocol}")),
            "{head}"
        );
        Self(reader)
    }

    fn send(&mut self, text: &str) {
        let mask = [1u8, 2, 3, 4];
        let mut frame = vec![0x81];
        if text.len() < 126 {
            frame.push(0x80 | text.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(text.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(text.bytes().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.get_mut().write_all(&frame).unwrap();
    }

    fn receive(&mut self) -> Json {
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
        Json::parse(std::str::from_utf8(&payload).unwrap()).unwrap()
    }

    /// The next message of a type, past keep-alives.
    fn wait_for(&mut self, kind: &str) -> Json {
        loop {
            let message = self.receive();
            if message.get("type").and_then(Json::as_str) == Some(kind) {
                return message;
            }
            assert_ne!(
                message.get("type").and_then(Json::as_str),
                Some("error"),
                "{}",
                message.write()
            );
        }
    }
}

const WATCH: &str =
    "subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }";

fn subscribes(protocol: &str, init: &str, start: &str, data: &str) {
    let db = database(&format!("ws-{protocol}"));
    let server = Server::start(&db, &[]);
    let mut socket = Socket::open(&server, protocol);
    socket.send(init);
    socket.wait_for("connection_ack");
    socket.send(start);
    let first = socket.wait_for(data);
    let id = inserted(&server.post(INSERT));
    let update = socket.wait_for(data);
    assert_ne!(first.write(), update.write());
    let payload = update.get("payload").unwrap();
    assert_eq!(ids(payload, "links"), vec![id], "{}", update.write());
    let _ = std::fs::remove_file(db);
}

#[test]
fn legacy_subscription_sees_a_write() {
    let start = Json::object(vec![
        ("id", Json::str("1")),
        ("type", Json::str("start")),
        ("payload", Json::object(vec![("query", Json::str(WATCH))])),
    ]);
    subscribes(
        "graphql-ws",
        r#"{"type":"connection_init","payload":{}}"#,
        &start.write(),
        "data",
    );
}

#[test]
fn graphql_ws_subscription_sees_a_write() {
    let start = Json::object(vec![
        ("id", Json::str("1")),
        ("type", Json::str("subscribe")),
        ("payload", Json::object(vec![("query", Json::str(WATCH))])),
    ]);
    subscribes(
        "graphql-transport-ws",
        r#"{"type":"connection_init","payload":{}}"#,
        &start.write(),
        "next",
    );
}

#[test]
fn the_token_is_the_subject_under_permissions() {
    let db = database("subject");
    // Whoever it is may create: creating needs some right to write.
    write_rules(&db, "(AllowWrite (Anything Doc))");
    let server = Server::start(
        &db,
        &["--permissions", "self", "--graphql-admin-secret", "root"],
    );
    let all = "query { links(where: {id: {_lte: 2}}) { id } }";
    assert_eq!(ids(&server.post(all), "links"), Vec::<i64>::new());
    let admin = server.post_raw(all, "x-hasura-admin-secret: root\r\n");
    assert_eq!(ids(&admin, "links").len(), 2, "{}", admin.write());
    let wrong = server.post_raw(all, "x-hasura-admin-secret: wrong\r\n");
    assert!(wrong.get("errors").is_some(), "{}", wrong.write());

    // What a subject created is its own: visible to it, not to another.
    let (token, _) = server.guest();
    let (other, _) = server.guest();
    // Its own type, as a type it could not write would be refused.
    let own = inserted(&server.post_raw(
        "mutation { insert_links(objects: {id: 9000, type_id: 9000}) { returning { id } } }",
        &bearer(&token),
    ));
    let by_id = format!("query {{ links(where: {{id: {{_eq: {own}}}}}) {{ id }} }}");
    assert_eq!(
        ids(&server.post_raw(&by_id, &bearer(&token)), "links"),
        vec![own]
    );
    assert!(ids(&server.post_raw(&by_id, &bearer(&other)), "links").is_empty());
    assert!(ids(&server.post(&by_id), "links").is_empty());

    // The same over a WebSocket, whose token comes in `connection_init`.
    let mut socket = Socket::open(&server, "graphql-ws");
    let init = format!(
        r#"{{"type":"connection_init","payload":{{"headers":{{"Authorization":"Bearer {token}"}}}}}}"#
    );
    socket.send(&init);
    socket.wait_for("connection_ack");
    let start = Json::object(vec![
        ("id", Json::str("1")),
        ("type", Json::str("start")),
        ("payload", Json::object(vec![("query", Json::str(&by_id))])),
    ]);
    socket.send(&start.write());
    let data = socket.wait_for("data");
    assert_eq!(ids(data.get("payload").unwrap(), "links"), vec![own]);

    // A forged token is refused.
    let forged = server.post_raw(all, &bearer("a.b.c"));
    assert!(forged.get("errors").is_some(), "{}", forged.write());
    let _ = std::fs::remove_file(db);
}

#[test]
fn can_shows_the_rules_of_the_policy() {
    let db = database("can");
    write_rules(&db, "(AllowRead (Anything Anything))");
    let server = Server::start(&db, &["--permissions", "self"]);
    let can = server.post("query { can { subject_id object_id } }");
    assert_eq!(
        can.write(),
        r#"{"data":{"can":[{"subject_id":0,"object_id":0}]}}"#
    );
    let read = server.post("query { links(where: {id: {_eq: 1}}) { id } }");
    assert_eq!(ids(&read, "links"), vec![1]);
    drop(server);
    let _ = std::fs::remove_file(db);
}

#[test]
fn links_survive_a_restart() {
    let db = database("restart");
    let id = {
        let server = Server::start(&db, &[]);
        let answer = server.post(
            r#"mutation { insert_links(objects: {type_id: 1, string: {data: {value: "kept"}}}) { returning { id } } }"#,
        );
        inserted(&answer)
    };
    let server = Server::start(&db, &[]);
    let query = format!("query {{ links(where: {{id: {{_eq: {id}}}}}) {{ id value }} }}");
    let answer = server.post(&query);
    assert_eq!(ids(&answer, "links"), vec![id], "{}", answer.write());
    assert!(answer.write().contains("kept"), "{}", answer.write());
    let _ = std::fs::remove_file(db);
}

#[test]
fn the_embedded_schema_is_the_recorded_one() {
    let recorded = std::fs::read(docs("introspection.json")).unwrap();
    let embedded = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/graphql/introspection.json"),
    )
    .unwrap();
    assert!(
        recorded == embedded,
        "src/graphql/introspection.json differs from docs"
    );
}
