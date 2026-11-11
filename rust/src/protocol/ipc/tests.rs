//! In-process tests of every transport kind this platform supports.

use super::datagram::{self, memory, Session, Settings};
use super::*;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

/// Runs `test` on a thread and fails if it takes longer than `seconds`.
fn within(seconds: u64, test: impl FnOnce() + Send + 'static) {
    let (done, finished) = mpsc::channel();
    let worker = thread::spawn(move || {
        test();
        let _ = done.send(());
    });
    match finished.recv_timeout(Duration::from_secs(seconds)) {
        Ok(()) => worker.join().expect("the test thread"),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Err(panic) = worker.join() {
                std::panic::resume_unwind(panic);
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("the test did not finish in {seconds} s"),
    }
}

fn payload(len: usize, seed: u32) -> Vec<u8> {
    (0..len as u32)
        .map(|index| (index.wrapping_mul(31).wrapping_add(seed) % 251) as u8)
        .collect()
}

/// Echoes every connection of `listener` until it is closed: reads all
/// input, then writes it back and ends the outgoing direction.
fn echo_server(listener: Box<dyn Listener>) -> thread::JoinHandle<usize> {
    serve(listener, Framing::HalfClose)
}

/// How a test marks the end of a payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    /// The writer ends its outgoing direction.
    HalfClose,
    /// An 8-byte little-endian length comes first, for kinds that cannot
    /// half-close (a Windows named pipe closes both directions at once).
    LengthPrefix,
}

fn serve(mut listener: Box<dyn Listener>, framing: Framing) -> thread::JoinHandle<usize> {
    thread::spawn(move || {
        let mut served = 0;
        while let Some(mut stream) = listener.accept().expect("accept") {
            served += 1;
            thread::spawn(move || {
                if framing == Framing::LengthPrefix {
                    let mut len = [0u8; 8];
                    stream.read_exact(&mut len).expect("server read length");
                    let mut received = vec![0u8; u64::from_le_bytes(len) as usize];
                    stream.read_exact(&mut received).expect("server read");
                    stream.write_all(&received).expect("server write");
                    stream.flush().expect("server flush");
                    // Wait for the client to hang up.
                    let mut rest = [0u8; 1];
                    let _ = stream.read(&mut rest);
                    return;
                }
                let mut received = Vec::new();
                stream.read_to_end(&mut received).expect("server read");
                stream.write_all(&received).expect("server write");
                stream.flush().expect("server flush");
                stream.close_write().expect("server close_write");
                // Wait for the client to hang up.
                let mut rest = [0u8; 1];
                let _ = stream.read(&mut rest);
            });
        }
        served
    })
}

fn round_trip(endpoint: &str, len: usize, seed: u32) {
    round_trip_with(endpoint, len, seed, Framing::HalfClose);
}

fn round_trip_with(endpoint: &str, len: usize, seed: u32, framing: Framing) {
    let mut client =
        connect(endpoint).unwrap_or_else(|error| panic!("connect {endpoint}: {error}"));
    let sent = payload(len, seed);
    let writer = {
        let mut client = client.clone();
        let sent = sent.clone();
        thread::spawn(move || {
            if framing == Framing::LengthPrefix {
                client
                    .write_all(&(sent.len() as u64).to_le_bytes())
                    .expect("client write length");
            }
            client.write_all(&sent).expect("client write");
            client.flush().expect("client flush");
            if framing == Framing::HalfClose {
                client.close_write().expect("client close_write");
            }
        })
    };
    let mut echoed = Vec::new();
    match framing {
        Framing::HalfClose => {
            client.read_to_end(&mut echoed).expect("client read");
        }
        Framing::LengthPrefix => {
            echoed.resize(len, 0);
            client.read_exact(&mut echoed).expect("client read");
        }
    }
    writer.join().expect("the writer");
    assert_eq!(echoed.len(), sent.len(), "{endpoint}: echoed length");
    assert!(echoed == sent, "{endpoint}: echoed bytes differ");
    client.shutdown();
}

/// Two consecutive and two concurrent connections echo their payloads.
fn check_kind(endpoint: String, len: usize) {
    check_kind_with(endpoint, len, Framing::HalfClose);
}

