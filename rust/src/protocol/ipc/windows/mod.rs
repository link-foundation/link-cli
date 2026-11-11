//! Windows mechanisms: process liveness, file mappings and named shared
//! memory, events, named pipes, `AF_UNIX` sockets, mailslots, `WM_COPYDATA`
//! and ALPC.

use super::datagram::{agree_chunk, local_settings, Datagrams, Session, HEADER};
use super::json::Object;
use super::net::{SocketTransport, StreamSocket};
use super::rendezvous::{accept_with, Rendezvous, RendezvousListener};
use super::ring::{Mapping, Notifier};
use super::{
    broken_pipe, hung_up, timed_out, trace, Endpoint, IpcStream, Listener, PeerWatch, Transport,
    Waker,
};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::net::Shutdown;
use std::os::windows::io::{AsRawHandle, IntoRawHandle};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;
use web_time::Instant;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BROKEN_PIPE,
    ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_OPERATION_ABORTED,
    ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT, HANDLE,
    INVALID_HANDLE_VALUE, STILL_ACTIVE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE,
    FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, SYNCHRONIZE,
};
use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Mailslots::{CreateMailslotW, SetMailslotInfo};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery,
    FILE_MAP_ALL_ACCESS, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetExitCodeProcess, OpenEventW, OpenProcess, SetEvent, WaitForSingleObject,
    EVENT_MODIFY_STATE, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
/// How long one blocking wait lasts before closing is checked again.
const SLICE: Duration = Duration::from_millis(100);

fn last_code() -> u32 {
    // SAFETY: reads the calling thread's last error.
    unsafe { GetLastError() }
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn code_error(code: u32) -> io::Error {
    io::Error::from_raw_os_error(code as i32)
}

fn wide(text: &str) -> io::Result<Vec<u16>> {
    if text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a name must not contain NUL characters",
        ));
    }
    Ok(text.encode_utf16().chain(Some(0)).collect())
}

fn millis(duration: Duration) -> u32 {
    duration.as_millis().clamp(1, u128::from(u32::MAX - 1)) as u32
}

/// An owned kernel handle.
#[derive(Debug)]
struct Handle(HANDLE);

// SAFETY: kernel handles may be used and closed from any thread.
unsafe impl Send for Handle {}
// SAFETY: as above; the operations used on shared handles are thread-safe.
unsafe impl Sync for Handle {}

impl Handle {
    /// Takes ownership of `raw`, or returns the last error when it is invalid.
    fn new(raw: HANDLE) -> io::Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(last_error())
        } else {
            Ok(Self(raw))
        }
    }

    /// An unnamed manual-reset event for overlapped I/O.
    fn event() -> io::Result<Self> {
        // SAFETY: no attributes and no name.
        Self::new(unsafe { CreateEventW(null(), 1, 0, null()) })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle this value owns.
        unsafe { CloseHandle(self.0) };
    }
}

/// Whether the process `pid` is still running.
pub(crate) fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: opening a process for a query only.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        // A process of another user is still a running process.
        return last_code() == ERROR_ACCESS_DENIED;
    }
    let process = Handle(process);
    let mut code = 0u32;
    // SAFETY: a valid process handle and output pointer.
    let queried = unsafe { GetExitCodeProcess(process.0, &mut code) };
    queried == 0 || code == STILL_ACTIVE as u32
}

