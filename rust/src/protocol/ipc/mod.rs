//! Interchangeable inter-process communication transports (issue #140).
//!
//! The LiNo text and binary protocols need a reliable, ordered, duplex byte
//! stream. This module turns every IPC mechanism of the operating system into
//! such a stream, an [`IpcStream`], chosen by an endpoint string, so the
//! mechanism is a decorator layer under the protocol: a [`LinksServer`]
//! listens and a [`LinksClient`] connects through any of them, a server can
//! listen on several at once and [`forward`] stacks them.
//!
//! | Endpoint | Mechanism |
//! | --- | --- |
//! | `host:port`, `tcp:host:port` | TCP/IP |
//! | `udp:host:port` | UDP datagrams with a reliable session |
//! | `unix:PATH`, `unix:@NAME` | Unix domain stream socket |
//! | `pipe:NAME` | Windows named pipe; on POSIX the same as `fifo` |
//! | `fifo:DIR` | two POSIX named pipes |
//! | `stdio`, `exec:COMMAND` | anonymous pipes of standard input/output |
//! | `shm:DIR`, `mmap:DIR`, `file:DIR` | shared memory, memory-mapped file, file on disk |
//! | `signal:DIR` | memory-mapped ring announced by OS signals (Windows: events) |
//! | `mq:DIR`, `sysvmq:DIR` | POSIX and System V message queues |
//! | `mailslot:DIR`, `copydata:DIR`, `alpc:DIR` | Windows mailslots, `WM_COPYDATA`, ALPC |
//!
//! The wire format of every kind is specified in
//! `docs/protocol/ipc-transports-v1.md`; the C# and JavaScript
//! implementations follow the same document, so any of them talks to any
//! other.
//!
//! ```
//! use link_cli::protocol::ipc::Endpoint;
//!
//! let endpoint = Endpoint::parse("shm:/tmp/links?capacity=1048576").unwrap();
//! assert_eq!(endpoint.kind(), "shm");
//! assert_eq!(endpoint.address(), "/tmp/links");
//! assert_eq!(Endpoint::parse("127.0.0.1:8080").unwrap().kind(), "tcp");
//! ```
//!
//! [`LinksServer`]: super::LinksServer
//! [`LinksClient`]: super::LinksClient

mod datagram;
mod endpoint;
mod json;
mod net;
mod rendezvous;
mod ring;
mod stdio;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use endpoint::Endpoint;

use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// Every built-in endpoint kind, in documentation order.
pub const KINDS: &[&str] = &[
    "tcp", "udp", "unix", "pipe", "fifo", "stdio", "exec", "shm", "mmap", "file", "signal", "mq",
    "sysvmq", "mailslot", "copydata", "alpc",
];

/// One side of a duplex byte channel.
///
/// Reading and writing may happen on two threads at once, and
/// [`shutdown`](Self::shutdown) may come from a third one.
pub trait Transport: Send + Sync + fmt::Debug {
    /// Reads at least one byte, waiting up to `timeout` (`None`: forever).
    /// `Ok(0)` is the end of the incoming direction; a timeout is
    /// [`io::ErrorKind::TimedOut`].
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize>;
    /// Writes at least one byte of `buf`.
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    /// Pushes buffered bytes to the peer.
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
    /// Ends the outgoing direction; the peer reads the end of the stream.
    fn close_write(&self) -> io::Result<()>;
    /// Hangs up both directions; pending and later operations end.
    fn shutdown(&self);
    /// The endpoint kind, such as `shm`.
    fn kind(&self) -> &'static str;
}

/// A cloneable duplex byte stream over any [`Transport`].
///
/// Clones share the channel, so one thread can read while another writes.
#[derive(Clone)]
pub struct IpcStream {
    inner: Arc<dyn Transport>,
    read_timeout: Option<Duration>,
}

impl fmt::Debug for IpcStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IpcStream")
            .field("transport", &self.inner)
            .field("read_timeout", &self.read_timeout)
            .finish()
    }
}

impl IpcStream {
    /// Wraps a transport.
    pub fn new(transport: impl Transport + 'static) -> Self {
        Self::from_arc(Arc::new(transport))
    }

    /// Wraps a shared transport.
    pub fn from_arc(transport: Arc<dyn Transport>) -> Self {
        Self {
            inner: transport,
            read_timeout: None,
        }
    }