fn check_kind_with(endpoint: String, len: usize, framing: Framing) {
    within(120, move || {
        let listener =
            listen(&endpoint).unwrap_or_else(|error| panic!("listen {endpoint}: {error}"));
        let address = listener.endpoint();
        let closer = listener.closer();
        let server = serve(listener, framing);
        round_trip_with(&address, len, 1, framing);
        round_trip_with(&address, 1, 2, framing);
        let concurrent: Vec<_> = (0..2)
            .map(|index| {
                let address = address.clone();
                thread::spawn(move || {
                    round_trip_with(&address, len / 2 + index, 3 + index as u32, framing)
                })
            })
            .collect();
        for client in concurrent {
            client.join().expect("a concurrent client");
        }
        closer();
        assert_eq!(server.join().expect("the server"), 4);
    });
}

fn rendezvous_endpoint(kind: &str, options: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let endpoint = format!("{kind}:{}{options}", dir.path().join("links").display());
    (dir, endpoint)
}

#[test]
fn tcp_echoes() {
    check_kind("tcp:127.0.0.1:0".into(), 300_000);
}

#[test]
fn bare_address_is_tcp() {
    let listener = listen("127.0.0.1:0").expect("listen");
    assert!(listener.local_addr().is_some());
    assert_eq!(Endpoint::parse(&listener.endpoint()).unwrap().kind(), "tcp");
}

#[test]
fn udp_echoes() {
    check_kind("udp:127.0.0.1:0".into(), 100_000);
}

#[test]
fn file_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("file", "?capacity=4096");
    check_kind(endpoint, 100_000);
}

#[test]
fn mmap_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("mmap", "?capacity=4096");
    check_kind(endpoint, 300_000);
}

#[test]
fn shm_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("shm", "?capacity=4096");
    check_kind(endpoint, 300_000);
}

#[test]
fn signal_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("signal", "?capacity=4096");
    check_kind(endpoint, 300_000);
}

#[test]
fn unix_echoes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    check_kind(
        format!("unix:{}", dir.path().join("links.sock").display()),
        300_000,
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn abstract_unix_echoes() {
    check_kind(format!("unix:@clink-test-{}", random_id()), 10_000);
}

#[cfg(unix)]
#[test]
fn fifo_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("fifo", "");
    check_kind(endpoint, 300_000);
}

#[test]
fn pipe_echoes() {
    if cfg!(unix) {
        let (_dir, endpoint) = rendezvous_endpoint("pipe", "");
        check_kind(endpoint, 100_000);
    } else {
        check_kind_with(
            format!("pipe:clink-test-{}", random_id()),
            300_000,
            Framing::LengthPrefix,
        );
    }
}

#[cfg(windows)]
#[test]
fn a_second_pipe_server_is_refused() {
    let endpoint = format!("pipe:clink-test-{}", random_id());
    let _first = listen(&endpoint).expect("listen");
    let error = listen(&endpoint).expect_err("the name is taken");
    assert_eq!(error.kind(), io::ErrorKind::AddrInUse, "{error}");
}

#[cfg(windows)]
#[test]
fn connecting_to_a_missing_pipe_is_refused() {
    let error = connect(&format!("pipe:clink-test-{}", random_id())).expect_err("nobody listens");
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused, "{error}");
}

#[cfg(target_os = "linux")]
#[test]
fn mq_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("mq", "?chunk=1024");
    check_kind(endpoint, 100_000);
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
#[test]
fn sysvmq_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("sysvmq", "?chunk=2048&poll=1");
    check_kind(endpoint, 50_000);
}

/// A chunk longer than a queue holds (Linux `msgmax` is 8192 bytes, a macOS
/// queue 2048) is lowered to fit instead of failing every send.
#[cfg(any(target_os = "linux", target_vendor = "apple"))]
#[test]
fn sysvmq_lowers_a_chunk_the_queues_cannot_hold() {
    let (_dir, endpoint) = rendezvous_endpoint("sysvmq", "?chunk=65000&poll=1");
    check_kind(endpoint, 50_000);
}

#[cfg(windows)]
#[test]
fn mailslot_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("mailslot", "");
    check_kind(endpoint, 50_000);
}

#[cfg(windows)]
#[test]
fn copydata_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("copydata", "");
    check_kind(endpoint, 100_000);
}

