//! HTTP and WebSocket transport of the GraphQL layer.
//!
//! Connections have threads; requests run on the serving thread through a
//! channel, as in [`crate::protocol`], so the store needs no locks. After each
//! write every subscription runs again, and is sent if its answer changed.
//! WebSocket speaks the subprotocols `graphql-ws` (`subscriptions-transport-ws`,
//! which `DeepClient` uses) and `graphql-transport-ws` (the `graphql-ws`
//! library).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::crypto::websocket_accept;
use super::execute::{execute, Auth, GqlError, Settings};
use super::json::Json;
use super::store::DeepStore;

const MAX_BODY: usize = 64 * 1024 * 1024;

type Shared = Arc<Mutex<TcpStream>>;

enum Job {
    Http {
        body: Json,
        auth: Auth,
        reply: Sender<Json>,
    },
    Subscribe {
        connection: u64,
        id: String,
        payload: Json,
        auth: Auth,
        legacy: bool,
        socket: Shared,
    },
    Stop {
        connection: u64,
        id: Option<String>,
    },
}

struct Subscription {
    connection: u64,
    id: String,
    query: String,
    operation: Option<String>,
    variables: Json,
    auth: Auth,
    legacy: bool,
    socket: Shared,
    last: String,
}

pub struct GraphqlServer {
    listener: TcpListener,
}

fn request_parts(body: &Json) -> (String, Option<String>, Json) {
    (
        body.get("query")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        body.get("operationName")
            .and_then(Json::as_str)
            .map(str::to_string),
        body.get("variables").cloned().unwrap_or(Json::Null),
    )
}

impl GraphqlServer {
    pub fn bind(address: &str) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(address)?,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// Serves until the process stops.
    pub fn serve(self, store: &mut DeepStore, settings: Settings) -> Result<()> {
        let (jobs, inbox) = channel();
        let settings = Arc::new(settings);
        let thread_settings = Arc::clone(&settings);
        let listener = self.listener;
        std::thread::spawn(move || {
            let mut next = 0;
            for stream in listener.incoming().flatten() {
                next += 1;
                let (jobs, settings, connection) =
                    (jobs.clone(), Arc::clone(&thread_settings), next);
                std::thread::spawn(move || {
                    let _ = handle(stream, jobs, &settings, connection);
                });
            }
        });
        run_jobs(store, &settings, inbox);
        Ok(())
    }
}

fn run_jobs(store: &mut DeepStore, settings: &Settings, inbox: Receiver<Job>) {
    let mut subscriptions: Vec<Subscription> = Vec::new();
    for job in inbox {
        match job {
            Job::Http { body, auth, reply } => {
                let (query, operation, variables) = request_parts(&body);
                let before = store.generation;
                let (response, _) = execute(
                    store,
                    settings,
                    &query,
                    operation.as_deref(),
                    &variables,
                    auth,
                );
                let _ = reply.send(response);
                if store.generation != before {
                    refresh(store, settings, &mut subscriptions);
                }
            }
            Job::Subscribe {
                connection,
                id,
                payload,
                auth,
                legacy,
                socket,
            } => {
                let (query, operation, variables) = request_parts(&payload);
                let before = store.generation;
                let (response, _) = execute(
                    store,
                    settings,
                    &query,
                    operation.as_deref(),
                    &variables,
                    auth,
                );
                if response.get("errors").is_some() {
                    let _ = send_error(&socket, &id, &response, legacy);
                } else {
                    let last = response.write();
                    let _ = send_data(&socket, &id, &last, legacy);
                    subscriptions.push(Subscription {
                        connection,
                        id,
                        query,
                        operation,
                        variables,
                        auth,
                        legacy,
                        socket,
                        last,
                    });
                }
                if store.generation != before {
                    refresh(store, settings, &mut subscriptions);
                }
            }
            Job::Stop { connection, id } => subscriptions.retain(|subscription| {
                subscription.connection != connection
                    || id.as_ref().is_some_and(|id| *id != subscription.id)
            }),
        }
    }
}

fn refresh(store: &mut DeepStore, settings: &Settings, subscriptions: &mut Vec<Subscription>) {
    subscriptions.retain_mut(|subscription| {
        let (response, _) = execute(
            store,
            settings,
            &subscription.query,
            subscription.operation.as_deref(),
            &subscription.variables,
            subscription.auth,
        );
        let text = response.write();
        if text == subscription.last {
            return true;
        }
        subscription.last = text;
        send_data(
            &subscription.socket,
            &subscription.id,
            &subscription.last,
            subscription.legacy,
        )
        .is_ok()
    });
}