    /// The endpoint kind of the transport.
    pub fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    /// The transport.
    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.inner
    }

    /// Makes reads of this handle fail with [`io::ErrorKind::TimedOut`]
    /// after `timeout`; `None` waits forever.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.read_timeout = timeout;
    }

    /// The read timeout of this handle.
    pub fn read_timeout(&self) -> Option<Duration> {
        self.read_timeout
    }

    /// Ends the outgoing direction.
    pub fn close_write(&self) -> io::Result<()> {
        self.inner.close_write()
    }

    /// Hangs up both directions.
    pub fn shutdown(&self) {
        self.inner.shutdown()
    }
}

impl Read for IpcStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match self.inner.read(buf, self.read_timeout) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

impl Write for IpcStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match self.inner.write(buf) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Wakes a blocked [`Listener::accept`] so it notices that it was closed.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Accepts [`IpcStream`]s on one endpoint.
pub trait Listener: Send + fmt::Debug {
    /// Waits for the next connection; `Ok(None)` when the listener was closed
    /// or can never accept another one (a `stdio` listener after its single
    /// connection ended).
    fn accept(&mut self) -> io::Result<Option<IpcStream>>;
    /// The endpoint clients connect to, such as `shm:/tmp/links` or, for TCP,
    /// the bound `host:port`.
    fn endpoint(&self) -> String;
    /// Every endpoint clients connect to: one, except for a
    /// [`MultiListener`].
    fn endpoints(&self) -> Vec<String> {
        vec![self.endpoint()]
    }
    /// The bound TCP address, for TCP listeners.
    fn local_addr(&self) -> Option<std::net::SocketAddr> {
        None
    }
    /// A function that closes the listener from any thread: the blocked or
    /// next [`accept`](Self::accept) returns `Ok(None)`.
    fn closer(&self) -> Waker;
}

/// Connects to `endpoint` (see [`Endpoint`]).
pub fn connect(endpoint: &str) -> io::Result<IpcStream> {
    connect_endpoint(&Endpoint::parse(endpoint)?)
}

/// Connects to a parsed endpoint.
pub fn connect_endpoint(endpoint: &Endpoint) -> io::Result<IpcStream> {
    trace!("connect {endpoint}");
    let stream = match registered(endpoint.kind()) {
        Some(Registered {
            connect: Some(connect),
            ..
        }) => connect(endpoint),
        Some(_) => Err(unsupported(endpoint.kind(), "connecting")),
        None => builtin_connect(endpoint),
    };
    match &stream {
        Ok(_) => trace!("connected {endpoint}"),
        Err(error) => trace!("connect {endpoint} failed: {error}"),
    }
    stream
}

/// Listens on `endpoint` (see [`Endpoint`]).
pub fn listen(endpoint: &str) -> io::Result<Box<dyn Listener>> {
    listen_endpoint(&Endpoint::parse(endpoint)?)
}

/// Listens on a parsed endpoint.
pub fn listen_endpoint(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    let listener = match registered(endpoint.kind()) {
        Some(Registered {
            listen: Some(listen),
            ..
        }) => listen(endpoint),
        Some(_) => Err(unsupported(endpoint.kind(), "listening")),
        None => builtin_listen(endpoint),
    }?;
    trace!("listening on {}", listener.endpoint());
    Ok(listener)
}

/// Wraps a connected TCP stream (Nagle's algorithm is turned off).
pub fn from_tcp_stream(stream: std::net::TcpStream) -> IpcStream {
    net::tcp_stream(stream)
}

/// Wraps an already bound TCP listener.
pub fn from_tcp_listener(listener: std::net::TcpListener) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(net::TcpIpcListener::new(listener)?))
}

/// Listens on every endpoint at once; one listener is returned as it is.
pub fn listen_all<S: AsRef<str>>(endpoints: &[S]) -> io::Result<Box<dyn Listener>> {
    let mut listeners = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        listeners.push(listen(endpoint.as_ref())?);
    }
    match listeners.len() {
        0 => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no endpoint to listen on",
        )),
        1 => Ok(listeners.pop().expect("one listener")),
        _ => Ok(Box::new(MultiListener::new(listeners))),
    }
}

