//! Sockets: `tcp`, `unix` and `udp`.

use super::datagram::{self, agree_chunk, Datagrams, Session, Settings, HELLO, WELCOME};
use super::{random_token, timed_out, trace, Endpoint, IpcStream, Listener, Transport, Waker};
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{
    Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use web_time::Instant;

/// A connected stream socket.
pub(crate) trait StreamSocket: Send + Sync + fmt::Debug + 'static {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    fn shutdown(&self, how: Shutdown) -> io::Result<()>;
}

impl StreamSocket for TcpStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, timeout)
    }
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut socket = self;
        Read::read(&mut socket, buf)
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut socket = self;
        Write::write(&mut socket, buf)
    }
    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        TcpStream::shutdown(self, how)
    }
}

#[cfg(unix)]
impl StreamSocket for std::os::unix::net::UnixStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        std::os::unix::net::UnixStream::set_read_timeout(self, timeout)
    }
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut socket = self;
        Read::read(&mut socket, buf)
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut socket = self;
        Write::write(&mut socket, buf)
    }
    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        std::os::unix::net::UnixStream::shutdown(self, how)
    }
}

/// A stream socket as a [`Transport`].
#[derive(Debug)]
pub(crate) struct SocketTransport<S: StreamSocket> {
    socket: S,
    timeout: Mutex<Option<Duration>>,
    kind: &'static str,
}

impl<S: StreamSocket> SocketTransport<S> {
    pub(crate) fn new(socket: S, kind: &'static str) -> Self {
        Self {
            socket,
            timeout: Mutex::new(None),
            kind,
        }
    }
}

impl<S: StreamSocket> Transport for SocketTransport<S> {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        {
            let timeout = timeout.map(|timeout| timeout.max(Duration::from_millis(1)));
            let mut current = self
                .timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *current != timeout {
                self.socket.set_read_timeout(timeout)?;
                *current = timeout;
            }
        }
        match self.socket.read(buf) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Err(timed_out())
            }
            // A hung-up socket ends like a closed one.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotConnected | io::ErrorKind::ConnectionAborted
                ) =>
            {
                Ok(0)
            }
            result => result,
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.socket.write(buf)
    }

    fn close_write(&self) -> io::Result<()> {
        match self.socket.shutdown(Shutdown::Write) {
            Err(error) if error.kind() == io::ErrorKind::NotConnected => Ok(()),
            result => result,
        }
    }

    fn shutdown(&self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }

    fn kind(&self) -> &'static str {
        self.kind
    }
}

