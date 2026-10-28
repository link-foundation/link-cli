use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

// A bounded wire-level peer: no runtime, real sockets, finite deadlines, tiny bodies.
pub(super) fn peer(replies: Vec<Option<Value>>) -> (String, thread::JoinHandle<Vec<Value>>) {
    peer_raw(
        replies
            .into_iter()
            .map(|reply| reply.map(|value| value.to_string()))
            .collect(),
    )
}
pub(super) fn peer_raw(replies: Vec<Option<String>>) -> (String, thread::JoinHandle<Vec<Value>>) {
    peer_raw_headers(replies, vec![])
}
pub(super) fn peer_raw_headers(
    replies: Vec<Option<String>>,
    expected_headers: Vec<&'static str>,
) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (request_index, reply) in replies.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(cause)
                        if cause.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(cause) => panic!("bounded peer accept failed: {cause}"),
                }
            };
            // Winsock inherits the listener's nonblocking mode. Restore
            // blocking reads before applying the finite request deadlines.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap_or_else(|cause| {
                    panic!(
                        "peer request {request_index}: header read failed after {} bytes: {cause}",
                        bytes.len()
                    )
                });
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
            }
            let header = String::from_utf8(bytes).unwrap();
            for expected in &expected_headers {
                assert!(
                    header
                        .lines()
                        .any(|line| line.eq_ignore_ascii_case(expected)),
                    "missing fixture header {expected}"
                );
            }
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|text| text.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 16384);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            if let Some(reply) = reply {
                let text = reply;
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
            }
        }
        requests
    });
    (endpoint, handle)
}
