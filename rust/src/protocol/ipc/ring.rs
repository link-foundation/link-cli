//! The two byte rings shared by `shm`, `mmap`, `file` and `signal` (see
//! the "Ring region" section of `docs/protocol/ipc-transports-v1.md`).

use super::json::Object;
use super::rendezvous::{Rendezvous, RendezvousListener};
use super::{broken_pipe, hung_up, timed_out, trace, Endpoint, IpcStream, Listener, PeerWatch};
use super::{Transport, Waker};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use web_time::Instant;

pub(crate) const MAGIC: [u8; 4] = *b"LIPC";
pub(crate) const VERSION: u32 = 1;
pub(crate) const HEADER: usize = 64;
pub(crate) const RING_HEADER: usize = 128;
const HEAD: usize = 0;
const WRITER_STATE: usize = 8;
const TAIL: usize = 64;
const READER_STATE: usize = 72;
const OPEN: u32 = 0;
const ENDED: u32 = 1;
const ABORTED: u32 = 2;
const GONE: u32 = 2;

pub(crate) fn region_size(capacity: u32) -> usize {
    HEADER + 2 * (RING_HEADER + capacity as usize)
}

fn ring_base(ring: usize, capacity: u32) -> usize {
    HEADER + ring * (RING_HEADER + capacity as usize)
}

/// Bytes both processes see.
pub(crate) trait Region: Send + Sync + fmt::Debug {
    fn len(&self) -> usize;
    fn load(&self, offset: usize) -> io::Result<u32>;
    fn store(&self, offset: usize, value: u32) -> io::Result<()>;
    fn read_bytes(&self, offset: usize, buf: &mut [u8]) -> io::Result<()>;
    fn write_bytes(&self, offset: usize, data: &[u8]) -> io::Result<()>;
}

/// A shared memory mapping, released on drop.
pub(crate) struct Mapping {
    ptr: *mut u8,
    len: usize,
    release: Option<Box<dyn FnOnce() + Send>>,
}

// SAFETY: the mapping is plain shared memory; every access below goes
// through atomics or byte copies of ranges the ring protocol hands to one
// side at a time.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Mapping {
    /// # Safety
    ///
    /// `ptr` must point to `len` mapped, page-aligned bytes that stay valid
    /// until `release` runs.
    pub(crate) unsafe fn new(
        ptr: *mut u8,
        len: usize,
        release: Box<dyn FnOnce() + Send>,
    ) -> Self {
        Self {
            ptr,
            len,
            release: Some(release),
        }
    }

    fn atomic(&self, offset: usize) -> &AtomicU32 {
        assert!(offset % 4 == 0 && offset + 4 <= self.len);
        // SAFETY: in bounds, 4-aligned (the mapping is page-aligned).
        unsafe { &*(self.ptr.add(offset) as *const AtomicU32) }
    }
}

impl fmt::Debug for Mapping {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Mapping")
            .field("len", &self.len)
            .finish()
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

impl Region for Mapping {
    fn len(&self) -> usize {
        self.len
    }

    fn load(&self, offset: usize) -> io::Result<u32> {
        Ok(u32::from_le(self.atomic(offset).load(Ordering::Acquire)))
    }

    fn store(&self, offset: usize, value: u32) -> io::Result<()> {
        self.atomic(offset).store(value.to_le(), Ordering::Release);
        Ok(())
    }

    fn read_bytes(&self, offset: usize, buf: &mut [u8]) -> io::Result<()> {
        assert!(offset + buf.len() <= self.len);
        // SAFETY: in bounds; the writer does not touch these bytes until the
        // reader publishes its tail.
        unsafe { std::ptr::copy_nonoverlapping(self.ptr.add(offset), buf.as_mut_ptr(), buf.len()) };
        Ok(())
    }

