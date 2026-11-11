//! POSIX mechanisms: process liveness, FIFOs, shared memory, signals and
//! message queues.

use super::datagram::{agree_chunk, local_settings, Datagrams, Session};
use super::json::Object;
use super::rendezvous::{accept_with, Backoff, Rendezvous, RendezvousListener, Request};
use super::ring::{Mapping, Notifier};
use super::{timed_out, trace, Endpoint, IpcStream, Listener, PeerWatch, Transport, Waker};
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use web_time::Instant;

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    if result == -1 {
        Err(last_error())
    } else {
        Ok(result)
    }
}

fn c_string(text: &str) -> io::Result<CString> {
    CString::new(text).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a name must not contain NUL bytes",
        )
    })
}

fn c_path(path: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a path must not contain NUL bytes",
        )
    })
}

/// Whether the process `pid` is still running.
pub(crate) fn process_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: signal 0 only checks for existence and permission.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || last_error().raw_os_error() == Some(libc::EPERM)
}

/// Replaces standard output with `/dev/null`, so the reader of the original
/// one sees its end.
pub(crate) fn close_stdout() -> io::Result<()> {
    let null = File::options().write(true).open("/dev/null")?;
    // SAFETY: duplicating a valid descriptor onto descriptor 1.
    check(unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) })?;
    Ok(())
}

// ---------------------------------------------------------------- mappings

fn map(fd: RawFd, len: usize) -> io::Result<Mapping> {
    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot map an empty region",
        ));
    }
    // SAFETY: a fresh shared mapping of `len` bytes of a valid descriptor.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return Err(last_error());
    }
    let address = ptr as usize;
    // SAFETY: the mapping is valid until the release unmaps it.
    Ok(unsafe {
        Mapping::new(
            ptr.cast(),
            len,
            Box::new(move || {
                libc::munmap(address as *mut libc::c_void, len);
            }),
        )
    })
}

/// Maps a whole regular file.
pub(crate) fn map_file(file: &File, len: usize) -> io::Result<Mapping> {
    map(file.as_raw_fd(), len)
}

fn shm_name(id: &str) -> io::Result<CString> {
    c_string(&format!("/lc{id}"))
}

fn shm_open(name: &CString, flags: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: a valid name; the mode is passed as the C variadic expects.
    #[cfg(target_vendor = "apple")]
    let fd = unsafe { libc::shm_open(name.as_ptr(), flags, 0o600 as libc::c_uint) };
    #[cfg(not(target_vendor = "apple"))]
    let fd = unsafe { libc::shm_open(name.as_ptr(), flags, 0o600 as libc::mode_t) };
    // SAFETY: a descriptor this call just opened.
    Ok(unsafe { OwnedFd::from_raw_fd(check(fd)?) })
}

/// Creates the POSIX shared memory object `/lcID`.
pub(crate) fn create_shm(id: &str, len: usize) -> io::Result<Mapping> {
    let name = shm_name(id)?;
    let fd = shm_open(&name, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
    // SAFETY: a valid descriptor.
    let sized = check(unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) });
    let mapped = sized.and_then(|_| map(fd.as_raw_fd(), len));
    if mapped.is_err() {
        unlink_shm(id);
    }
    mapped
}

/// Opens the POSIX shared memory object `/lcID`.
pub(crate) fn open_shm(id: &str) -> io::Result<Mapping> {
    let fd = shm_open(&shm_name(id)?, libc::O_RDWR)?;
    // SAFETY: an all-zero `stat` is a valid output buffer.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: a valid descriptor and buffer.
    check(unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) })?;
    map(fd.as_raw_fd(), stat.st_size as usize)
}

/// Removes the name of `/lcID`; mappings stay valid.
pub(crate) fn unlink_shm(id: &str) {
    if let Ok(name) = shm_name(id) {
        // SAFETY: a valid name.
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
}

// ---------------------------------------------------------------- signals

/// Bumped by the dispatcher thread on every caught signal.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);

struct Wake {
    lock: Mutex<()>,
    changed: Condvar,
    installed: Mutex<Vec<libc::c_int>>,
}

fn wake() -> &'static Wake {
    static WAKE: OnceLock<Wake> = OnceLock::new();
    WAKE.get_or_init(|| Wake {
        lock: Mutex::new(()),
        changed: Condvar::new(),
        installed: Mutex::new(Vec::new()),
    })
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
unsafe fn errno_location() -> *mut libc::c_int {
    #[cfg(target_os = "android")]
    {
        libc::__errno()
    }
    #[cfg(not(target_os = "android"))]
    {
        libc::__errno_location()
    }
}

#[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__error()
}