/// Replaces standard output with `NUL` and closes the original handle, so
/// the reader of the original one sees its end.
pub(crate) fn close_stdout() -> io::Result<()> {
    let null_device = File::options().write(true).open("NUL")?.into_raw_handle();
    // SAFETY: swapping in a handle this function owns; the old one is closed
    // only after it was replaced.
    unsafe {
        let old = GetStdHandle(STD_OUTPUT_HANDLE);
        if SetStdHandle(STD_OUTPUT_HANDLE, null_device) == 0 {
            let error = last_error();
            CloseHandle(null_device);
            return Err(error);
        }
        if !old.is_null() && old != INVALID_HANDLE_VALUE {
            CloseHandle(old);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- mappings

/// Maps a view of `mapping`; `len` 0 maps the whole section. The mapping
/// handle stays open until the view is released.
fn map_view(mapping: Handle, len: usize) -> io::Result<Mapping> {
    // SAFETY: a valid section handle.
    let view = unsafe { MapViewOfFile(mapping.0, FILE_MAP_ALL_ACCESS, 0, 0, len) };
    if view.Value.is_null() {
        return Err(last_error());
    }
    let address = view.Value as usize;
    let unmap = move || {
        // SAFETY: the view this mapping owns, unmapped once.
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: address as *mut c_void,
            })
        };
    };
    let len = if len == 0 {
        // SAFETY: an all-zero structure is a valid output buffer.
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: querying the view just mapped.
        let written = unsafe {
            VirtualQuery(
                view.Value,
                &mut info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if written == 0 {
            let error = last_error();
            unmap();
            return Err(error);
        }
        info.RegionSize
    } else {
        len
    };
    let release: Box<dyn FnOnce() + Send> = Box::new(move || {
        unmap();
        drop(mapping);
    });
    // SAFETY: the view is valid for `len` bytes until the release unmaps it.
    Ok(unsafe { Mapping::new(address as *mut u8, len, release) })
}

/// Maps a whole regular file.
pub(crate) fn map_file(file: &File, len: usize) -> io::Result<Mapping> {
    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot map an empty region",
        ));
    }
    // SAFETY: a valid file handle; size 0 maps the file's current size.
    let mapping = Handle::new(unsafe {
        CreateFileMappingW(file.as_raw_handle(), null(), PAGE_READWRITE, 0, 0, null())
    })?;
    map_view(mapping, len)
}

fn shm_name(id: &str) -> io::Result<Vec<u16>> {
    wide(&format!("Local\\clink-{id}"))
}

/// Creates the named section `Local\clink-ID` backed by the paging file.
pub(crate) fn create_shm(id: &str, len: usize) -> io::Result<Mapping> {
    let name = shm_name(id)?;
    let size = len as u64;
    // SAFETY: a valid name; `INVALID_HANDLE_VALUE` selects the paging file.
    let raw = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            null(),
            PAGE_READWRITE,
            (size >> 32) as u32,
            size as u32,
            name.as_ptr(),
        )
    };
    let code = last_code();
    let mapping = Handle::new(raw)?;
    if code == ERROR_ALREADY_EXISTS {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("the shared memory `Local\\clink-{id}` already exists"),
        ));
    }
    map_view(mapping, len)
}

/// Opens the named section `Local\clink-ID`.
pub(crate) fn open_shm(id: &str) -> io::Result<Mapping> {
    let name = shm_name(id)?;
    // SAFETY: a valid name.
    let mapping = Handle::new(unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) })?;
    map_view(mapping, 0)
}

/// Named sections vanish with their last handle; nothing to remove.
pub(crate) fn unlink_shm(_id: &str) {}

// ---------------------------------------------------------------- events

#[derive(Debug, Default)]
struct EventWake {
    generation: AtomicU64,
    lock: Mutex<()>,
    changed: Condvar,
    stop: AtomicBool,
}

/// Wakes the peer by setting its auto-reset event; a thread turns this
/// side's event into a generation every waiter sees.
#[derive(Debug)]
pub(crate) struct EventNotifier {
    peer: Handle,
    wake: Arc<EventWake>,
}

impl EventNotifier {
    fn start(own: Handle, peer: Handle) -> io::Result<Arc<dyn Notifier>> {
        let wake = Arc::new(EventWake::default());
        let shared = Arc::clone(&wake);
        thread::Builder::new()
            .name("clink-event".to_string())
            .spawn(move || {
                // Moves the whole handle in, not just its raw pointer.
                let own = own;
                while !shared.stop.load(Ordering::SeqCst) {
                    // SAFETY: an event handle this thread owns.
                    if unsafe { WaitForSingleObject(own.0, millis(SLICE)) } == WAIT_OBJECT_0 {
                        let _guard = shared
                            .lock
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        shared.generation.fetch_add(1, Ordering::SeqCst);
                        shared.changed.notify_all();
                    }
                }
            })?;
        Ok(Arc::new(Self { peer, wake }))
    }
}

impl Notifier for EventNotifier {
    fn generation(&self) -> u64 {
        self.wake.generation.load(Ordering::SeqCst)
    }