    fn write_bytes(&self, offset: usize, data: &[u8]) -> io::Result<()> {
        assert!(offset + data.len() <= self.len);
        // SAFETY: as above, for the free part of the ring.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(offset), data.len()) };
        Ok(())
    }
}

/// A regular file accessed only with positioned reads and writes.
#[derive(Debug)]
pub(crate) struct FileRegion {
    file: File,
    len: usize,
}

impl FileRegion {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> io::Result<()> {
        let mut done = 0;
        while done < buf.len() {
            #[cfg(unix)]
            let read = {
                use std::os::unix::fs::FileExt;
                self.file.read_at(&mut buf[done..], (offset + done) as u64)?
            };
            #[cfg(windows)]
            let read = {
                use std::os::windows::fs::FileExt;
                self.file.seek_read(&mut buf[done..], (offset + done) as u64)?
            };
            #[cfg(not(any(unix, windows)))]
            let read: usize = {
                let _ = offset;
                return Err(super::unsupported("file", "positioned reads"));
            };
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the ring file is shorter than its header says",
                ));
            }
            done += read;
        }
        Ok(())
    }

    fn write_at(&self, offset: usize, data: &[u8]) -> io::Result<()> {
        let mut done = 0;
        while done < data.len() {
            #[cfg(unix)]
            let written = {
                use std::os::unix::fs::FileExt;
                self.file.write_at(&data[done..], (offset + done) as u64)?
            };
            #[cfg(windows)]
            let written = {
                use std::os::windows::fs::FileExt;
                self.file.seek_write(&data[done..], (offset + done) as u64)?
            };
            #[cfg(not(any(unix, windows)))]
            let written: usize = {
                let _ = offset;
                return Err(super::unsupported("file", "positioned writes"));
            };
            if written == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "the ring file is full"));
            }
            done += written;
        }
        Ok(())
    }
}

impl Region for FileRegion {
    fn len(&self) -> usize {
        self.len
    }

    fn load(&self, offset: usize) -> io::Result<u32> {
        let mut bytes = [0; 4];
        self.read_at(offset, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn store(&self, offset: usize, value: u32) -> io::Result<()> {
        self.write_at(offset, &value.to_le_bytes())
    }

    fn read_bytes(&self, offset: usize, buf: &mut [u8]) -> io::Result<()> {
        self.read_at(offset, buf)
    }

    fn write_bytes(&self, offset: usize, data: &[u8]) -> io::Result<()> {
        self.write_at(offset, data)
    }
}

/// How a ring side waits for the other one.
pub(crate) trait Notifier: Send + Sync + fmt::Debug {
    /// A value that changes on every wake-up; read before checking the ring.
    fn generation(&self) -> u64 {
        0
    }
    /// Wakes the peer after this side published something.
    fn notify(&self) {}
    /// Waits for a wake-up after `generation`, at most `limit`.
    fn wait(&self, generation: u64, attempt: u32, limit: Duration);
}

/// Polling with a backoff from busy checks to sleeps of at most `poll`.
#[derive(Debug)]
pub(crate) struct PollNotifier {
    pub(crate) poll: Duration,
}

impl Notifier for PollNotifier {
    fn wait(&self, _generation: u64, attempt: u32, limit: Duration) {
        if attempt < 64 {
            std::hint::spin_loop();
        } else if attempt < 128 {
            thread::yield_now();
        } else {
            let step = Duration::from_micros(50) * (1u32 << (attempt - 128).min(10));
            thread::sleep(step.min(self.poll).min(limit));
        }
    }
}

/// One side of a ring connection.
pub(crate) struct RingTransport {
    region: Box<dyn Region>,
    capacity: u32,
    outgoing: usize,
    incoming: usize,
    notifier: Arc<dyn Notifier>,
    peer: PeerWatch,
    kind: &'static str,
    reading: Mutex<()>,
    writing: Mutex<()>,
    closed: AtomicBool,
    // Declared last: dropped after `region`, because Windows cannot delete
    // mapped files.
    _cleanup: Cleanup,
}

/// Files removed on drop.
#[derive(Debug)]
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

impl fmt::Debug for RingTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RingTransport")
            .field("kind", &self.kind)
            .field("capacity", &self.capacity)
            .field("peer", &self.peer.pid())
            .finish_non_exhaustive()
    }
}

const PEER_CHECK: Duration = Duration::from_millis(100);

impl RingTransport {
    pub(crate) fn new(
        kind: &'static str,
        region: Box<dyn Region>,
        client: bool,
        notifier: Arc<dyn Notifier>,
        peer_pid: u32,
        cleanup: Vec<PathBuf>,
    ) -> io::Result<Self> {
        let capacity = validate(&*region)?;
        let (outgoing, incoming) = if client {
            (ring_base(0, capacity), ring_base(1, capacity))
        } else {
            (ring_base(1, capacity), ring_base(0, capacity))
        };
        Ok(Self {
            region,
            capacity,
            outgoing,
            incoming,
            notifier,
            peer: PeerWatch::new(peer_pid),
            kind,
            reading: Mutex::new(()),
            writing: Mutex::new(()),
            closed: AtomicBool::new(false),
            _cleanup: Cleanup(cleanup),
        })
    }