/// Whether this build can connect to (`listen == false`) or listen on the
/// kind on the current platform.
pub fn supports(kind: &str, listen: bool) -> bool {
    if let Some(registered) = registered(kind) {
        return if listen {
            registered.listen.is_some()
        } else {
            registered.connect.is_some()
        };
    }
    let kind = kind.to_ascii_lowercase();
    match kind.as_str() {
        "tcp" | "udp" | "mmap" | "file" | "shm" | "signal" | "pipe" => cfg!(any(unix, windows)),
        "stdio" => listen,
        "exec" => !listen,
        "unix" => cfg!(any(unix, windows)),
        "fifo" => cfg!(unix),
        "sysvmq" => cfg!(any(target_os = "linux", target_vendor = "apple")),
        "mq" => cfg!(target_os = "linux"),
        "mailslot" | "copydata" => cfg!(windows),
        "alpc" => cfg!(all(windows, target_pointer_width = "64")),
        _ => false,
    }
}

/// Opens an outgoing connection of a custom kind.
pub type ConnectFn = Arc<dyn Fn(&Endpoint) -> io::Result<IpcStream> + Send + Sync>;
/// Opens a listener of a custom kind.
pub type ListenFn = Arc<dyn Fn(&Endpoint) -> io::Result<Box<dyn Listener>> + Send + Sync>;

#[derive(Clone)]
struct Registered {
    connect: Option<ConnectFn>,
    listen: Option<ListenFn>,
}

fn registry() -> &'static Mutex<BTreeMap<String, Registered>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<String, Registered>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn registered(kind: &str) -> Option<Registered> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&kind.to_ascii_lowercase())
        .cloned()
}

/// Whether `kind` was added with [`register`].
pub(crate) fn is_registered(kind: &str) -> bool {
    registered(kind).is_some()
}

/// Adds a custom endpoint kind, such as a transport over a device driver.
///
/// `kind` is lowercase ASCII letters, digits and `-`; built-in kinds cannot
/// be replaced, and registering a kind again replaces the earlier one.
pub fn register(
    kind: &str,
    connect: Option<ConnectFn>,
    listen: Option<ListenFn>,
) -> io::Result<()> {
    let kind = kind.to_ascii_lowercase();
    if kind.is_empty()
        || !kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid endpoint kind `{kind}`"),
        ));
    }
    if KINDS.contains(&kind.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("`{kind}` is a built-in endpoint kind"),
        ));
    }
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(kind, Registered { connect, listen });
    Ok(())
}

fn unsupported(kind: &str, what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{what} on `{kind}:` endpoints is not supported on this platform"),
    )
}

fn builtin_connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
    match endpoint.kind() {
        "tcp" => net::connect_tcp(endpoint),
        "udp" => net::connect_udp(endpoint),
        "exec" => stdio::connect_exec(endpoint),
        "stdio" => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "`stdio` can only be listened on; connect with `exec:COMMAND` instead",
        )),
        "mmap" | "file" | "shm" | "signal" => ring::connect(endpoint),
        "unix" => net::connect_unix(endpoint),
        #[cfg(unix)]
        "pipe" | "fifo" => unix::connect_fifo(endpoint),
        #[cfg(windows)]
        "pipe" => windows::connect_pipe(endpoint),
        #[cfg(any(target_os = "linux", target_vendor = "apple"))]
        "sysvmq" => unix::connect_sysvmq(endpoint),
        #[cfg(target_os = "linux")]
        "mq" => unix::connect_mq(endpoint),
        #[cfg(windows)]
        "mailslot" => windows::connect_mailslot(endpoint),
        #[cfg(windows)]
        "copydata" => windows::connect_copydata(endpoint),
        #[cfg(all(windows, target_pointer_width = "64"))]
        "alpc" => windows::connect_alpc(endpoint),
        kind => Err(unsupported(kind, "connecting")),
    }
}