    fn notify(&self) {
        // SAFETY: the peer's event, opened with `EVENT_MODIFY_STATE`.
        unsafe { SetEvent(self.peer.0) };
    }

    fn wait(&self, generation: u64, attempt: u32, limit: Duration) {
        if attempt < 16 {
            std::hint::spin_loop();
            return;
        }
        let guard = self
            .wake
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.wake.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = self.wake.changed.wait_timeout(guard, limit);
    }
}

impl Drop for EventNotifier {
    fn drop(&mut self) {
        self.wake.stop.store(true, Ordering::SeqCst);
    }
}

fn event_name(id: &str, side: char) -> io::Result<Vec<u16>> {
    wide(&format!("Local\\clink-{id}-{side}"))
}

fn create_event(id: &str, side: char) -> io::Result<Handle> {
    let name = event_name(id, side)?;
    // SAFETY: an auto-reset, initially unset event with a valid name.
    let raw = unsafe { CreateEventW(null(), 0, 0, name.as_ptr()) };
    let code = last_code();
    let event = Handle::new(raw)?;
    if code == ERROR_ALREADY_EXISTS {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("the event `Local\\clink-{id}-{side}` already exists"),
        ));
    }
    Ok(event)
}

fn open_event(id: &str, side: char) -> io::Result<Handle> {
    let name = event_name(id, side)?;
    // SAFETY: a valid name.
    Handle::new(unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE, 0, name.as_ptr()) })
}

/// The client's events, created before the request.
pub(crate) struct SignalSetup {
    server: Handle,
    client: Handle,
}

impl SignalSetup {
    pub(crate) fn finish(self, _ack: &Object, _peer: u32) -> io::Result<Arc<dyn Notifier>> {
        EventNotifier::start(self.client, self.server)
    }
}

pub(crate) fn signal_client(_endpoint: &Endpoint, id: &str) -> io::Result<(Object, SignalSetup)> {
    let server = create_event(id, 's')?;
    let client = create_event(id, 'c')?;
    Ok((Object::new(), SignalSetup { server, client }))
}

pub(crate) fn signal_server(
    _endpoint: &Endpoint,
    id: &str,
    _request: &Object,
    _peer: u32,
) -> io::Result<(Object, Arc<dyn Notifier>)> {
    let own = open_event(id, 's')?;
    let peer = open_event(id, 'c')?;
    Ok((Object::new(), EventNotifier::start(own, peer)?))
}

// ---------------------------------------------------------------- overlapped I/O

/// Cancels the operation of `overlapped` and waits until it ended, so its
/// buffer and structure may go; returns what it transferred anyway.
fn cancel(handle: HANDLE, overlapped: &OVERLAPPED) -> u32 {
    let mut transferred = 0;
    // SAFETY: the pending operation of `overlapped` on `handle`.
    unsafe {
        CancelIoEx(handle, overlapped);
        if GetOverlappedResult(handle, overlapped, &mut transferred, 1) == 0 {
            transferred = 0;
        }
    }
    transferred
}

/// Runs one overlapped operation, waiting in slices that check `closed` and
/// `deadline`.
fn overlapped_io(
    handle: HANDLE,
    event: &Handle,
    deadline: Option<Instant>,
    closed: &AtomicBool,
    start: impl FnOnce(*mut OVERLAPPED) -> i32,
) -> io::Result<u32> {
    // SAFETY: an all-zero `OVERLAPPED` is valid.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = event.0;
    if start(&mut overlapped) == 0 {
        let code = last_code();
        if code != ERROR_IO_PENDING {
            return Err(code_error(code));
        }
        loop {
            let slice = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return match cancel(handle, &overlapped) {
                            0 => Err(timed_out()),
                            done => Ok(done),
                        };
                    }
                    (deadline - now).min(SLICE)
                }
                None => SLICE,
            };
            if closed.load(Ordering::SeqCst) {
                cancel(handle, &overlapped);
                return Err(code_error(ERROR_OPERATION_ABORTED));
            }
            // SAFETY: the event of the pending operation.
            match unsafe { WaitForSingleObject(event.0, millis(slice)) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => {}
                _ => {
                    let error = last_error();
                    cancel(handle, &overlapped);
                    return Err(error);
                }
            }
        }
    }
    let mut transferred = 0;
    // SAFETY: the completed operation of `overlapped`.
    if unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, 0) } == 0 {
        return Err(last_error());
    }
    Ok(transferred)
}