    fn counter(&self, offset: usize) -> io::Result<u32> {
        let mut attempt = 0u32;
        loop {
            let value = self.region.load(offset)?;
            let complement = self.region.load(offset + 4)?;
            if value == !complement {
                return Ok(value);
            }
            attempt += 1;
            if attempt > 1_000_000 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a ring counter stays torn",
                ));
            }
            if attempt > 64 {
                thread::yield_now();
            } else {
                std::hint::spin_loop();
            }
        }
    }

    fn publish(&self, offset: usize, value: u32) -> io::Result<()> {
        self.region.store(offset, value)?;
        self.region.store(offset + 4, !value)
    }

    /// `(head, tail)` of a ring, consistent with each other.
    fn positions(&self, base: usize) -> io::Result<(u32, u32)> {
        loop {
            let head = self.counter(base + HEAD)?;
            let tail = self.counter(base + TAIL)?;
            if head.wrapping_sub(tail) <= self.capacity {
                return Ok((head, tail));
            }
            thread::yield_now();
        }
    }

    fn copy_out(&self, base: usize, position: u32, buf: &mut [u8]) -> io::Result<()> {
        let data = base + RING_HEADER;
        let start = (position % self.capacity) as usize;
        let first = buf.len().min(self.capacity as usize - start);
        self.region.read_bytes(data + start, &mut buf[..first])?;
        if first < buf.len() {
            self.region.read_bytes(data, &mut buf[first..])?;
        }
        Ok(())
    }

    fn copy_in(&self, base: usize, position: u32, bytes: &[u8]) -> io::Result<()> {
        let data = base + RING_HEADER;
        let start = (position % self.capacity) as usize;
        let first = bytes.len().min(self.capacity as usize - start);
        self.region.write_bytes(data + start, &bytes[..first])?;
        if first < bytes.len() {
            self.region.write_bytes(data, &bytes[first..])?;
        }
        Ok(())
    }
}

impl Transport for RingTransport {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        let _reading = self.reading.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut attempt = 0;
        let mut last_check = Instant::now();
        loop {
            let generation = self.notifier.generation();
            let (head, tail) = self.positions(self.incoming)?;
            let used = head.wrapping_sub(tail) as usize;
            if used > 0 {
                let count = used.min(buf.len());
                self.copy_out(self.incoming, tail, &mut buf[..count])?;
                self.publish(self.incoming + TAIL, tail.wrapping_add(count as u32))?;
                self.notifier.notify();
                return Ok(count);
            }
            if self.closed.load(Ordering::SeqCst) {
                return Ok(0);
            }
            match self.region.load(self.incoming + WRITER_STATE)? {
                OPEN => {}
                ABORTED => return Err(hung_up()),
                _ => return Ok(0),
            }
            let now = Instant::now();
            if now.duration_since(last_check) >= PEER_CHECK {
                last_check = now;
                if !self.peer.alive() {
                    return Err(hung_up());
                }
            }
            let limit = match deadline {
                Some(deadline) if now >= deadline => return Err(timed_out()),
                Some(deadline) => (deadline - now).min(PEER_CHECK),
                None => PEER_CHECK,
            };
            self.notifier.wait(generation, attempt, limit);
            attempt = attempt.saturating_add(1);
        }
    }

    fn write(&self, data: &[u8]) -> io::Result<usize> {
        let _writing = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut attempt = 0;
        let mut last_check = Instant::now();
        loop {
            if self.closed.load(Ordering::SeqCst)
                || self.region.load(self.outgoing + WRITER_STATE)? != OPEN
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the outgoing direction was closed",
                ));
            }
            if self.region.load(self.outgoing + READER_STATE)? == GONE {
                return Err(broken_pipe());
            }
            let generation = self.notifier.generation();
            let (head, tail) = self.positions(self.outgoing)?;
            let free = (self.capacity - head.wrapping_sub(tail)) as usize;
            if free > 0 {
                let count = free.min(data.len());
                self.copy_in(self.outgoing, head, &data[..count])?;
                self.publish(self.outgoing + HEAD, head.wrapping_add(count as u32))?;
                self.notifier.notify();
                return Ok(count);
            }
            let now = Instant::now();
            if now.duration_since(last_check) >= PEER_CHECK {
                last_check = now;
                if !self.peer.alive() {
                    return Err(broken_pipe());
                }
            }
            self.notifier.wait(generation, attempt, PEER_CHECK);
            attempt = attempt.saturating_add(1);
        }
    }

    fn close_write(&self) -> io::Result<()> {
        let _writing = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.region.load(self.outgoing + WRITER_STATE)? == OPEN {
            self.region.store(self.outgoing + WRITER_STATE, ENDED)?;
            self.notifier.notify();
        }
        Ok(())
    }

    fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        trace!("{} ring to process {} hung up", self.kind, self.peer.pid());
        if self.region.load(self.outgoing + WRITER_STATE).ok() == Some(OPEN) {
            let _ = self.region.store(self.outgoing + WRITER_STATE, ENDED);
        }
        let _ = self.region.store(self.incoming + READER_STATE, GONE);
        self.notifier.notify();
    }

    fn kind(&self) -> &'static str {
        self.kind
    }
}