extern "C" fn on_signal(_signal: libc::c_int) {
    let fd = PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = 1u8;
        // SAFETY: `write` is async-signal-safe; errno is restored after it.
        unsafe {
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "emscripten",
                target_vendor = "apple",
                target_os = "freebsd"
            ))]
            let saved = *errno_location();
            libc::write(fd, (&byte as *const u8).cast(), 1);
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "emscripten",
                target_vendor = "apple",
                target_os = "freebsd"
            ))]
            {
                *errno_location() = saved;
            }
        }
    }
}

fn set_flags(fd: RawFd, nonblocking: bool) -> io::Result<()> {
    // SAFETY: fcntl on a valid descriptor.
    unsafe {
        let flags = check(libc::fcntl(fd, libc::F_GETFL))?;
        let flags = if nonblocking {
            flags | libc::O_NONBLOCK
        } else {
            flags & !libc::O_NONBLOCK
        };
        check(libc::fcntl(fd, libc::F_SETFL, flags))?;
        let fd_flags = check(libc::fcntl(fd, libc::F_GETFD))?;
        check(libc::fcntl(fd, libc::F_SETFD, fd_flags | libc::FD_CLOEXEC))?;
    }
    Ok(())
}

fn start_dispatcher() -> io::Result<()> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| {
            let mut fds = [0; 2];
            // SAFETY: a two-element output array.
            check(unsafe { libc::pipe(fds.as_mut_ptr()) }).map_err(|error| error.to_string())?;
            let (read, write) = (fds[0], fds[1]);
            set_flags(read, false).map_err(|error| error.to_string())?;
            set_flags(write, true).map_err(|error| error.to_string())?;
            PIPE_WRITE.store(write, Ordering::SeqCst);
            thread::Builder::new()
                .name("clink-signal".into())
                .spawn(move || {
                    let mut buffer = [0u8; 64];
                    loop {
                        // SAFETY: reading into a local buffer.
                        let read = unsafe { libc::read(read, buffer.as_mut_ptr().cast(), 64) };
                        if read < 0 && last_error().kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        if read <= 0 {
                            return;
                        }
                        let wake = wake();
                        let _guard = wake.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        GENERATION.fetch_add(1, Ordering::SeqCst);
                        wake.changed.notify_all();
                    }
                })
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(io::Error::other)
}

/// Catches `signo` from now on, turning it into ring wake-ups.
fn install(signo: libc::c_int) -> io::Result<()> {
    start_dispatcher()?;
    let wake = wake();
    let mut installed = wake.installed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if installed.contains(&signo) {
        return Ok(());
    }
    if matches!(signo, libc::SIGKILL | libc::SIGSTOP) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("signal {signo} cannot be caught"),
        ));
    }
    // SAFETY: an all-zero `sigaction` is a valid starting point.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
    action.sa_flags = libc::SA_RESTART;
    // SAFETY: valid pointers; the handler only writes to a pipe.
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        check(libc::sigaction(signo, &action, std::ptr::null_mut()))?;
    }
    trace!("signal handler installed for signal {signo}");
    installed.push(signo);
    Ok(())
}

/// Wakes the peer with a signal and waits for the peer's.
#[derive(Debug)]
pub(crate) struct SignalNotifier {
    peer: libc::pid_t,
    peer_signal: libc::c_int,
}

impl Notifier for SignalNotifier {
    fn generation(&self) -> u64 {
        GENERATION.load(Ordering::SeqCst)
    }

    fn notify(&self) {
        // SAFETY: sending a signal the peer asked for.
        unsafe { libc::kill(self.peer, self.peer_signal) };
    }

    fn wait(&self, generation: u64, attempt: u32, limit: Duration) {
        if attempt < 16 {
            std::hint::spin_loop();
            return;
        }
        let wake = wake();
        let guard = wake.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = wake.changed.wait_timeout(guard, limit);
    }
}

fn own_signal(endpoint: &Endpoint) -> io::Result<libc::c_int> {
    let signo = endpoint.number("signo", libc::SIGUSR2 as u64, 1, 64)? as libc::c_int;
    install(signo)?;
    Ok(signo)
}