fn builtin_listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    match endpoint.kind() {
        "tcp" => net::listen_tcp(endpoint),
        "udp" => net::listen_udp(endpoint),
        "stdio" => stdio::listen_stdio(endpoint),
        "exec" => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "`exec` can only be connected to; listen on `stdio` in the started command",
        )),
        "mmap" | "file" | "shm" | "signal" => ring::listen(endpoint),
        "unix" => net::listen_unix(endpoint),
        #[cfg(unix)]
        "pipe" | "fifo" => unix::listen_fifo(endpoint),
        #[cfg(windows)]
        "pipe" => windows::listen_pipe(endpoint),
        #[cfg(any(target_os = "linux", target_vendor = "apple"))]
        "sysvmq" => unix::listen_sysvmq(endpoint),
        #[cfg(target_os = "linux")]
        "mq" => unix::listen_mq(endpoint),
        #[cfg(windows)]
        "mailslot" => windows::listen_mailslot(endpoint),
        #[cfg(windows)]
        "copydata" => windows::listen_copydata(endpoint),
        #[cfg(all(windows, target_pointer_width = "64"))]
        "alpc" => windows::listen_alpc(endpoint),
        kind => Err(unsupported(kind, "listening")),
    }
}

/// Listens on several endpoints at once; see [`listen_all`].
pub struct MultiListener {
    endpoints: Vec<String>,
    local_addr: Option<std::net::SocketAddr>,
    closers: Vec<Waker>,
    incoming: mpsc::Receiver<Option<io::Result<IpcStream>>>,
    open: usize,
    closed: Arc<AtomicBool>,
}

impl fmt::Debug for MultiListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MultiListener")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

impl MultiListener {
    /// Accepts on every listener; each one runs on a thread of its own.
    pub fn new(listeners: Vec<Box<dyn Listener>>) -> Self {
        let (sender, incoming) = mpsc::channel();
        let endpoints = listeners
            .iter()
            .flat_map(|listener| listener.endpoints())
            .collect();
        let local_addr = listeners.iter().find_map(|listener| listener.local_addr());
        let closers = listeners.iter().map(|listener| listener.closer()).collect();
        let open = listeners.len();
        for mut listener in listeners {
            let sender = sender.clone();
            thread::spawn(move || loop {
                match listener.accept() {
                    Ok(Some(stream)) => {
                        if sender.send(Some(Ok(stream))).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        let _ = sender.send(None);
                        return;
                    }
                    Err(error) => {
                        if sender.send(Some(Err(error))).is_err() {
                            return;
                        }
                    }
                }
            });
        }
        Self {
            endpoints,
            local_addr,
            closers,
            incoming,
            open,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Listener for MultiListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        while self.open > 0 && !self.closed.load(Ordering::SeqCst) {
            match self.incoming.recv() {
                Ok(Some(result)) => return result.map(Some),
                Ok(None) => self.open -= 1,
                Err(_) => break,
            }
        }
        Ok(None)
    }

    fn endpoint(&self) -> String {
        self.endpoints.join(" ")
    }

    fn endpoints(&self) -> Vec<String> {
        self.endpoints.clone()
    }

    fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.local_addr
    }

    fn closer(&self) -> Waker {
        let closers = self.closers.clone();
        let closed = Arc::clone(&self.closed);
        Arc::new(move || {
            closed.store(true, Ordering::SeqCst);
            for closer in &closers {
                closer();
            }
        })
    }
}

impl Drop for MultiListener {
    fn drop(&mut self) {
        for closer in &self.closers {
            closer();
        }
    }
}

/// Copies both directions between `a` and `b` until both ended, passing
/// the end of each direction on with [`IpcStream::close_write`]. Returns
/// the bytes copied from `a` to `b` and from `b` to `a`.
///
/// This stacks transports: a relay that accepts on one endpoint and
/// forwards to another one never parses the protocol.
pub fn forward(a: IpcStream, b: IpcStream) -> io::Result<(u64, u64)> {
    let (a2, b2) = (a.clone(), b.clone());
    let backward = thread::spawn(move || pump(b2, a2));
    let forward = pump(a.clone(), b.clone());
    let backward = backward
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("relay thread panicked")));
    a.shutdown();
    b.shutdown();
    Ok((forward?, backward?))
}

fn pump(mut from: IpcStream, mut to: IpcStream) -> io::Result<u64> {
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0;
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                trace!("relay {} -> {}: {error}", from.kind(), to.kind());
                // The other direction cannot go on either.
                to.shutdown();
                from.shutdown();
                return Err(error);
            }
        };
        if let Err(error) = to.write_all(&buffer[..read]).and_then(|()| to.flush()) {
            trace!("relay {} -> {}: {error}", from.kind(), to.kind());
            from.shutdown();
            to.shutdown();
            return Err(error);
        }
        total += read as u64;
    }
    trace!(
        "relay {} -> {}: end after {total} bytes",
        from.kind(),
        to.kind()
    );
    let _ = to.close_write();
    Ok(total)
}