fn ends_pipe(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(
            ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA | ERROR_OPERATION_ABORTED
        )
    )
}

// ---------------------------------------------------------------- named pipes

fn pipe_name(address: &str) -> io::Result<String> {
    if address.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "`pipe:` needs a name",
        ));
    }
    Ok(if address.starts_with("\\\\") {
        address.to_string()
    } else {
        format!("\\\\.\\pipe\\{address}")
    })
}

/// One end of a named pipe instance. A named pipe cannot half-close, so
/// ending the outgoing direction closes the handle.
#[derive(Debug)]
struct PipeTransport {
    handle: Mutex<Option<Arc<Handle>>>,
    read_event: Mutex<Handle>,
    write_event: Mutex<Handle>,
    closed: AtomicBool,
}

impl PipeTransport {
    fn new(handle: Handle) -> io::Result<Self> {
        Ok(Self {
            handle: Mutex::new(Some(Arc::new(handle))),
            read_event: Mutex::new(Handle::event()?),
            write_event: Mutex::new(Handle::event()?),
            closed: AtomicBool::new(false),
        })
    }

    fn slot(&self) -> MutexGuard<'_, Option<Arc<Handle>>> {
        self.handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn current(&self) -> Option<Arc<Handle>> {
        self.slot().clone()
    }

    fn close(&self) {
        if let Some(handle) = self.slot().take() {
            // SAFETY: cancels this side's pending operations on the pipe;
            // whatever was written stays readable by the peer.
            unsafe { CancelIoEx(handle.0, null()) };
            trace!("pipe closed");
        }
    }
}

impl Transport for PipeTransport {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let event = self
            .read_event
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            let Some(handle) = self.current() else {
                return Ok(0);
            };
            let len = buf.len().min(u32::MAX as usize) as u32;
            let pointer = buf.as_mut_ptr();
            let result = overlapped_io(handle.0, &event, deadline, &self.closed, |overlapped| {
                // SAFETY: `buf` outlives the operation, which ends before
                // `overlapped_io` returns.
                unsafe { ReadFile(handle.0, pointer, len, null_mut(), overlapped) }
            });
            match result {
                // A zero-length write of the peer.
                Ok(0) => {}
                Ok(count) => return Ok(count as usize),
                Err(error) if ends_pipe(&error) => return Ok(0),
                Err(error) => return Err(error),
            }
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let event = self
            .write_event
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(handle) = self.current() else {
            return Err(broken_pipe());
        };
        let len = buf.len().min(u32::MAX as usize) as u32;
        let result = overlapped_io(handle.0, &event, None, &self.closed, |overlapped| {
            // SAFETY: `buf` outlives the operation.
            unsafe { WriteFile(handle.0, buf.as_ptr(), len, null_mut(), overlapped) }
        });
        match result {
            Ok(count) => Ok(count as usize),
            Err(error) if ends_pipe(&error) => Err(broken_pipe()),
            Err(error) => Err(error),
        }
    }

    fn close_write(&self) -> io::Result<()> {
        self.close();
        Ok(())
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.close();
    }

    fn kind(&self) -> &'static str {
        "pipe"
    }
}

impl Drop for PipeTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn create_instance(name: &[u16], first: bool) -> io::Result<Handle> {
    let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: a valid name and default security.
    Handle::new(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            null(),
        )
    })
}

pub(crate) fn connect_pipe(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let name = pipe_name(endpoint.address())?;
    let wide_name = wide(&name)?;
    let deadline = Instant::now() + endpoint.timeout();
    loop {
        // SAFETY: a valid name; the server may only identify this client.
        let raw = unsafe {
            CreateFileW(
                wide_name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        };
        if raw != INVALID_HANDLE_VALUE {
            trace!("pipe connected to {name}");
            return Ok(IpcStream::new(PipeTransport::new(Handle(raw))?));
        }
        match last_code() {
            ERROR_PIPE_BUSY => {
                let now = Instant::now();
                if now >= deadline {
                    return Err(timed_out());
                }
                // SAFETY: a valid name.
                unsafe {
                    WaitNamedPipeW(wide_name.as_ptr(), millis((deadline - now).min(SLICE * 10)))
                };
            }
            ERROR_FILE_NOT_FOUND => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("nobody listens on `{name}`"),
                ))
            }
            code => return Err(code_error(code)),
        }
    }
}