fn peer_signal(fields: &Object) -> io::Result<libc::c_int> {
    let signo = fields.require_number("signal")?;
    if !(1..=64).contains(&signo) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the peer asked for the invalid signal {signo}"),
        ));
    }
    Ok(signo as libc::c_int)
}

/// The client's signal setup before the request.
pub(crate) struct SignalSetup {
    _own: libc::c_int,
}

impl SignalSetup {
    pub(crate) fn finish(self, ack: &Object, peer: u32) -> io::Result<Arc<dyn Notifier>> {
        Ok(Arc::new(SignalNotifier {
            peer: peer as libc::pid_t,
            peer_signal: peer_signal(ack)?,
        }))
    }
}

pub(crate) fn signal_client(endpoint: &Endpoint, _id: &str) -> io::Result<(Object, SignalSetup)> {
    let own = own_signal(endpoint)?;
    Ok((Object::new().with("signal", own as u32), SignalSetup { _own: own }))
}

pub(crate) fn signal_server(
    endpoint: &Endpoint,
    _id: &str,
    request: &Object,
    peer: u32,
) -> io::Result<(Object, Arc<dyn Notifier>)> {
    let own = own_signal(endpoint)?;
    let notifier = SignalNotifier {
        peer: peer as libc::pid_t,
        peer_signal: peer_signal(request)?,
    };
    Ok((Object::new().with("signal", own as u32), Arc::new(notifier)))
}

// ---------------------------------------------------------------- fifo

fn mkfifo(path: &Path) -> io::Result<()> {
    let name = c_path(path)?;
    // SAFETY: a valid path.
    check(unsafe { libc::mkfifo(name.as_ptr(), 0o600) })?;
    Ok(())
}

fn open_fifo(path: &Path, write: bool) -> io::Result<OwnedFd> {
    let name = c_path(path)?;
    let mode = if write { libc::O_WRONLY } else { libc::O_RDONLY };
    // SAFETY: a valid path.
    let fd = check(unsafe { libc::open(name.as_ptr(), mode | libc::O_NONBLOCK | libc::O_CLOEXEC) })?;
    // SAFETY: a descriptor this call just opened.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Waits until `fd` is ready for `events`, at most `timeout`.
fn poll(fd: RawFd, events: libc::c_short, timeout: Duration) -> io::Result<bool> {
    let mut entry = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    let millis = timeout.as_millis().clamp(1, 100) as libc::c_int;
    // SAFETY: one valid entry.
    match unsafe { libc::poll(&mut entry, 1, millis) } {
        -1 if last_error().kind() == io::ErrorKind::Interrupted => Ok(false),
        -1 => Err(last_error()),
        0 => Ok(false),
        _ => Ok(true),
    }
}

/// Two FIFOs, one per direction.
#[derive(Debug)]
struct FifoTransport {
    incoming: OwnedFd,
    outgoing: Mutex<Option<OwnedFd>>,
    greeted: AtomicBool,
    reading: Mutex<()>,
    closed: AtomicBool,
    peer: PeerWatch,
    kind: &'static str,
}

impl FifoTransport {
    fn new(
        kind: &'static str,
        incoming: OwnedFd,
        outgoing: OwnedFd,
        peer: u32,
    ) -> io::Result<Self> {
        let transport = Self {
            incoming,
            outgoing: Mutex::new(Some(outgoing)),
            greeted: AtomicBool::new(false),
            reading: Mutex::new(()),
            closed: AtomicBool::new(false),
            peer: PeerWatch::new(peer),
            kind,
        };
        // The hello byte: tells the peer that this writer is open.
        let mut written = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while written == 0 {
            written = transport.write(&[0])?;
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
        }
        Ok(transport)
    }
}

impl Transport for FifoTransport {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        let _reading = self.reading.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let fd = self.incoming.as_raw_fd();
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(0);
            }
            let wait = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(timed_out());
                    }
                    deadline - now
                }
                None => Duration::from_millis(100),
            };
            if !poll(fd, libc::POLLIN, wait)? {
                if !self.greeted.load(Ordering::SeqCst) && !self.peer.alive() {
                    return Err(super::hung_up());
                }
                continue;
            }
            // SAFETY: reading into `buf`.
            let read = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if read < 0 {
                let error = last_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                return Err(error);
            }
            let read = read as usize;
            if read == 0 {
                if self.greeted.load(Ordering::SeqCst) {
                    return Ok(0);
                }
                // The writer has not opened yet.
                if !self.peer.alive() {
                    return Err(super::hung_up());
                }
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            if !self.greeted.swap(true, Ordering::SeqCst) {
                buf.copy_within(1..read, 0);
                if read == 1 {
                    continue;
                }
                return Ok(read - 1);
            }
            return Ok(read);
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let outgoing = self.outgoing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(fd) = outgoing.as_ref().map(AsRawFd::as_raw_fd) else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the outgoing direction was closed",
            ));
        };
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the connection was closed",
                ));
            }
            // SAFETY: writing from `buf`.
            let written = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
            if written >= 0 {
                return Ok(written as usize);
            }
            let error = last_error();
            match error.kind() {
                io::ErrorKind::WouldBlock => {
                    poll(fd, libc::POLLOUT, Duration::from_millis(100))?;
                }
                io::ErrorKind::Interrupted => {}
                _ => return Err(error),
            }
        }
    }

    fn close_write(&self) -> io::Result<()> {
        drop(
            self.outgoing
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take(),
        );
        Ok(())
    }

    fn shutdown(&self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            trace!("{} to process {} hung up", self.kind, self.peer.pid());
            let _ = self.close_write();
        }
    }

    fn kind(&self) -> &'static str {
        self.kind
    }
}