fn resolve(address: &str) -> io::Result<Vec<SocketAddr>> {
    let addresses: Vec<SocketAddr> = address.to_socket_addrs()?.collect();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{address}` resolves to no address"),
        ));
    }
    Ok(addresses)
}

/// Wraps a connected TCP stream; one that refuses to turn Nagle's algorithm off still works, only slower.
pub(crate) fn tcp_stream(stream: TcpStream) -> IpcStream {
    if let Err(error) = stream.set_nodelay(true) {
        trace!("could not disable Nagle's algorithm: {error}");
    }
    IpcStream::new(SocketTransport::new(stream, "tcp"))
}

pub(crate) fn connect_tcp(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let mut last = None;
    for address in resolve(endpoint.address())? {
        let connected = if endpoint.option("timeout").is_some() {
            TcpStream::connect_timeout(&address, endpoint.timeout())
        } else {
            TcpStream::connect(address)
        };
        match connected {
            Ok(stream) => return Ok(tcp_stream(stream)),
            Err(error) => last = Some(error),
        }
    }
    Err(last.expect("at least one address"))
}

/// A TCP listener.
#[derive(Debug)]
pub(crate) struct TcpIpcListener {
    listener: TcpListener,
    address: SocketAddr,
    closed: Arc<AtomicBool>,
}

impl TcpIpcListener {
    pub(crate) fn new(listener: TcpListener) -> io::Result<Self> {
        Ok(Self {
            address: listener.local_addr()?,
            listener,
            closed: Arc::new(AtomicBool::new(false)),
        })
    }
}

/// Where to connect to reach a socket bound to `address`.
fn reachable(address: SocketAddr) -> SocketAddr {
    let mut target = address;
    if target.ip().is_unspecified() {
        target.set_ip(match address {
            SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    target
}

impl Listener for TcpIpcListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if self.closed.load(Ordering::SeqCst) {
                        return Ok(None);
                    }
                    trace!("tcp accepted {peer}");
                    return Ok(Some(tcp_stream(stream)));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn endpoint(&self) -> String {
        self.address.to_string()
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        Some(self.address)
    }

    fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        let target = reachable(self.address);
        Arc::new(move || {
            if !closed.swap(true, Ordering::SeqCst) {
                let _ = TcpStream::connect_timeout(&target, Duration::from_secs(1));
            }
        })
    }
}

pub(crate) fn listen_tcp(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(TcpIpcListener::new(TcpListener::bind(
        endpoint.address(),
    )?)?))
}

// ---------------------------------------------------------------- unix

#[cfg(unix)]
mod unix_socket {
    use super::*;
    use std::os::unix::net::{SocketAddr as UnixAddr, UnixListener, UnixStream};
    use std::path::PathBuf;

    fn address(name: &str) -> io::Result<Option<UnixAddr>> {
        let Some(abstract_name) = name.strip_prefix('@') else {
            return Ok(None);
        };
        #[cfg(target_os = "linux")]
        {
            use std::os::linux::net::SocketAddrExt;
            UnixAddr::from_abstract_name(abstract_name.as_bytes()).map(Some)
        }
        #[cfg(target_os = "android")]
        {
            use std::os::android::net::SocketAddrExt;
            UnixAddr::from_abstract_name(abstract_name.as_bytes()).map(Some)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            let _ = abstract_name;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "abstract `unix:@NAME` sockets exist only on Linux",
            ))
        }
    }

    fn connect_name(name: &str) -> io::Result<UnixStream> {
        match address(name)? {
            Some(address) => UnixStream::connect_addr(&address),
            None => UnixStream::connect(name),
        }
    }

    pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let stream = connect_name(endpoint.address())?;
        Ok(IpcStream::new(SocketTransport::new(stream, "unix")))
    }

    #[derive(Debug)]
    struct UnixIpcListener {
        listener: UnixListener,
        name: String,
        path: Option<PathBuf>,
        closed: Arc<AtomicBool>,
    }

    pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        let name = endpoint.address().to_string();
        let (listener, path) = match address(&name)? {
            Some(address) => (UnixListener::bind_addr(&address)?, None),
            None => {
                let path = PathBuf::from(&name);
                if path.exists() {
                    match UnixStream::connect(&path) {
                        Ok(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::AddrInUse,
                                format!("a server already listens on `{name}`"),
                            ))
                        }
                        Err(_) => {
                            trace!("unix removes the stale socket `{name}`");
                            std::fs::remove_file(&path)?;
                        }
                    }
                }
                (UnixListener::bind(&path)?, Some(path))
            }
        };
        Ok(Box::new(UnixIpcListener {
            listener,
            name,
            path,
            closed: Arc::new(AtomicBool::new(false)),
        }))
    }

    impl Listener for UnixIpcListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            loop {
                if self.closed.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if self.closed.load(Ordering::SeqCst) {
                            return Ok(None);
                        }
                        trace!("unix accepted on {}", self.name);
                        return Ok(Some(IpcStream::new(SocketTransport::new(stream, "unix"))));
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                        ) => {}
                    Err(error) => return Err(error),
                }
            }
        }

        fn endpoint(&self) -> String {
            format!("unix:{}", self.name)
        }

        fn closer(&self) -> Waker {
            let closed = Arc::clone(&self.closed);
            let name = self.name.clone();
            Arc::new(move || {
                if !closed.swap(true, Ordering::SeqCst) {
                    let _ = connect_name(&name);
                }
            })
        }
    }

    impl Drop for UnixIpcListener {
        fn drop(&mut self) {
            if let Some(path) = &self.path {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

pub(crate) fn connect_unix(endpoint: &Endpoint) -> io::Result<IpcStream> {
    #[cfg(unix)]
    {
        unix_socket::connect(endpoint)
    }
    #[cfg(windows)]
    {
        super::windows::connect_unix(endpoint)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(super::unsupported(endpoint.kind(), "connecting"))
    }
}

pub(crate) fn listen_unix(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    #[cfg(unix)]
    {
        unix_socket::listen(endpoint)
    }
    #[cfg(windows)]
    {
        super::windows::listen_unix(endpoint)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(super::unsupported(endpoint.kind(), "listening"))
    }
}

// ---------------------------------------------------------------- udp

fn ignorable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::Interrupted
    )
}

/// A connected UDP socket.
#[derive(Debug)]
struct UdpLink {
    socket: UdpSocket,
    timeout: Mutex<Option<Duration>>,
}

impl UdpLink {
    fn new(socket: UdpSocket) -> Self {
        Self {
            socket,
            timeout: Mutex::new(None),
        }
    }
}

impl Datagrams for UdpLink {
    fn send(&self, frame: &[u8]) -> io::Result<bool> {
        match self.socket.send(frame) {
            Ok(_) => Ok(true),
            Err(error) if ignorable(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
        let timeout = Some(timeout.max(Duration::from_millis(1)));
        {
            let mut current = self
                .timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *current != timeout {
                self.socket.set_read_timeout(timeout)?;
                *current = timeout;
            }
        }
        buf.resize(65536, 0);
        match self.socket.recv(buf) {
            Ok(count) => {
                buf.truncate(count);
                Ok(true)
            }
            Err(error) if ignorable(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn udp_settings(endpoint: &Endpoint, chunk: usize) -> io::Result<Settings> {
    Ok(Settings {
        chunk,
        rto: endpoint.millis("rto", 20)?,
        heartbeat: Some(
            endpoint
                .millis("heartbeat", 1000)?
                .max(Duration::from_millis(1)),
        ),
        dead: Some(
            endpoint
                .millis("dead", 10_000)?
                .max(Duration::from_millis(1)),
        ),
    })
}

fn unspecified(address: &SocketAddr) -> SocketAddr {
    match address {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    }
}

fn hello_payload(token: &[u8; 16], chunk: u32) -> Vec<u8> {
    let mut payload = token.to_vec();
    payload.extend_from_slice(&chunk.to_le_bytes());
    payload
}

fn token_and_chunk(payload: &[u8]) -> Option<([u8; 16], u32)> {
    if payload.len() != 20 {
        return None;
    }
    let token: [u8; 16] = payload[..16].try_into().ok()?;
    let chunk = u32::from_le_bytes(payload[16..].try_into().ok()?);
    Some((token, chunk))
}

pub(crate) fn connect_udp(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let server = resolve(endpoint.address())?[0];
    let socket = UdpSocket::bind(unspecified(&server))?;
    let own_chunk = endpoint.number("chunk", 1200, 16, 65000)?;
    let token = random_token();
    let hello = datagram::frame(HELLO, 0, &hello_payload(&token, own_chunk as u32));
    let deadline = Instant::now() + endpoint.timeout();
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut buffer = vec![0; 65536];
    let mut next_hello = Instant::now();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no `udp` listener answered at {server}"),
            ));
        }
        if now >= next_hello {
            match socket.send_to(&hello, server) {
                Ok(_) => {}
                Err(error) if ignorable(&error) => {}
                Err(error) => return Err(error),
            }
            next_hello = now + Duration::from_millis(100);
        }
        let (count, source) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(error) if ignorable(&error) => continue,
            Err(error) => return Err(error),
        };
        let Some((WELCOME, _, payload)) = datagram::parse(&buffer[..count]) else {
            continue;
        };
        let Some((welcome_token, chunk)) = token_and_chunk(payload) else {
            continue;
        };
        if welcome_token != token {
            continue;
        }
        socket.connect(source)?;
        let chunk = agree_chunk(own_chunk, Some(chunk.into()));
        trace!("udp connected to {source} through {server}, chunk {chunk}");
        let session = Session::start(
            "udp",
            Arc::new(UdpLink::new(socket)),
            udp_settings(endpoint, chunk)?,
        )?;
        return Ok(IpcStream::new(session));
    }
}

/// The session socket of one client, the WELCOME sent from it, and when.
type UdpSession = (UdpSocket, Vec<u8>, Instant);

#[derive(Debug)]
struct UdpIpcListener {
    socket: UdpSocket,
    address: SocketAddr,
    endpoint: Endpoint,
    own_chunk: u64,
    /// `(source, token)` → the session socket and the WELCOME sent from it.
    sessions: HashMap<(SocketAddr, [u8; 16]), UdpSession>,
    closed: Arc<AtomicBool>,
}

const WELCOME_KEPT: Duration = Duration::from_secs(30);

pub(crate) fn listen_udp(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    let socket = UdpSocket::bind(endpoint.address())?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    Ok(Box::new(UdpIpcListener {
        address: socket.local_addr()?,
        socket,
        own_chunk: endpoint.number("chunk", 1200, 16, 65000)?,
        endpoint: endpoint.clone(),
        sessions: HashMap::new(),
        closed: Arc::new(AtomicBool::new(false)),
    }))
}

impl Listener for UdpIpcListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        let mut buffer = vec![0; 65536];
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let (count, source) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(error) if ignorable(&error) => continue,
                Err(error) => return Err(error),
            };
            let now = Instant::now();
            self.sessions
                .retain(|_, (_, _, created)| now.duration_since(*created) < WELCOME_KEPT);
            let Some((HELLO, _, payload)) = datagram::parse(&buffer[..count]) else {
                continue;
            };
            let Some((token, chunk)) = token_and_chunk(payload) else {
                continue;
            };
            if let Some((socket, welcome, _)) = self.sessions.get(&(source, token)) {
                let _ = socket.send(welcome);
                continue;
            }
            let mut own = self.address;
            own.set_port(0);
            let socket = UdpSocket::bind(own)?;
            socket.connect(source)?;
            let chunk = agree_chunk(self.own_chunk, Some(chunk.into()));
            let welcome = datagram::frame(WELCOME, 0, &hello_payload(&token, chunk as u32));
            let _ = socket.send(&welcome);
            self.sessions
                .insert((source, token), (socket.try_clone()?, welcome, now));
            trace!(
                "udp accepted {source} on {}, chunk {chunk}",
                socket.local_addr()?
            );
            let session = Session::start(
                "udp",
                Arc::new(UdpLink::new(socket)),
                udp_settings(&self.endpoint, chunk)?,
            )?;
            return Ok(Some(IpcStream::new(session)));
        }
    }

    fn endpoint(&self) -> String {
        format!("udp:{}", self.address)
    }

    fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        Arc::new(move || closed.store(true, Ordering::SeqCst))
    }
}