/// Accepts connections on `listener` and forwards each one to a new
/// connection to `target` (see [`forward`]) until the listener is closed or
/// exhausted. Waits for the forwarded connections to end before returning.
pub fn relay(listener: &mut dyn Listener, target: &Endpoint) -> io::Result<()> {
    let mut connections = Vec::new();
    loop {
        let stream = match listener.accept() {
            Ok(Some(stream)) => stream,
            Ok(None) => break,
            Err(error) => {
                trace!("relay accept on {} failed: {error}", listener.endpoint());
                // A failing listener must not spin.
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        };
        let target = target.clone();
        connections.push(thread::spawn(move || match connect_endpoint(&target) {
            Ok(outgoing) => {
                if let Err(error) = forward(stream, outgoing) {
                    trace!("relay to {target} ended: {error}");
                }
            }
            Err(error) => {
                crate::diagnostics::stderr!("clink: cannot forward to {target}: {error}");
                stream.shutdown();
            }
        }));
        connections.retain(|connection| !connection.is_finished());
    }
    for connection in connections {
        let _ = connection.join();
    }
    Ok(())
}

const TRACE_UNSET: u8 = 0;
const TRACE_OFF: u8 = 1;
const TRACE_ON: u8 = 2;
static TRACE: AtomicU8 = AtomicU8::new(TRACE_UNSET);

/// Turns the `[ipc]` transport log on standard error on or off. It starts
/// on when the environment variable `CLINK_IPC_TRACE` is `1`.
pub fn set_trace(enabled: bool) {
    TRACE.store(
        if enabled { TRACE_ON } else { TRACE_OFF },
        Ordering::Relaxed,
    );
}

/// Whether the `[ipc]` transport log is on.
pub fn trace_enabled() -> bool {
    match TRACE.load(Ordering::Relaxed) {
        TRACE_ON => true,
        TRACE_OFF => false,
        _ => {
            let enabled = std::env::var("CLINK_IPC_TRACE").is_ok_and(|value| value == "1");
            set_trace(enabled);
            enabled
        }
    }
}

macro_rules! trace {
    ($($argument:tt)*) => {
        if $crate::protocol::ipc::trace_enabled() {
            $crate::diagnostics::stderr!("[ipc] {}", ::std::format_args!($($argument)*));
        }
    };
}
pub(crate) use trace;

/// A fresh identifier of 20 lowercase hexadecimal digits.
pub(crate) fn random_id() -> String {
    let [a, b] = random_u64s();
    format!("{:016x}{:04x}", a, b & 0xffff)
}

/// Sixteen bytes nobody else can predict cheaply; not cryptographic.
pub(crate) fn random_token() -> [u8; 16] {
    let [a, b] = random_u64s();
    let mut token = [0; 16];
    token[..8].copy_from_slice(&a.to_le_bytes());
    token[8..].copy_from_slice(&b.to_le_bytes());
    token
}

fn random_u64s() -> [u64; 2] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    let mut values = [0; 2];
    for (index, value) in values.iter_mut().enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(count);
        hasher.write_u64(nanos);
        hasher.write_u32(std::process::id());
        hasher.write_usize(index);
        *value = hasher.finish();
    }
    values
}

/// Whether the process `pid` is still running.
pub(crate) fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unix::process_alive(pid)
    }
    #[cfg(windows)]
    {
        windows::process_alive(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

/// Watches whether a peer process is alive at most every 100 ms.
#[derive(Debug)]
pub(crate) struct PeerWatch {
    pid: u32,
    state: Mutex<(web_time::Instant, bool)>,
}

impl PeerWatch {
    pub(crate) fn new(pid: u32) -> Self {
        Self {
            pid,
            state: Mutex::new((web_time::Instant::now(), true)),
        }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    pub(crate) fn alive(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.1 && state.0.elapsed() >= Duration::from_millis(100) {
            state.1 = process_alive(self.pid);
            state.0 = web_time::Instant::now();
        }
        state.1
    }
}

pub(crate) fn hung_up() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionReset, "the peer hung up")
}

pub(crate) fn broken_pipe() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the peer no longer reads")
}

pub(crate) fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "timed out")
}

#[cfg(test)]
mod tests;