impl Drop for RingTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Checks the header and returns the capacity.
fn validate(region: &dyn Region) -> io::Result<u32> {
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidData, message.to_string());
    if region.len() < HEADER {
        return Err(invalid("the ring region is too small"));
    }
    let mut magic = [0; 4];
    region.read_bytes(0, &mut magic)?;
    if magic != MAGIC {
        return Err(invalid("the ring region lacks the `LIPC` magic"));
    }
    if region.load(4)? != VERSION {
        return Err(invalid("the ring region has another version"));
    }
    let capacity = region.load(8)?;
    if !(64..=1 << 30).contains(&capacity) || region_size(capacity) > region.len() {
        return Err(invalid("the ring region has an invalid capacity"));
    }
    Ok(capacity)
}

/// Writes the header of a fresh, zero-filled region.
pub(crate) fn initialize(region: &dyn Region, capacity: u32) -> io::Result<()> {
    region.write_bytes(0, &MAGIC)?;
    region.store(4, VERSION)?;
    region.store(8, capacity)?;
    for ring in 0..2 {
        let base = ring_base(ring, capacity);
        region.store(base + HEAD + 4, !0)?;
        region.store(base + TAIL + 4, !0)?;
    }
    Ok(())
}

fn ring_kind(endpoint: &Endpoint) -> &'static str {
    match endpoint.kind() {
        "shm" => "shm",
        "mmap" => "mmap",
        "file" => "file",
        _ => "signal",
    }
}

fn file_suffix(kind: &str) -> &'static str {
    match kind {
        "mmap" => "mmap",
        "file" => "ring",
        _ => "sig",
    }
}

fn create_file(path: &PathBuf, size: usize) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    file.set_len(size as u64)?;
    Ok(file)
}

fn open_file(path: &PathBuf) -> io::Result<(File, usize)> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let len = file.metadata()?.len() as usize;
    Ok((file, len))
}

fn file_region(kind: &str, file: File, len: usize) -> io::Result<Box<dyn Region>> {
    if kind == "file" {
        Ok(Box::new(FileRegion { file, len }))
    } else {
        Ok(Box::new(platform::map_file(&file, len)?))
    }
}

#[cfg(unix)]
use super::unix as platform;
#[cfg(windows)]
use super::windows as platform;

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    pub(crate) fn map_file(_file: &File, _len: usize) -> io::Result<Mapping> {
        Err(super::super::unsupported("mmap", "mapping files"))
    }
    pub(crate) fn create_shm(_id: &str, _len: usize) -> io::Result<Mapping> {
        Err(super::super::unsupported("shm", "shared memory"))
    }
    pub(crate) fn open_shm(_id: &str) -> io::Result<Mapping> {
        Err(super::super::unsupported("shm", "shared memory"))
    }
    pub(crate) fn unlink_shm(_id: &str) {}
    pub(crate) fn signal_client(
        _endpoint: &Endpoint,
        _id: &str,
    ) -> io::Result<(Object, SignalSetup)> {
        Err(super::super::unsupported("signal", "signals"))
    }
    pub(crate) fn signal_server(
        _endpoint: &Endpoint,
        _id: &str,
        _request: &Object,
        _peer: u32,
    ) -> io::Result<(Object, Arc<dyn Notifier>)> {
        Err(super::super::unsupported("signal", "signals"))
    }
    pub(crate) struct SignalSetup;
    impl SignalSetup {
        pub(crate) fn finish(self, _ack: &Object, _peer: u32) -> io::Result<Arc<dyn Notifier>> {
            Err(super::super::unsupported("signal", "signals"))
        }
    }
}