#[derive(Debug)]
struct PipeListener {
    address: String,
    name: Vec<u16>,
    next: Option<Handle>,
    event: Arc<Handle>,
    closed: Arc<AtomicBool>,
}

pub(crate) fn listen_pipe(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    let full = pipe_name(endpoint.address())?;
    let name = wide(&full)?;
    let first = create_instance(&name, true).map_err(|error| {
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
            io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("a server already listens on `{full}`"),
            )
        } else {
            error
        }
    })?;
    Ok(Box::new(PipeListener {
        address: endpoint.address().to_string(),
        name,
        next: Some(first),
        event: Arc::new(Handle::event()?),
        closed: Arc::new(AtomicBool::new(false)),
    }))
}

impl PipeListener {
    /// Waits for a client on `instance`; `Ok(false)` when closed.
    fn wait_for_client(&self, instance: &Handle) -> io::Result<bool> {
        // SAFETY: an all-zero `OVERLAPPED` is valid.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = self.event.0;
        // SAFETY: a fresh instance and its own event.
        if unsafe { ConnectNamedPipe(instance.0, &mut overlapped) } != 0 {
            return Ok(true);
        }
        match last_code() {
            ERROR_PIPE_CONNECTED => return Ok(true),
            // The client came and went before this call.
            ERROR_NO_DATA => return Ok(false),
            ERROR_IO_PENDING => {}
            code => return Err(code_error(code)),
        }
        loop {
            // SAFETY: the event of the pending connection.
            let waited = unsafe { WaitForSingleObject(self.event.0, millis(SLICE)) };
            if self.closed.load(Ordering::SeqCst) {
                cancel(instance.0, &overlapped);
                return Ok(false);
            }
            match waited {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => {}
                _ => {
                    let error = last_error();
                    cancel(instance.0, &overlapped);
                    return Err(error);
                }
            }
        }
        let mut transferred = 0;
        // SAFETY: the completed connection of `overlapped`.
        if unsafe { GetOverlappedResult(instance.0, &overlapped, &mut transferred, 0) } == 0 {
            let error = last_error();
            if ends_pipe(&error) {
                return Ok(false);
            }
            return Err(error);
        }
        Ok(true)
    }
}

impl Listener for PipeListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let instance = match self.next.take() {
                Some(instance) => instance,
                None => create_instance(&self.name, false)?,
            };
            if !self.wait_for_client(&instance)? {
                continue;
            }
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            // The next client finds an instance waiting.
            self.next = create_instance(&self.name, false).ok();
            trace!("pipe accepted on {}", self.address);
            return Ok(Some(IpcStream::new(PipeTransport::new(instance)?)));
        }
    }

    fn endpoint(&self) -> String {
        format!("pipe:{}", self.address)
    }

    fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        let event = Arc::clone(&self.event);
        Arc::new(move || {
            closed.store(true, Ordering::SeqCst);
            // SAFETY: the listener's event, kept alive by this closure.
            unsafe { SetEvent(event.0) };
        })
    }
}

// ---------------------------------------------------------------- AF_UNIX

mod unix_socket;

pub(crate) use unix_socket::{connect_unix, listen_unix};

// ---------------------------------------------------------------- mailslots

mod mailslot;

pub(crate) use mailslot::{connect_mailslot, listen_mailslot};

// ---------------------------------------------------------------- WM_COPYDATA

mod copydata;

pub(crate) use copydata::{connect as connect_copydata, listen as listen_copydata};

// ---------------------------------------------------------------- ALPC

#[cfg(target_pointer_width = "64")]
mod alpc;

#[cfg(target_pointer_width = "64")]
pub(crate) use alpc::{connect as connect_alpc, listen as listen_alpc};