fn message(kind: &str, id: Option<&str>, payload: Option<&str>) -> String {
    let id = id.map_or_else(String::new, |id| {
        format!(",\"id\":{}", Json::str(id).write())
    });
    let payload = payload.map_or_else(String::new, |payload| format!(",\"payload\":{payload}"));
    format!("{{\"type\":\"{kind}\"{id}{payload}}}")
}

fn send_data(socket: &Shared, id: &str, payload: &str, legacy: bool) -> std::io::Result<()> {
    let kind = if legacy { "data" } else { "next" };
    send_frame(socket, 1, message(kind, Some(id), Some(payload)).as_bytes())
}

fn send_error(socket: &Shared, id: &str, response: &Json, legacy: bool) -> std::io::Result<()> {
    if legacy {
        return send_data(socket, id, &response.write(), true);
    }
    let errors = response
        .get("errors")
        .map_or_else(|| "[]".to_string(), Json::write);
    send_frame(
        socket,
        1,
        message("error", Some(id), Some(&errors)).as_bytes(),
    )
}

fn send_frame(socket: &Shared, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut frame = vec![0x80 | opcode];
    match payload.len() {
        length @ 0..=125 => frame.push(length as u8),
        length @ 126..=65535 => {
            frame.push(126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(payload);
    let mut stream = socket
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))?;
    stream.write_all(&frame)
}

/// Reads one WebSocket message, joining fragments and answering pings.
fn read_message(reader: &mut impl Read, socket: &Shared) -> std::io::Result<Option<String>> {
    let mut text = Vec::new();
    loop {
        let mut header = [0; 2];
        reader.read_exact(&mut header)?;
        let opcode = header[0] & 0x0f;
        let mut length = u64::from(header[1] & 0x7f);
        if length == 126 {
            let mut bytes = [0; 2];
            reader.read_exact(&mut bytes)?;
            length = u64::from(u16::from_be_bytes(bytes));
        } else if length == 127 {
            let mut bytes = [0; 8];
            reader.read_exact(&mut bytes)?;
            length = u64::from_be_bytes(bytes);
        }
        if length as usize > MAX_BODY {
            return Ok(None);
        }
        let mut mask = [0; 4];
        if header[1] & 0x80 != 0 {
            reader.read_exact(&mut mask)?;
        }
        let mut payload = vec![0; length as usize];
        reader.read_exact(&mut payload)?;
        for (at, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[at % 4];
        }
        match opcode {
            8 => {
                let _ = send_frame(socket, 8, &payload);
                return Ok(None);
            }
            9 => send_frame(socket, 10, &payload)?,
            10 => {}
            _ => {
                text.extend_from_slice(&payload);
                if header[0] & 0x80 != 0 {
                    return Ok(Some(String::from_utf8_lossy(&text).into_owned()));
                }
            }
        }
    }
}

fn header<'h>(headers: &'h HashMap<String, String>, name: &str) -> Option<&'h str> {
    headers.get(name).map(String::as_str)
}

const CORS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: authorization, content-type, x-hasura-admin-secret, x-hasura-role, x-hasura-user-id\r\n";

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\n{CORS}Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn handle(
    stream: TcpStream,
    jobs: Sender<Job>,
    settings: &Settings,
    connection: u64,
) -> Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut words = line.split_whitespace();
        let (method, path) = (
            words.next().unwrap_or("").to_string(),
            words.next().unwrap_or("").to_string(),
        );
        let path = path.split('?').next().unwrap_or("").to_string();
        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let graphql = path == "/v1/graphql" || path == "/gql" || path == "/v1alpha1/graphql";
        if header(&headers, "upgrade")
            .is_some_and(|upgrade| upgrade.eq_ignore_ascii_case("websocket"))
            && graphql
        {
            return websocket(reader, writer, &headers, jobs, settings, connection);
        }
        let length: usize = header(&headers, "content-length")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);
        if length > MAX_BODY {
            respond(&mut writer, "413 Payload Too Large", "{}")?;
            return Ok(());
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        match (method.as_str(), path.as_str()) {
            ("OPTIONS", _) => write!(
                writer,
                "HTTP/1.1 204 No Content\r\n{CORS}Content-Length: 0\r\n\r\n"
            )?,
            ("GET", "/healthz" | "/api/healthz" | "/v1/healthz") => {
                respond(&mut writer, "200 OK", "{\"status\":\"ok\"}")?
            }
            ("GET", "/v1/version" | "/v1/version/") => {
                respond(&mut writer, "200 OK", "{\"version\":\"clink\"}")?
            }
            ("POST", _) if graphql => {
                let auth = settings.authorize(
                    header(&headers, "authorization"),
                    header(&headers, "x-hasura-admin-secret"),
                );
                let response = match (auth, Json::parse(&String::from_utf8_lossy(&body))) {
                    (Ok(auth), Ok(body)) => {
                        let (reply, answer) = channel();
                        jobs.send(Job::Http { body, auth, reply })?;
                        answer.recv()?
                    }
                    (Err(message), _) => GqlError {
                        message,
                        code: "invalid-jwt",
                        path: "$".to_string(),
                    }
                    .to_json(),
                    (_, Err(error)) => GqlError::validation(error.to_string(), "$").to_json(),
                };
                respond(&mut writer, "200 OK", &response.write())?;
            }
            _ => respond(
                &mut writer,
                "404 Not Found",
                "{\"error\":\"resource does not exist\"}",
            )?,
        }
    }
}