fn fifo_kind(endpoint: &Endpoint) -> &'static str {
    if endpoint.kind() == "pipe" {
        "pipe"
    } else {
        "fifo"
    }
}

pub(crate) fn connect_fifo(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let kind = fifo_kind(endpoint);
    let rendezvous = Rendezvous::begin(endpoint, "fifo")?;
    let (c2s, s2c) = (rendezvous.path("c2s"), rendezvous.path("s2c"));
    let remove = || {
        let _ = std::fs::remove_file(&c2s);
        let _ = std::fs::remove_file(&s2c);
    };
    let opened = mkfifo(&c2s)
        .and_then(|()| mkfifo(&s2c))
        .and_then(|()| open_fifo(&s2c, false));
    let incoming = match opened {
        Ok(incoming) => incoming,
        Err(error) => {
            remove();
            return Err(error);
        }
    };
    let result = rendezvous.request(Object::new()).and_then(|ack| {
        let peer = ack.number("pid").map_or(rendezvous.server_pid, |pid| pid as u32);
        let outgoing = open_fifo(&c2s, true)?;
        FifoTransport::new(kind, incoming, outgoing, peer)
    });
    remove();
    let transport = result?;
    trace!("{kind} connected as {}", rendezvous.id);
    Ok(IpcStream::new(transport))
}

#[derive(Debug)]
struct FifoListener {
    rendezvous: RendezvousListener,
    kind: &'static str,
}

pub(crate) fn listen_fifo(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(FifoListener {
        rendezvous: RendezvousListener::bind(endpoint, "fifo")?,
        kind: fifo_kind(endpoint),
    }))
}

impl FifoListener {
    fn open(&self, request: &Request) -> io::Result<FifoTransport> {
        let outgoing = open_fifo(&request.path("s2c"), true)?;
        let incoming = open_fifo(&request.path("c2s"), false)?;
        FifoTransport::new(self.kind, incoming, outgoing, request.pid())
    }
}

impl Listener for FifoListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        accept_with(&self.rendezvous, self.kind, |request| {
            self.open(request).map(|transport| (Object::new(), IpcStream::new(transport)))
        })
    }

    fn endpoint(&self) -> String {
        self.rendezvous.endpoint()
    }

    fn closer(&self) -> Waker {
        self.rendezvous.closer()
    }
}

// ---------------------------------------------------------------- datagrams

#[cfg(target_os = "linux")]
mod posix_mq {
    use super::*;

    fn queue_name(id: &str, suffix: char) -> io::Result<CString> {
        c_string(&format!("/lc{id}{suffix}"))
    }

    fn deadline_after(timeout: Duration) -> libc::timespec {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: a valid output pointer.
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        let nanos = now.tv_nsec as u64 + u64::from(timeout.subsec_nanos());
        libc::timespec {
            tv_sec: now.tv_sec + timeout.as_secs() as libc::time_t + (nanos / 1_000_000_000) as libc::time_t,
            tv_nsec: (nanos % 1_000_000_000) as libc::c_long,
        }
    }