/// Connects to a ring kind.
pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let kind = ring_kind(endpoint);
    let rendezvous = Rendezvous::begin(endpoint, kind)?;
    let capacity = endpoint.number("capacity", 65536, 64, 1 << 30)? as u32;
    let size = region_size(capacity);
    let mut files = Vec::new();
    let region: Box<dyn Region> = if kind == "shm" {
        Box::new(platform::create_shm(&rendezvous.id, size)?)
    } else {
        let path = rendezvous.path(file_suffix(kind));
        let file = create_file(&path, size)?;
        files.push(path);
        match file_region(kind, file, size) {
            Ok(region) => region,
            Err(error) => {
                let _ = fs::remove_file(&files[0]);
                return Err(error);
            }
        }
    };
    let forget = |files: &[PathBuf]| {
        if kind == "shm" {
            platform::unlink_shm(&rendezvous.id);
        }
        for path in files {
            let _ = fs::remove_file(path);
        }
    };
    if let Err(error) = initialize(&*region, capacity) {
        drop(region);
        forget(&files);
        return Err(error);
    }
    let poll = endpoint.millis("poll", 5)?;
    let handshake = (|| {
        let (fields, signal) = if kind == "signal" {
            let (fields, setup) = platform::signal_client(endpoint, &rendezvous.id)?;
            (fields, Some(setup))
        } else {
            (Object::new(), None)
        };
        let ack = rendezvous.request(fields)?;
        let peer = ack.number("pid").unwrap_or(rendezvous.server_pid.into()) as u32;
        let notifier: Arc<dyn Notifier> = match signal {
            Some(setup) => setup.finish(&ack, peer)?,
            None => Arc::new(PollNotifier { poll }),
        };
        Ok((peer, notifier))
    })();
    let (peer, notifier) = match handshake {
        Ok(handshake) => handshake,
        Err(error) => {
            drop(region);
            forget(&files);
            return Err(error);
        }
    };
    // Both sides hold the region open now; nothing should outlive a crash.
    let cleanup = if cfg!(windows) {
        files
    } else {
        forget(&files);
        Vec::new()
    };
    let transport = RingTransport::new(kind, region, true, notifier, peer, cleanup)?;
    trace!(
        "{kind} connected as {} to process {peer}, {capacity} bytes per ring",
        rendezvous.id
    );
    Ok(IpcStream::new(transport))
}

/// Listens on a ring kind.
pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    let kind = ring_kind(endpoint);
    if kind == "shm" && !super::supports("shm", true) {
        return Err(super::unsupported("shm", "listening"));
    }
    Ok(Box::new(RingListener {
        rendezvous: RendezvousListener::bind(endpoint, kind)?,
        endpoint: endpoint.clone(),
        kind,
    }))
}

#[derive(Debug)]
struct RingListener {
    rendezvous: RendezvousListener,
    endpoint: Endpoint,
    kind: &'static str,
}

impl RingListener {
    fn open(&self, request: &super::rendezvous::Request) -> io::Result<(Box<dyn Region>, Vec<PathBuf>)> {
        if self.kind == "shm" {
            return Ok((Box::new(platform::open_shm(&request.id)?), Vec::new()));
        }
        let path = request.path(file_suffix(self.kind));
        let (file, len) = open_file(&path)?;
        let cleanup = if cfg!(windows) { vec![path] } else { Vec::new() };
        Ok((file_region(self.kind, file, len)?, cleanup))
    }
}

impl Listener for RingListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        while let Some(mut request) = self.rendezvous.next()? {
            let peer = request.pid();
            let setup = self.open(&request).and_then(|(region, cleanup)| {
                let (fields, notifier): (Object, Arc<dyn Notifier>) = if self.kind == "signal" {
                    platform::signal_server(&self.endpoint, &request.id, &request.fields, peer)?
                } else {
                    (
                        Object::new(),
                        Arc::new(PollNotifier {
                            poll: self.endpoint.millis("poll", 5)?,
                        }),
                    )
                };
                let transport =
                    RingTransport::new(self.kind, region, false, notifier, peer, cleanup)?;
                Ok((fields, transport))
            });
            match setup {
                Ok((fields, transport)) => {
                    let id = request.id.clone();
                    request.ack(fields)?;
                    trace!("{} accepted {id} from process {peer}", self.kind);
                    return Ok(Some(IpcStream::new(transport)));
                }
                Err(error) => {
                    trace!("{} refused {}: {error}", self.kind, request.id);
                    request.refuse(&error.to_string());
                }
            }
        }
        Ok(None)
    }

    fn endpoint(&self) -> String {
        self.rendezvous.endpoint()
    }

    fn closer(&self) -> Waker {
        self.rendezvous.closer()
    }
}