fn websocket(
    mut reader: BufReader<TcpStream>,
    mut writer: TcpStream,
    headers: &HashMap<String, String>,
    jobs: Sender<Job>,
    settings: &Settings,
    connection: u64,
) -> Result<()> {
    let key = header(headers, "sec-websocket-key").unwrap_or("");
    let offered = header(headers, "sec-websocket-protocol").unwrap_or("");
    let modern = offered
        .split(',')
        .any(|name| name.trim() == "graphql-transport-ws");
    let chosen = if modern {
        "graphql-transport-ws"
    } else {
        "graphql-ws"
    };
    let legacy = !modern;
    write!(
        writer,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\nSec-WebSocket-Protocol: {chosen}\r\n\r\n",
        websocket_accept(key)
    )?;
    let socket: Shared = Arc::new(Mutex::new(writer));
    let first = (
        header(headers, "authorization").map(str::to_string),
        header(headers, "x-hasura-admin-secret").map(str::to_string),
    );
    let mut auth = settings
        .authorize(first.0.as_deref(), first.1.as_deref())
        .unwrap_or_default();
    let result = (|| -> Result<()> {
        while let Some(text) = read_message(&mut reader, &socket)? {
            let Ok(incoming) = Json::parse(&text) else {
                continue;
            };
            let id = incoming
                .get("id")
                .and_then(Json::as_str)
                .map(str::to_string);
            let payload = incoming.get("payload").cloned().unwrap_or(Json::Null);
            match incoming.get("type").and_then(Json::as_str).unwrap_or("") {
                "connection_init" => {
                    let holder = payload.get("headers").unwrap_or(&payload);
                    let found = |name: &str, lower: &str| {
                        holder
                            .get(name)
                            .or_else(|| holder.get(lower))
                            .and_then(Json::as_str)
                            .map(str::to_string)
                    };
                    let authorization = found("Authorization", "authorization");
                    let admin = found("x-hasura-admin-secret", "x-hasura-admin-secret");
                    match settings.authorize(authorization.as_deref(), admin.as_deref()) {
                        Ok(found) => {
                            if authorization.is_some() || admin.is_some() {
                                auth = found;
                            }
                            send_frame(
                                &socket,
                                1,
                                message("connection_ack", None, None).as_bytes(),
                            )?;
                            if legacy {
                                send_frame(&socket, 1, message("ka", None, None).as_bytes())?;
                            }
                        }
                        Err(problem) => {
                            let error = Json::object(vec![("message", Json::str(problem))]).write();
                            send_frame(
                                &socket,
                                1,
                                message("connection_error", None, Some(&error)).as_bytes(),
                            )?;
                            return Ok(());
                        }
                    }
                }
                "start" | "subscribe" => {
                    jobs.send(Job::Subscribe {
                        connection,
                        id: id.unwrap_or_default(),
                        payload,
                        auth,
                        legacy,
                        socket: Arc::clone(&socket),
                    })?;
                }
                "stop" | "complete" => {
                    let done = id.clone();
                    jobs.send(Job::Stop { connection, id })?;
                    if legacy {
                        send_frame(
                            &socket,
                            1,
                            message("complete", done.as_deref(), None).as_bytes(),
                        )?;
                    }
                }
                "ping" => send_frame(&socket, 1, message("pong", None, None).as_bytes())?,
                "connection_terminate" => return Ok(()),
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = jobs.send(Job::Stop {
        connection,
        id: None,
    });
    result
}