    #[derive(Debug)]
    struct Queue(libc::mqd_t);

    impl Queue {
        fn create(name: &CString, maxmsg: u64, msgsize: usize) -> io::Result<Self> {
            // SAFETY: an all-zero `mq_attr` is valid.
            let mut attributes: libc::mq_attr = unsafe { std::mem::zeroed() };
            attributes.mq_maxmsg = maxmsg as _;
            attributes.mq_msgsize = msgsize as _;
            // SAFETY: a valid name and attributes.
            let queue = unsafe {
                libc::mq_open(
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                    0o600 as libc::mode_t,
                    &attributes as *const libc::mq_attr,
                )
            };
            check(queue).map(Self)
        }

        fn open(name: &CString) -> io::Result<Self> {
            // SAFETY: a valid name.
            let queue = unsafe { libc::mq_open(name.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
            check(queue).map(Self)
        }

        fn message_size(&self) -> io::Result<usize> {
            // SAFETY: an all-zero `mq_attr` is a valid output buffer.
            let mut attributes: libc::mq_attr = unsafe { std::mem::zeroed() };
            // SAFETY: a valid queue.
            check(unsafe { libc::mq_getattr(self.0, &mut attributes) })?;
            Ok(attributes.mq_msgsize as usize)
        }
    }

    impl Drop for Queue {
        fn drop(&mut self) {
            // SAFETY: a queue this value owns.
            unsafe { libc::mq_close(self.0) };
        }
    }

    #[derive(Debug)]
    struct MqLink {
        send: Queue,
        receive: Queue,
        size: usize,
        peer: PeerWatch,
    }

    impl Datagrams for MqLink {
        fn send(&self, frame: &[u8]) -> io::Result<bool> {
            let deadline = deadline_after(Duration::from_millis(10));
            // SAFETY: a valid queue and frame.
            let result = unsafe {
                libc::mq_timedsend(self.send.0, frame.as_ptr().cast(), frame.len(), 0, &deadline)
            };
            if result == 0 {
                return Ok(true);
            }
            let error = last_error();
            match error.raw_os_error() {
                Some(libc::ETIMEDOUT | libc::EAGAIN | libc::EINTR) => Ok(false),
                _ => Err(error),
            }
        }

        fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
            buf.resize(self.size, 0);
            let deadline = deadline_after(timeout);
            // SAFETY: a buffer of the queue's message size.
            let read = unsafe {
                libc::mq_timedreceive(
                    self.receive.0,
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    std::ptr::null_mut(),
                    &deadline,
                )
            };
            if read >= 0 {
                buf.truncate(read as usize);
                return Ok(true);
            }
            let error = last_error();
            match error.raw_os_error() {
                Some(libc::ETIMEDOUT | libc::EAGAIN | libc::EINTR) => Ok(false),
                _ => Err(error),
            }
        }

        fn peer_alive(&self) -> bool {
            self.peer.alive()
        }
    }

    fn link(send: Queue, receive: Queue, peer: u32) -> io::Result<Arc<dyn Datagrams>> {
        let size = receive.message_size()?;
        Ok(Arc::new(MqLink {
            send,
            receive,
            size,
            peer: PeerWatch::new(peer),
        }))
    }

    pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let rendezvous = Rendezvous::begin(endpoint, "mq")?;
        let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
        let maxmsg = endpoint.number("maxmsg", 8, 1, 65536)?;
        let (server_name, client_name) = (
            queue_name(&rendezvous.id, 's')?,
            queue_name(&rendezvous.id, 'c')?,
        );
        let unlink = || {
            // SAFETY: valid names.
            unsafe {
                libc::mq_unlink(server_name.as_ptr());
                libc::mq_unlink(client_name.as_ptr());
            }
        };
        let size = own_chunk as usize + super::super::datagram::HEADER;
        let queues = Queue::create(&server_name, maxmsg, size).and_then(|send| {
            Queue::create(&client_name, maxmsg, size).map(|receive| (send, receive))
        });
        let (send, receive) = match queues {
            Ok(queues) => queues,
            Err(error) => {
                unlink();
                return Err(io::Error::new(
                    error.kind(),
                    format!("cannot create the POSIX message queues: {error}"),
                ));
            }
        };
        let ack = rendezvous.request(Object::new().with("chunk", own_chunk));
        unlink();
        let ack = ack?;
        let chunk = agree_chunk(own_chunk, ack.number("chunk"));
        let peer = ack.number("pid").map_or(rendezvous.server_pid, |pid| pid as u32);
        let session = Session::start("mq", link(send, receive, peer)?, local_settings(endpoint, chunk)?)?;
        trace!("mq connected as {}, chunk {chunk}", rendezvous.id);
        Ok(IpcStream::new(session))
    }