#[cfg(all(windows, target_pointer_width = "64"))]
#[test]
fn alpc_echoes() {
    let (_dir, endpoint) = rendezvous_endpoint("alpc", "");
    check_kind(endpoint, 100_000);
}

#[test]
fn every_kind_is_reported() {
    for kind in KINDS {
        let either = supports(kind, true) || supports(kind, false);
        let expected = match *kind {
            "fifo" => cfg!(unix),
            "mq" => cfg!(target_os = "linux"),
            "sysvmq" => cfg!(any(target_os = "linux", target_vendor = "apple")),
            "mailslot" | "copydata" => cfg!(windows),
            "alpc" => cfg!(all(windows, target_pointer_width = "64")),
            _ => true,
        };
        assert_eq!(either, expected, "{kind}");
    }
    assert!(supports("stdio", true) && !supports("stdio", false));
    assert!(supports("exec", false) && !supports("exec", true));
}

#[test]
fn unsupported_kinds_fail_cleanly() {
    for kind in KINDS {
        let endpoint = format!("{kind}:/nonexistent/clink");
        if !supports(kind, false) && *kind != "stdio" {
            let error = connect(&endpoint).expect_err(kind);
            assert_eq!(error.kind(), io::ErrorKind::Unsupported, "{kind}: {error}");
        }
        if !supports(kind, true) && *kind != "exec" {
            let error = listen(&endpoint).expect_err(kind);
            assert_eq!(error.kind(), io::ErrorKind::Unsupported, "{kind}: {error}");
        }
    }
    assert_eq!(
        connect("stdio").unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(
        listen("exec:cat").unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn connecting_without_a_listener_fails_quickly() {
    within(30, || {
        let (_dir, endpoint) = rendezvous_endpoint("mmap", "?timeout=300");
        let error = connect(&endpoint).expect_err("nobody listens");
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::TimedOut
            ),
            "{error}"
        );
    });
}

#[test]
fn closing_a_listener_ends_accept() {
    within(30, || {
        let (_dir, endpoint) = rendezvous_endpoint("file", "");
        let mut listener = listen(&endpoint).expect("listen");
        let closer = listener.closer();
        let accepting = thread::spawn(move || listener.accept().map(|stream| stream.is_none()));
        thread::sleep(Duration::from_millis(50));
        closer();
        assert!(accepting.join().expect("accept thread").expect("accept"));
    });
}

#[test]
fn listen_all_accepts_on_every_endpoint() {
    within(60, || {
        let (_dir, mmap) = rendezvous_endpoint("mmap", "");
        let listener = listen_all(&["tcp:127.0.0.1:0".to_string(), mmap.clone()]).expect("listen");
        let tcp = listener.local_addr().expect("a TCP address").to_string();
        let closer = listener.closer();
        let server = echo_server(listener);
        round_trip(&tcp, 5_000, 7);
        round_trip(&mmap, 5_000, 8);
        closer();
        assert_eq!(server.join().expect("the server"), 2);
    });
}

#[test]
fn relays_stack_transports() {
    within(60, || {
        // client --mmap--> relay --udp--> relay --tcp--> echo server
        let tcp = listen("tcp:127.0.0.1:0").expect("listen tcp");
        let tcp_endpoint = Endpoint::parse(&tcp.endpoint()).unwrap();
        let tcp_closer = tcp.closer();
        let server = echo_server(tcp);

        let mut udp = listen("udp:127.0.0.1:0").expect("listen udp");
        let udp_endpoint = Endpoint::parse(&udp.endpoint()).unwrap();
        let udp_closer = udp.closer();
        let udp_relay = thread::spawn(move || relay(&mut *udp, &tcp_endpoint));

        let (_dir, mmap) = rendezvous_endpoint("mmap", "");
        let mut ring = listen(&mmap).expect("listen mmap");
        let ring_closer = ring.closer();
        let ring_relay = thread::spawn(move || relay(&mut *ring, &udp_endpoint));

        round_trip(&mmap, 20_000, 9);

        ring_closer();
        ring_relay.join().expect("mmap relay").expect("relay");
        udp_closer();
        udp_relay.join().expect("udp relay").expect("relay");
        tcp_closer();
        assert_eq!(server.join().expect("the server"), 1);
    });
}

#[cfg(unix)]
#[test]
fn exec_runs_a_child_over_anonymous_pipes() {
    within(30, || {
        round_trip("exec:cat", 200_000, 10);
        round_trip(r#"exec:["sh", "-c", "cat"]"#, 1_000, 11);
    });
}

#[test]
fn exec_command_lines() {
    assert_eq!(
        stdio::command_line("clink --serve stdio").unwrap(),
        ["clink", "--serve", "stdio"]
    );
    assert_eq!(
        stdio::command_line(r#"["C:\\Program Files\\clink.exe", "--serve", "stdio"]"#).unwrap(),
        ["C:\\Program Files\\clink.exe", "--serve", "stdio"]
    );
    assert!(stdio::command_line("  ").is_err());
}

#[test]
fn read_timeouts_expire() {
    within(30, || {
        let (_dir, endpoint) = rendezvous_endpoint("mmap", "");
        let mut listener = listen(&endpoint).expect("listen");
        let address = listener.endpoint();
        let accepting =
            thread::spawn(move || listener.accept().expect("accept").expect("a stream"));
        let mut client = connect(&address).expect("connect");
        let server = accepting.join().expect("accept thread");
        client.set_read_timeout(Some(Duration::from_millis(50)));
        let mut buffer = [0u8; 8];
        let error = client.read(&mut buffer).expect_err("nothing was sent");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        drop(server);
        client.set_read_timeout(Some(Duration::from_secs(10)));
        assert_eq!(client.read(&mut buffer).expect("the end"), 0);
    });
}

/// A socket shut down both ways still reads its end under a new timeout:
/// macOS refuses `SO_RCVTIMEO` on it with `EINVAL`.
#[cfg(unix)]
#[test]
fn a_socket_shut_both_ways_still_reads_its_end() {
    within(30, || {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut listener =
            listen(&format!("unix:{}", dir.path().join("links.sock").display())).expect("listen");
        let address = listener.endpoint();
        let accepting =
            thread::spawn(move || listener.accept().expect("accept").expect("a stream"));
        let mut client = connect(&address).expect("connect");
        drop(accepting.join().expect("accept thread"));
        client.close_write().expect("close the outgoing direction");
        let mut buffer = [0u8; 8];
        client.set_read_timeout(Some(Duration::from_secs(10)));
        assert_eq!(client.read(&mut buffer).expect("the end"), 0);
        client.set_read_timeout(Some(Duration::from_millis(50)));
        assert_eq!(client.read(&mut buffer).expect("the end again"), 0);
    });
}

#[test]
fn custom_kinds_can_be_registered() {
    let connect_fn: ConnectFn = Arc::new(|endpoint: &Endpoint| {
        // A custom kind that is an alias of TCP.
        connect(&format!("tcp:{}", endpoint.address()))
    });
    register("test-alias", Some(connect_fn), None).expect("register");
    assert!(supports("test-alias", false));
    assert!(!supports("test-alias", true));
    assert!(register("tcp", None, None).is_err());
    assert!(register("Bad Kind", None, None).is_err());
    within(30, || {
        let listener = listen("tcp:127.0.0.1:0").expect("listen");
        let address = listener.endpoint();
        let closer = listener.closer();
        let server = echo_server(listener);
        round_trip(&format!("test-alias:{address}"), 1_000, 12);
        closer();
        server.join().expect("the server");
    });
}

#[test]
fn endpoints_parse() {
    let endpoint = Endpoint::parse("shm:/tmp/links?capacity=1048576&timeout=500").unwrap();
    assert_eq!(endpoint.kind(), "shm");
    assert_eq!(endpoint.address(), "/tmp/links");
    assert_eq!(endpoint.option("capacity"), Some("1048576"));
    assert_eq!(endpoint.timeout(), Duration::from_millis(500));
    assert_eq!(Endpoint::parse("localhost:80").unwrap().kind(), "tcp");
    assert_eq!(Endpoint::parse("[::1]:80").unwrap().kind(), "tcp");
    assert_eq!(Endpoint::parse("stdio").unwrap().kind(), "stdio");
    assert!(Endpoint::parse("shm:").is_err());
    assert!(Endpoint::parse("shm:/tmp/x?capacity=1").is_err());
    assert!(Endpoint::parse("shm:/tmp/x?nonsense=1").is_err());
    assert!(Endpoint::parse("stdio:x").is_err());
    let text = "mq:/tmp/links?chunk=512";
    assert_eq!(Endpoint::parse(text).unwrap().to_string(), text);
}

fn session_pair(lose_every: u32, chunk: usize) -> (IpcStream, IpcStream) {
    let (a, b) = memory::pair(lose_every);
    let settings = || Settings {
        chunk,
        rto: Duration::from_millis(5),
        heartbeat: None,
        dead: None,
    };
    (
        IpcStream::new(Session::start("test", Arc::new(a), settings()).expect("session a")),
        IpcStream::new(Session::start("test", Arc::new(b), settings()).expect("session b")),
    )
}

#[test]
fn datagram_sessions_survive_loss() {
    within(60, || {
        for lose_every in [0, 3, 7] {
            let (mut a, mut b) = session_pair(lose_every, 64);
            let sent = payload(20_000, lose_every);
            let writer = {
                let sent = sent.clone();
                thread::spawn(move || {
                    a.write_all(&sent).expect("write");
                    a.close_write().expect("close_write");
                    a
                })
            };
            let mut received = Vec::new();
            b.read_to_end(&mut received).expect("read");
            assert!(received == sent, "lose every {lose_every}");
            writer.join().expect("writer").shutdown();
        }
    });
}

/// A reader may hang up as soon as it reads the end, and the acknowledgement of
/// that end can be lost (a full queue drops it, a removed System V queue
/// discards it): the writer's half-close then still succeeds, as all its data
/// was acknowledged, while a later DATA frame is a broken pipe.
#[test]
fn a_peer_hanging_up_instead_of_acknowledging_the_end_still_ends_the_stream() {
    within(30, || {
        let (a, peer) = memory::pair(0);
        let settings = Settings {
            chunk: 64,
            rto: Duration::from_millis(5),
            heartbeat: None,
            dead: None,
        };
        let mut a = IpcStream::new(Session::start("test", Arc::new(a), settings).expect("session"));
        let peer = thread::spawn(move || {
            let mut buffer = Vec::new();
            let mut received = Vec::new();
            let mut accepted = 0;
            loop {
                if !datagram::Datagrams::recv(&peer, &mut buffer, Duration::from_secs(5))
                    .expect("receive")
                {
                    continue;
                }
                let (kind, seq, payload) = datagram::parse(&buffer).expect("a frame");
                match kind {
                    datagram::DATA => {
                        // A retransmission is acknowledged again, not read twice.
                        if seq == accepted + 1 {
                            accepted = seq;
                            received.extend_from_slice(payload);
                        }
                        datagram::Datagrams::send(&peer, &datagram::frame(datagram::ACK, seq, &[]))
                            .expect("ack");
                    }
                    datagram::EOF => {
                        datagram::Datagrams::send(&peer, &datagram::frame(datagram::ABORT, 0, &[]))
                            .expect("abort");
                        return received;
                    }
                    _ => {}
                }
            }
        });
        a.write_all(b"links").expect("write");
        a.close_write().expect("the end counts as delivered");
        assert_eq!(peer.join().expect("peer"), b"links");
        let error = a.write_all(b"more").expect_err("the peer no longer reads");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    });
}

#[test]
fn datagram_frames_round_trip() {
    let frame = datagram::frame(datagram::DATA, 7, b"links");
    assert_eq!(frame.len(), datagram::HEADER + 5);
    assert_eq!(
        datagram::parse(&frame),
        Some((datagram::DATA, 7, &b"links"[..]))
    );
    assert_eq!(datagram::parse(&frame[..4]), None);
    assert_eq!(datagram::agree_chunk(4096, Some(1024)), 1024);
    assert_eq!(datagram::agree_chunk(4096, None), 4096);
    assert_eq!(datagram::agree_chunk(1, Some(1)), 16);
}

#[test]
fn ring_region_layout_matches_the_specification() {
    assert_eq!(ring::region_size(4096), 64 + 2 * (128 + 4096));
}

#[test]
fn random_ids_are_twenty_hex_digits() {
    let (a, b) = (random_id(), random_id());
    assert_eq!(a.len(), 20);
    assert!(a.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(a, b);
}