    #[derive(Debug)]
    struct MqListener {
        rendezvous: RendezvousListener,
        endpoint: Endpoint,
    }

    pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        Ok(Box::new(MqListener {
            rendezvous: RendezvousListener::bind(endpoint, "mq")?,
            endpoint: endpoint.clone(),
        }))
    }

    impl Listener for MqListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            let endpoint = self.endpoint.clone();
            accept_with(&self.rendezvous, "mq", |request| {
                let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
                let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
                let receive = Queue::open(&queue_name(&request.id, 's')?)?;
                let send = Queue::open(&queue_name(&request.id, 'c')?)?;
                let session = Session::start(
                    "mq",
                    link(send, receive, request.pid())?,
                    local_settings(&endpoint, chunk)?,
                )?;
                Ok((
                    Object::new().with("chunk", chunk as u64),
                    IpcStream::new(session),
                ))
            })
        }

        fn endpoint(&self) -> String {
            self.rendezvous.endpoint()
        }

        fn closer(&self) -> Waker {
            self.rendezvous.closer()
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use posix_mq::{connect as connect_mq, listen as listen_mq};

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
mod sysv_mq {
    use super::*;

    #[cfg(target_os = "linux")]
    use libc::{msgctl, msgget, msgrcv, msgsnd};

    // The `libc` crate does not bind the System V message queue calls of
    // Apple platforms; their 64-bit symbols carry no suffix.
    #[cfg(target_vendor = "apple")]
    extern "C" {
        fn msgget(key: libc::key_t, flags: libc::c_int) -> libc::c_int;
        fn msgsnd(
            queue: libc::c_int,
            message: *const libc::c_void,
            size: libc::size_t,
            flags: libc::c_int,
        ) -> libc::c_int;
        fn msgrcv(
            queue: libc::c_int,
            message: *mut libc::c_void,
            size: libc::size_t,
            mtype: libc::c_long,
            flags: libc::c_int,
        ) -> libc::ssize_t;
        fn msgctl(queue: libc::c_int, command: libc::c_int, buffer: *mut libc::c_void) -> libc::c_int;
    }

    const MTYPE: libc::c_long = 1;
    const LONG: usize = std::mem::size_of::<libc::c_long>();
    const MAX_FRAME: usize = 65000 + super::super::datagram::HEADER;

    #[derive(Debug)]
    struct SysvLink {
        send: libc::c_int,
        receive: libc::c_int,
        poll: Duration,
        peer: PeerWatch,
        removed: AtomicBool,
    }

    fn create_queue() -> io::Result<libc::c_int> {
        // SAFETY: plain system call.
        check(unsafe { msgget(libc::IPC_PRIVATE, libc::IPC_CREAT | 0o600) })
    }

    fn remove_queue(queue: libc::c_int) {
        // SAFETY: removing a queue id; a stale id fails harmlessly.
        unsafe { msgctl(queue, libc::IPC_RMID, std::ptr::null_mut()) };
    }

    fn gone(error: io::Error) -> io::Error {
        match error.raw_os_error() {
            Some(libc::EIDRM | libc::EINVAL) => super::super::hung_up(),
            _ => error,
        }
    }

    impl Datagrams for SysvLink {
        fn send(&self, frame: &[u8]) -> io::Result<bool> {
            let mut message = vec![0u8; LONG + frame.len()];
            message[..LONG].copy_from_slice(&MTYPE.to_ne_bytes());
            message[LONG..].copy_from_slice(frame);
            // SAFETY: a `msgbuf`-shaped buffer of `frame.len()` text bytes.
            let result = unsafe {
                msgsnd(
                    self.send,
                    message.as_ptr().cast(),
                    frame.len(),
                    libc::IPC_NOWAIT,
                )
            };
            if result == 0 {
                return Ok(true);
            }
            let error = last_error();
            match error.raw_os_error() {
                Some(libc::EAGAIN | libc::EINTR) => Ok(false),
                _ => Err(gone(error)),
            }
        }

        fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
            let deadline = Instant::now() + timeout;
            let mut message = vec![0u8; LONG + MAX_FRAME];
            let mut backoff = Backoff::new(self.poll);
            loop {
                // SAFETY: a `msgbuf`-shaped buffer of `MAX_FRAME` text bytes.
                let read = unsafe {
                    msgrcv(
                        self.receive,
                        message.as_mut_ptr().cast(),
                        MAX_FRAME,
                        0,
                        libc::IPC_NOWAIT,
                    )
                };
                if read >= 0 {
                    buf.clear();
                    buf.extend_from_slice(&message[LONG..LONG + read as usize]);
                    return Ok(true);
                }
                let error = last_error();
                match error.raw_os_error() {
                    Some(libc::ENOMSG | libc::EAGAIN | libc::EINTR) => {}
                    _ => return Err(gone(error)),
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                backoff.sleep();
            }
        }

        fn peer_alive(&self) -> bool {
            self.peer.alive()
        }

        fn close(&self) {
            if !self.removed.swap(true, Ordering::SeqCst) {
                remove_queue(self.send);
                remove_queue(self.receive);
            }
        }
    }

    fn link(
        endpoint: &Endpoint,
        send: libc::c_int,
        receive: libc::c_int,
        peer: u32,
    ) -> io::Result<Arc<dyn Datagrams>> {
        Ok(Arc::new(SysvLink {
            send,
            receive,
            poll: endpoint.millis("poll", 5)?.max(Duration::from_micros(50)),
            peer: PeerWatch::new(peer),
            removed: AtomicBool::new(false),
        }))
    }

    pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let rendezvous = Rendezvous::begin(endpoint, "sysvmq")?;
        let own_chunk = endpoint.number("chunk", 1024, 16, 65000)?;
        let server_queue = create_queue()?;
        let client_queue = match create_queue() {
            Ok(queue) => queue,
            Err(error) => {
                remove_queue(server_queue);
                return Err(error);
            }
        };
        let ack = rendezvous.request(
            Object::new()
                .with("chunk", own_chunk)
                .with("server_queue", server_queue as u64)
                .with("client_queue", client_queue as u64),
        );
        let ack = match ack {
            Ok(ack) => ack,
            Err(error) => {
                remove_queue(server_queue);
                remove_queue(client_queue);
                return Err(error);
            }
        };
        let chunk = agree_chunk(own_chunk, ack.number("chunk"));
        let peer = ack.number("pid").map_or(rendezvous.server_pid, |pid| pid as u32);
        let session = Session::start(
            "sysvmq",
            link(endpoint, server_queue, client_queue, peer)?,
            local_settings(endpoint, chunk)?,
        )?;
        trace!("sysvmq connected as {}, chunk {chunk}", rendezvous.id);
        Ok(IpcStream::new(session))
    }

    #[derive(Debug)]
    struct SysvListener {
        rendezvous: RendezvousListener,
        endpoint: Endpoint,
    }

    pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        Ok(Box::new(SysvListener {
            rendezvous: RendezvousListener::bind(endpoint, "sysvmq")?,
            endpoint: endpoint.clone(),
        }))
    }

    impl Listener for SysvListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            let endpoint = self.endpoint.clone();
            accept_with(&self.rendezvous, "sysvmq", |request| {
                let own_chunk = endpoint.number("chunk", 1024, 16, 65000)?;
                let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
                let queue = |name: &str| -> io::Result<libc::c_int> {
                    let id = request.fields.require_number(name)?;
                    libc::c_int::try_from(id).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "an invalid queue id")
                    })
                };
                let (receive, send) = (queue("server_queue")?, queue("client_queue")?);
                let session = Session::start(
                    "sysvmq",
                    link(&endpoint, send, receive, request.pid())?,
                    local_settings(&endpoint, chunk)?,
                )?;
                Ok((
                    Object::new().with("chunk", chunk as u64),
                    IpcStream::new(session),
                ))
            })
        }

        fn endpoint(&self) -> String {
            self.rendezvous.endpoint()
        }

        fn closer(&self) -> Waker {
            self.rendezvous.closer()
        }
    }
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
pub(crate) use sysv_mq::{connect as connect_sysvmq, listen as listen_sysvmq};
