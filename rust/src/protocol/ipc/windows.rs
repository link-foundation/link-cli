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

mod unix_socket {
    use super::*;
    use windows_sys::Win32::Networking::WinSock::{
        accept, bind, closesocket, connect, listen, recv, send, setsockopt, shutdown,
        WSAGetLastError, WSASocketW, WSAStartup, AF_UNIX, INVALID_SOCKET, SD_BOTH, SD_RECEIVE,
        SD_SEND, SOCKADDR, SOCKADDR_UN, SOCKET, SOCKET_ERROR, SOCK_STREAM, SOL_SOCKET, SO_RCVTIMEO,
        WSADATA, WSAECONNABORTED, WSAEINTR, WSAESHUTDOWN, WSA_FLAG_NO_HANDLE_INHERIT,
        WSA_FLAG_OVERLAPPED,
    };

    fn socket_error() -> io::Error {
        // SAFETY: reads the calling thread's last socket error.
        io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
    }

    fn winsock() -> io::Result<()> {
        static STARTED: OnceLock<i32> = OnceLock::new();
        let code = *STARTED.get_or_init(|| {
            // SAFETY: an all-zero `WSADATA` is a valid output buffer.
            let mut data: WSADATA = unsafe { std::mem::zeroed() };
            // SAFETY: Winsock 2.2; never cleaned up while the process runs.
            unsafe { WSAStartup(0x0202, &mut data) }
        });
        if code == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(code))
        }
    }

    fn address(path: &str) -> io::Result<(SOCKADDR_UN, i32)> {
        let bytes = path.as_bytes();
        // SAFETY: an all-zero address is valid.
        let mut address: SOCKADDR_UN = unsafe { std::mem::zeroed() };
        if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{path}` is not a valid socket path (at most 107 bytes, no NUL)"),
            ));
        }
        address.sun_family = AF_UNIX;
        for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
            *target = *byte as i8;
        }
        let len = std::mem::size_of_val(&address.sun_family) + bytes.len() + 1;
        Ok((address, len as i32))
    }

    /// A connected `AF_UNIX` stream socket.
    #[derive(Debug)]
    pub(super) struct WinUnixStream(SOCKET);

    impl WinUnixStream {
        fn open() -> io::Result<Self> {
            winsock()?;
            // SAFETY: a plain stream socket; overlapped so receive timeouts work.
            let socket = unsafe {
                WSASocketW(
                    i32::from(AF_UNIX),
                    SOCK_STREAM,
                    0,
                    null(),
                    0,
                    WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT,
                )
            };
            if socket == INVALID_SOCKET {
                return Err(socket_error());
            }
            Ok(Self(socket))
        }

        fn connect(path: &str) -> io::Result<Self> {
            let (address, len) = address(path)?;
            let stream = Self::open()?;
            // SAFETY: a valid socket and address.
            let result = unsafe {
                connect(
                    stream.0,
                    (&address as *const SOCKADDR_UN).cast::<SOCKADDR>(),
                    len,
                )
            };
            if result == SOCKET_ERROR {
                return Err(socket_error());
            }
            Ok(stream)
        }
    }

    impl Drop for WinUnixStream {
        fn drop(&mut self) {
            // SAFETY: a socket this value owns.
            unsafe { closesocket(self.0) };
        }
    }

    impl StreamSocket for WinUnixStream {
        fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            let value: u32 = timeout.map_or(0, millis);
            // SAFETY: a valid socket and a `DWORD` option value.
            let result = unsafe {
                setsockopt(
                    self.0,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    (&value as *const u32).cast::<u8>(),
                    std::mem::size_of::<u32>() as i32,
                )
            };
            if result == SOCKET_ERROR {
                return Err(socket_error());
            }
            Ok(())
        }

        fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            let len = buf.len().min(i32::MAX as usize) as i32;
            // SAFETY: a valid socket and buffer.
            let result = unsafe { recv(self.0, buf.as_mut_ptr(), len, 0) };
            if result == SOCKET_ERROR {
                let error = socket_error();
                return match error.raw_os_error() {
                    Some(WSAESHUTDOWN | WSAECONNABORTED) => Ok(0),
                    _ => Err(error),
                };
            }
            Ok(result as usize)
        }

        fn write(&self, buf: &[u8]) -> io::Result<usize> {
            let len = buf.len().min(i32::MAX as usize) as i32;
            // SAFETY: a valid socket and buffer.
            let result = unsafe { send(self.0, buf.as_ptr(), len, 0) };
            if result == SOCKET_ERROR {
                let error = socket_error();
                return match error.raw_os_error() {
                    Some(WSAESHUTDOWN) => Err(broken_pipe()),
                    _ => Err(error),
                };
            }
            Ok(result as usize)
        }

        fn shutdown(&self, how: Shutdown) -> io::Result<()> {
            let how = match how {
                Shutdown::Read => SD_RECEIVE,
                Shutdown::Write => SD_SEND,
                Shutdown::Both => SD_BOTH,
            };
            // SAFETY: a valid socket.
            let result = unsafe { shutdown(self.0, how) };
            if how == SD_BOTH {
                // Wakes a blocked receive on another thread.
                // SAFETY: a socket is a file handle for cancellation.
                unsafe { CancelIoEx(self.0 as HANDLE, null()) };
            }
            if result == SOCKET_ERROR {
                return Err(socket_error());
            }
            Ok(())
        }
    }

    pub(crate) fn connect_unix(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let stream = WinUnixStream::connect(endpoint.address())?;
        Ok(IpcStream::new(SocketTransport::new(stream, "unix")))
    }

    #[derive(Debug)]
    struct WinUnixListener {
        socket: WinUnixStream,
        name: String,
        path: PathBuf,
        closed: Arc<AtomicBool>,
    }

    pub(crate) fn listen_unix(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        let name = endpoint.address().to_string();
        if name.starts_with('@') {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "abstract `unix:@name` sockets are Linux-only",
            ));
        }
        let (address, len) = address(&name)?;
        let path = PathBuf::from(&name);
        if path.exists() {
            if WinUnixStream::connect(&name).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("a server already listens on `{name}`"),
                ));
            }
            trace!("unix removes the stale socket `{name}`");
            std::fs::remove_file(&path)?;
        }
        let socket = WinUnixStream::open()?;
        // SAFETY: a valid socket and address.
        if unsafe {
            bind(
                socket.0,
                (&address as *const SOCKADDR_UN).cast::<SOCKADDR>(),
                len,
            )
        } == SOCKET_ERROR
        {
            return Err(socket_error());
        }
        // SAFETY: a bound socket.
        if unsafe { listen(socket.0, 128) } == SOCKET_ERROR {
            let error = socket_error();
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(Box::new(WinUnixListener {
            socket,
            name,
            path,
            closed: Arc::new(AtomicBool::new(false)),
        }))
    }

    impl Listener for WinUnixListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            loop {
                if self.closed.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                // SAFETY: a listening socket; the peer address is not needed.
                let socket = unsafe { accept(self.socket.0, null_mut(), null_mut()) };
                if socket == INVALID_SOCKET {
                    let error = socket_error();
                    if self.closed.load(Ordering::SeqCst) {
                        return Ok(None);
                    }
                    match error.raw_os_error() {
                        Some(WSAEINTR | WSAECONNABORTED) => continue,
                        _ => return Err(error),
                    }
                }
                let stream = WinUnixStream(socket);
                if self.closed.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                trace!("unix accepted on {}", self.name);
                return Ok(Some(IpcStream::new(SocketTransport::new(stream, "unix"))));
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
                    let _ = WinUnixStream::connect(&name);
                }
            })
        }
    }

    impl Drop for WinUnixListener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(crate) use unix_socket::{connect_unix, listen_unix};

// ---------------------------------------------------------------- mailslots

fn mailslot_name(id: &str, side: char) -> io::Result<Vec<u16>> {
    wide(&format!("\\\\.\\mailslot\\clink-{id}-{side}"))
}

fn create_mailslot(id: &str, side: char, size: usize) -> io::Result<Handle> {
    let name = mailslot_name(id, side)?;
    // SAFETY: a valid name; reads wait at most one slice.
    Handle::new(unsafe { CreateMailslotW(name.as_ptr(), size as u32, millis(SLICE), null()) })
}

fn open_mailslot(id: &str, side: char) -> io::Result<Handle> {
    let name = mailslot_name(id, side)?;
    // SAFETY: a valid name.
    Handle::new(unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    })
}

/// This side's mailslot and a write handle of the peer's.
#[derive(Debug)]
struct MailslotLink {
    inbox: Handle,
    outbox: Handle,
    size: usize,
    timeout: Mutex<u32>,
    peer: PeerWatch,
}

impl MailslotLink {
    fn link(inbox: Handle, outbox: Handle, size: usize, peer: u32) -> Arc<dyn Datagrams> {
        Arc::new(Self {
            inbox,
            outbox,
            size,
            timeout: Mutex::new(millis(SLICE)),
            peer: PeerWatch::new(peer),
        })
    }
}

impl Datagrams for MailslotLink {
    fn send(&self, frame: &[u8]) -> io::Result<bool> {
        let mut written = 0;
        // SAFETY: a valid handle and frame; one frame per message.
        let ok = unsafe {
            WriteFile(
                self.outbox.0,
                frame.as_ptr(),
                frame.len() as u32,
                &mut written,
                null_mut(),
            )
        };
        if ok == 0 {
            trace!("mailslot dropped a frame: {}", last_error());
            return Ok(false);
        }
        Ok(true)
    }

    fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
        let wait = millis(timeout);
        {
            let mut current = self
                .timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *current != wait {
                // SAFETY: this side's mailslot.
                if unsafe { SetMailslotInfo(self.inbox.0, wait) } == 0 {
                    return Err(last_error());
                }
                *current = wait;
            }
        }
        buf.resize(self.size, 0);
        let mut read = 0;
        // SAFETY: a buffer of the largest message size.
        let ok = unsafe {
            ReadFile(
                self.inbox.0,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        if ok == 0 {
            let code = last_code();
            if code == ERROR_SEM_TIMEOUT {
                return Ok(false);
            }
            return Err(code_error(code));
        }
        buf.truncate(read as usize);
        Ok(true)
    }

    fn peer_alive(&self) -> bool {
        self.peer.alive()
    }
}

pub(crate) fn connect_mailslot(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let rendezvous = Rendezvous::begin(endpoint, "mailslot")?;
    let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
    let size = own_chunk as usize + HEADER;
    let inbox = create_mailslot(&rendezvous.id, 'c', size)?;
    let ack = rendezvous.request(Object::new().with("chunk", own_chunk))?;
    let chunk = agree_chunk(own_chunk, ack.number("chunk"));
    let peer = ack
        .number("pid")
        .map_or(rendezvous.server_pid, |pid| pid as u32);
    let outbox = open_mailslot(&rendezvous.id, 's')?;
    let session = Session::start(
        "mailslot",
        MailslotLink::link(inbox, outbox, size, peer),
        local_settings(endpoint, chunk)?,
    )?;
    trace!("mailslot connected as {}, chunk {chunk}", rendezvous.id);
    Ok(IpcStream::new(session))
}

#[derive(Debug)]
struct MailslotListener {
    rendezvous: RendezvousListener,
    endpoint: Endpoint,
}

pub(crate) fn listen_mailslot(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(MailslotListener {
        rendezvous: RendezvousListener::bind(endpoint, "mailslot")?,
        endpoint: endpoint.clone(),
    }))
}

impl Listener for MailslotListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        let endpoint = self.endpoint.clone();
        accept_with(&self.rendezvous, "mailslot", |request| {
            let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
            let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
            let size = own_chunk as usize + HEADER;
            let outbox = open_mailslot(&request.id, 'c')?;
            let inbox = create_mailslot(&request.id, 's', size)?;
            let session = Session::start(
                "mailslot",
                MailslotLink::link(inbox, outbox, size, request.pid()),
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

// ---------------------------------------------------------------- WM_COPYDATA

mod copydata {
    use super::*;
    use windows_sys::Win32::Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, ERROR_INVALID_WINDOW_HANDLE, HWND, LPARAM, LRESULT, WPARAM,
    };
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
        PostQuitMessage, RegisterClassW, SendMessageTimeoutW, TranslateMessage, GWLP_USERDATA,
        HWND_MESSAGE, MSG, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_CLOSE, WM_COPYDATA, WM_DESTROY,
        WM_NCDESTROY, WNDCLASSW,
    };

    /// `cds.dwData` of every frame: `LIP1`.
    const MAGIC: usize = 0x4C49_5031;
    /// Frames waiting to be received before more are refused.
    const QUEUE: usize = 64;

    #[cfg(target_pointer_width = "64")]
    unsafe fn set_user_data(hwnd: HWND, value: isize) -> isize {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, value)
    }
    #[cfg(target_pointer_width = "64")]
    unsafe fn user_data(hwnd: HWND) -> isize {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    }
    #[cfg(not(target_pointer_width = "64"))]
    unsafe fn set_user_data(hwnd: HWND, value: isize) -> isize {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongW(
            hwnd,
            GWLP_USERDATA,
            value as i32,
        ) as isize
    }
    #[cfg(not(target_pointer_width = "64"))]
    unsafe fn user_data(hwnd: HWND) -> isize {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongW(hwnd, GWLP_USERDATA) as isize
    }

    /// Frames the window procedure took, for the session's receiver.
    #[derive(Debug)]
    struct Inbox {
        frames: Mutex<VecDeque<Vec<u8>>>,
        changed: Condvar,
        /// The only window whose frames are taken; 0 until the handshake.
        peer: AtomicIsize,
        largest: usize,
    }

    impl Inbox {
        fn lock(&self) -> MutexGuard<'_, VecDeque<Vec<u8>>> {
            self.frames
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }
    }

    fn class_name() -> &'static [u16] {
        static NAME: OnceLock<Vec<u16>> = OnceLock::new();
        NAME.get_or_init(|| "clink-copydata".encode_utf16().chain(Some(0)).collect())
    }

    fn register_class() -> io::Result<()> {
        static REGISTERED: OnceLock<u32> = OnceLock::new();
        let code = *REGISTERED.get_or_init(|| {
            // SAFETY: an all-zero class with a procedure, instance and name.
            let mut class: WNDCLASSW = unsafe { std::mem::zeroed() };
            class.lpfnWndProc = Some(window_procedure);
            // SAFETY: this module's instance.
            class.hInstance = unsafe { GetModuleHandleW(null()) };
            class.lpszClassName = class_name().as_ptr();
            // SAFETY: a valid class description.
            if unsafe { RegisterClassW(&class) } == 0 {
                match last_code() {
                    ERROR_CLASS_ALREADY_EXISTS => 0,
                    code => code,
                }
            } else {
                0
            }
        });
        if code == 0 {
            Ok(())
        } else {
            Err(code_error(code))
        }
    }

    unsafe extern "system" fn window_procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_COPYDATA => {
                let pointer = user_data(hwnd);
                if pointer == 0 || lparam == 0 {
                    return 0;
                }
                let inbox = &*(pointer as *const Inbox);
                let data = &*(lparam as *const COPYDATASTRUCT);
                let peer = inbox.peer.load(Ordering::SeqCst);
                if data.dwData != MAGIC
                    || peer == 0
                    || wparam as isize != peer
                    || data.cbData as usize > inbox.largest
                {
                    return 0;
                }
                let frame = if data.cbData == 0 || data.lpData.is_null() {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize)
                        .to_vec()
                };
                let mut frames = inbox.lock();
                if frames.len() >= QUEUE {
                    return 0;
                }
                frames.push_back(frame);
                inbox.changed.notify_all();
                1
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_NCDESTROY => {
                let pointer = set_user_data(hwnd, 0);
                if pointer != 0 {
                    drop(Arc::from_raw(pointer as *const Inbox));
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    /// A hidden message-only window on a message-loop thread of its own.
    #[derive(Debug)]
    struct Window {
        hwnd: isize,
        inbox: Arc<Inbox>,
        thread: Mutex<Option<thread::JoinHandle<()>>>,
    }

    impl Window {
        fn spawn(largest: usize) -> io::Result<Self> {
            register_class()?;
            let inbox = Arc::new(Inbox {
                frames: Mutex::new(VecDeque::new()),
                changed: Condvar::new(),
                peer: AtomicIsize::new(0),
                largest,
            });
            let (created, creation) = mpsc::channel::<io::Result<isize>>();
            let shared = Arc::clone(&inbox);
            let thread = thread::Builder::new()
                .name("clink-copydata".to_string())
                .spawn(move || {
                    let pointer = Arc::into_raw(shared);
                    let name = class_name().as_ptr();
                    // SAFETY: a registered class; `HWND_MESSAGE` makes the
                    // window message-only.
                    let hwnd = unsafe {
                        CreateWindowExW(
                            0,
                            name,
                            name,
                            0,
                            0,
                            0,
                            0,
                            0,
                            HWND_MESSAGE,
                            null_mut(),
                            GetModuleHandleW(null()),
                            null(),
                        )
                    };
                    if hwnd.is_null() {
                        let _ = created.send(Err(last_error()));
                        // SAFETY: the reference this thread took.
                        drop(unsafe { Arc::from_raw(pointer) });
                        return;
                    }
                    // SAFETY: the window owns the reference from now on and
                    // releases it at `WM_NCDESTROY`.
                    unsafe { set_user_data(hwnd, pointer as isize) };
                    let _ = created.send(Ok(hwnd as isize));
                    // SAFETY: an all-zero message is a valid output buffer.
                    let mut message: MSG = unsafe { std::mem::zeroed() };
                    // SAFETY: this thread's message queue.
                    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
                        // SAFETY: a message just received.
                        unsafe {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                })?;
            let hwnd = creation.recv().map_err(|_| {
                io::Error::other("the copydata window thread ended before creating its window")
            })??;
            Ok(Self {
                hwnd,
                inbox,
                thread: Mutex::new(Some(thread)),
            })
        }

        fn close(&self) {
            let thread = self
                .thread
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            if let Some(thread) = thread {
                // SAFETY: asks the window to destroy itself on its thread.
                unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
                if thread.thread().id() != thread::current().id() {
                    let _ = thread.join();
                }
            }
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            self.close();
        }
    }

    #[derive(Debug)]
    struct CopydataLink {
        window: Window,
        peer_hwnd: isize,
        peer: PeerWatch,
    }

    impl Datagrams for CopydataLink {
        fn send(&self, frame: &[u8]) -> io::Result<bool> {
            let data = COPYDATASTRUCT {
                dwData: MAGIC,
                cbData: frame.len() as u32,
                lpData: frame.as_ptr() as *mut c_void,
            };
            let mut result = 0usize;
            // SAFETY: the system copies `data` into the peer's process
            // before this call returns.
            let sent = unsafe {
                SendMessageTimeoutW(
                    self.peer_hwnd as HWND,
                    WM_COPYDATA,
                    self.window.hwnd as WPARAM,
                    &data as *const COPYDATASTRUCT as LPARAM,
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    1000,
                    &mut result,
                )
            };
            if sent == 0 {
                return match last_code() {
                    ERROR_INVALID_WINDOW_HANDLE => Err(hung_up()),
                    _ => Ok(false),
                };
            }
            Ok(result == 1)
        }

        fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
            let deadline = Instant::now() + timeout;
            let inbox = &self.window.inbox;
            let mut frames = inbox.lock();
            loop {
                if let Some(frame) = frames.pop_front() {
                    *buf = frame;
                    return Ok(true);
                }
                let now = Instant::now();
                if now >= deadline {
                    return Ok(false);
                }
                frames = inbox
                    .changed
                    .wait_timeout(frames, deadline - now)
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0;
            }
        }

        fn peer_alive(&self) -> bool {
            self.peer.alive()
        }

        fn close(&self) {
            self.window.close();
        }
    }

    fn peer_window(fields: &Object) -> io::Result<isize> {
        let hwnd = fields.require_number("hwnd")? as usize as isize;
        if hwnd == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the peer sent no window",
            ));
        }
        Ok(hwnd)
    }

    pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let rendezvous = Rendezvous::begin(endpoint, "copydata")?;
        let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
        let window = Window::spawn(own_chunk as usize + HEADER)?;
        let ack = rendezvous.request(
            Object::new()
                .with("chunk", own_chunk)
                .with("hwnd", window.hwnd as usize as u64),
        )?;
        let peer_hwnd = peer_window(&ack)?;
        window.inbox.peer.store(peer_hwnd, Ordering::SeqCst);
        let chunk = agree_chunk(own_chunk, ack.number("chunk"));
        let peer = ack
            .number("pid")
            .map_or(rendezvous.server_pid, |pid| pid as u32);
        let link = CopydataLink {
            window,
            peer_hwnd,
            peer: PeerWatch::new(peer),
        };
        let session = Session::start("copydata", Arc::new(link), local_settings(endpoint, chunk)?)?;
        trace!("copydata connected as {}, chunk {chunk}", rendezvous.id);
        Ok(IpcStream::new(session))
    }

    #[derive(Debug)]
    struct CopydataListener {
        rendezvous: RendezvousListener,
        endpoint: Endpoint,
    }

    pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        Ok(Box::new(CopydataListener {
            rendezvous: RendezvousListener::bind(endpoint, "copydata")?,
            endpoint: endpoint.clone(),
        }))
    }

    impl Listener for CopydataListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            let endpoint = self.endpoint.clone();
            accept_with(&self.rendezvous, "copydata", |request| {
                let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
                let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
                let peer_hwnd = peer_window(&request.fields)?;
                let window = Window::spawn(own_chunk as usize + HEADER)?;
                window.inbox.peer.store(peer_hwnd, Ordering::SeqCst);
                let hwnd = window.hwnd;
                let link = CopydataLink {
                    window,
                    peer_hwnd,
                    peer: PeerWatch::new(request.pid()),
                };
                let session = Session::start(
                    "copydata",
                    Arc::new(link),
                    local_settings(&endpoint, chunk)?,
                )?;
                Ok((
                    Object::new()
                        .with("chunk", chunk as u64)
                        .with("hwnd", hwnd as usize as u64),
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

pub(crate) use copydata::{connect as connect_copydata, listen as listen_copydata};

// ---------------------------------------------------------------- ALPC

#[cfg(target_pointer_width = "64")]
mod alpc {
    use super::*;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    const STATUS_SUCCESS: i32 = 0;
    const STATUS_TIMEOUT: i32 = 0x102;
    const STATUS_PORT_DISCONNECTED: i32 = 0xC000_0037_u32 as i32;
    const ALPC_MSGFLG_SYNC_REQUEST: u32 = 0x0002_0000;
    const OBJ_CASE_INSENSITIVE: u32 = 0x40;
    const LPC_DATAGRAM: u16 = 3;
    const LPC_PORT_CLOSED: u16 = 5;
    const LPC_CLIENT_DIED: u16 = 6;
    const LPC_CONNECTION_REQUEST: u16 = 10;
    /// `sizeof(PORT_MESSAGE)` on 64-bit Windows.
    const MESSAGE_HEADER: usize = 40;

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: HANDLE,
        object_name: *const UnicodeString,
        attributes: u32,
        security_descriptor: *const c_void,
        security_quality_of_service: *const c_void,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SecurityQos {
        length: u32,
        impersonation_level: i32,
        context_tracking_mode: u8,
        effective_only: u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct PortAttributes {
        flags: u32,
        security_qos: SecurityQos,
        max_message_length: usize,
        memory_bandwidth: usize,
        max_pool_usage: usize,
        max_section_size: usize,
        max_view_size: usize,
        max_total_section_size: usize,
        dup_object_types: u32,
        reserved: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct PortMessage {
        data_length: u16,
        total_length: u16,
        kind: u16,
        data_info_offset: u16,
        unique_process: usize,
        unique_thread: usize,
        message_id: u32,
        client_view_size: usize,
    }

    const _: () = assert!(std::mem::size_of::<PortMessage>() == MESSAGE_HEADER);
    const _: () = assert!(std::mem::size_of::<PortAttributes>() == 72);
    const _: () = assert!(std::mem::size_of::<SecurityQos>() == 12);

    type CreatePort = unsafe extern "system" fn(
        *mut HANDLE,
        *const ObjectAttributes,
        *const PortAttributes,
    ) -> i32;
    type ConnectPort = unsafe extern "system" fn(
        *mut HANDLE,
        *const UnicodeString,
        *const ObjectAttributes,
        *const PortAttributes,
        u32,
        *const c_void,
        *mut PortMessage,
        *mut usize,
        *mut c_void,
        *mut c_void,
        *const i64,
    ) -> i32;
    type AcceptConnectPort = unsafe extern "system" fn(
        *mut HANDLE,
        HANDLE,
        u32,
        *const ObjectAttributes,
        *const PortAttributes,
        *mut c_void,
        *mut PortMessage,
        *mut c_void,
        u8,
    ) -> i32;
    type SendWaitReceivePort = unsafe extern "system" fn(
        HANDLE,
        u32,
        *mut PortMessage,
        *mut c_void,
        *mut PortMessage,
        *mut usize,
        *mut c_void,
        *const i64,
    ) -> i32;
    type DisconnectPort = unsafe extern "system" fn(HANDLE, u32) -> i32;

    #[derive(Clone, Copy)]
    struct Ntdll {
        create: CreatePort,
        connect: ConnectPort,
        accept: AcceptConnectPort,
        send_wait_receive: SendWaitReceivePort,
        disconnect: DisconnectPort,
    }

    fn ntdll() -> io::Result<Ntdll> {
        static LOADED: OnceLock<Result<Ntdll, String>> = OnceLock::new();
        LOADED
            .get_or_init(|| {
                let module_name: Vec<u16> = "ntdll.dll".encode_utf16().chain(Some(0)).collect();
                // SAFETY: ntdll is loaded into every process.
                let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
                if module.is_null() {
                    return Err("ntdll.dll is not loaded".to_string());
                }
                let find = |name: &[u8]| {
                    // SAFETY: a NUL-terminated name.
                    unsafe { GetProcAddress(module, name.as_ptr()) }.ok_or_else(|| {
                        format!(
                            "ntdll.dll has no {}",
                            String::from_utf8_lossy(&name[..name.len() - 1])
                        )
                    })
                };
                // SAFETY: the documented native signatures of these exports.
                unsafe {
                    Ok(Ntdll {
                        create: std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            CreatePort,
                        >(find(b"NtAlpcCreatePort\0")?),
                        connect: std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            ConnectPort,
                        >(find(b"NtAlpcConnectPort\0")?),
                        accept: std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            AcceptConnectPort,
                        >(find(b"NtAlpcAcceptConnectPort\0")?),
                        send_wait_receive: std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            SendWaitReceivePort,
                        >(find(
                            b"NtAlpcSendWaitReceivePort\0",
                        )?),
                        disconnect: std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            DisconnectPort,
                        >(find(b"NtAlpcDisconnectPort\0")?),
                    })
                }
            })
            .clone()
            .map_err(|message| io::Error::new(io::ErrorKind::Unsupported, message))
    }

    fn status_error(what: &str, status: i32) -> io::Error {
        let kind = match status as u32 {
            // STATUS_OBJECT_NAME_NOT_FOUND
            0xC000_0034 => io::ErrorKind::ConnectionRefused,
            0xC000_0022 => io::ErrorKind::PermissionDenied,
            0xC000_0035 => io::ErrorKind::AlreadyExists,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(
            kind,
            format!("{what} failed with NTSTATUS 0x{:08X}", status as u32),
        )
    }

    fn port_name(id: &str, side: char) -> Vec<u16> {
        format!("\\RPC Control\\clink-{id}-{side}")
            .encode_utf16()
            .collect()
    }

    fn unicode(name: &[u16]) -> UnicodeString {
        let length = (name.len() * 2) as u16;
        UnicodeString {
            length,
            maximum_length: length,
            buffer: name.as_ptr(),
        }
    }

    fn attributes(largest: usize) -> PortAttributes {
        PortAttributes {
            flags: 0,
            security_qos: SecurityQos {
                length: std::mem::size_of::<SecurityQos>() as u32,
                // SecurityIdentification: the peer may identify, not impersonate.
                impersonation_level: 1,
                context_tracking_mode: 0,
                effective_only: 0,
            },
            max_message_length: largest + MESSAGE_HEADER,
            memory_bandwidth: 0,
            max_pool_usage: 0,
            max_section_size: 0,
            max_view_size: 0,
            max_total_section_size: 0,
            dup_object_types: 0,
            reserved: 0,
        }
    }

    /// A relative timeout in 100 ns units.
    fn relative(timeout: Duration) -> i64 {
        -((timeout.max(Duration::from_millis(1)).as_nanos() / 100).min(i64::MAX as u128) as i64)
    }

    /// Creates this side's inbox port.
    fn create_port(id: &str, side: char, largest: usize) -> io::Result<Handle> {
        let nt = ntdll()?;
        let name = port_name(id, side);
        let name = unicode(&name);
        let object = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root_directory: null_mut(),
            object_name: &name,
            attributes: OBJ_CASE_INSENSITIVE,
            security_descriptor: null(),
            security_quality_of_service: null(),
        };
        let port_attributes = attributes(largest);
        let mut port = null_mut();
        // SAFETY: valid output pointer, name and attributes.
        let status = unsafe { (nt.create)(&mut port, &object, &port_attributes) };
        if status != STATUS_SUCCESS {
            return Err(status_error("NtAlpcCreatePort", status));
        }
        Ok(Handle(port))
    }

    /// Connects to the peer's inbox port; returns once the peer accepted.
    fn connect_port(id: &str, side: char, largest: usize, timeout: Duration) -> io::Result<Handle> {
        let nt = ntdll()?;
        let name = port_name(id, side);
        let name = unicode(&name);
        let port_attributes = attributes(largest);
        let mut message = PortMessage {
            total_length: MESSAGE_HEADER as u16,
            ..PortMessage::default()
        };
        let mut length = MESSAGE_HEADER;
        let timeout = relative(timeout);
        let mut port = null_mut();
        // SAFETY: valid pointers for the duration of the call.
        let status = unsafe {
            (nt.connect)(
                &mut port,
                &name,
                null(),
                &port_attributes,
                ALPC_MSGFLG_SYNC_REQUEST,
                null(),
                &mut message,
                &mut length,
                null_mut(),
                null_mut(),
                &timeout,
            )
        };
        if status == STATUS_TIMEOUT {
            return Err(timed_out());
        }
        if status != STATUS_SUCCESS {
            return Err(status_error("NtAlpcConnectPort", status));
        }
        Ok(Handle(port))
    }

    /// A message buffer aligned for `PORT_MESSAGE`.
    struct Buffer(Vec<u64>);

    impl Buffer {
        fn new(largest: usize) -> Self {
            Self(vec![0; (MESSAGE_HEADER + largest).div_ceil(8)])
        }

        fn len(&self) -> usize {
            self.0.len() * 8
        }

        fn message(&mut self) -> *mut PortMessage {
            self.0.as_mut_ptr().cast()
        }

        fn header(&self) -> PortMessage {
            // SAFETY: the buffer is aligned and larger than a header.
            unsafe { *self.0.as_ptr().cast::<PortMessage>() }
        }

        fn data(&self) -> &[u8] {
            let header = self.header();
            let len = usize::from(header.data_length).min(self.len() - MESSAGE_HEADER);
            // SAFETY: in bounds of the buffer.
            unsafe {
                std::slice::from_raw_parts(self.0.as_ptr().cast::<u8>().add(MESSAGE_HEADER), len)
            }
        }
    }

    enum Received {
        Nothing,
        Datagram,
        Connection(PortMessage),
        Closed,
    }

    fn receive(port: &Handle, buffer: &mut Buffer, timeout: Duration) -> io::Result<Received> {
        let nt = ntdll()?;
        let mut length = buffer.len();
        let interval = relative(timeout);
        // SAFETY: a valid port and a buffer of `length` bytes.
        let status = unsafe {
            (nt.send_wait_receive)(
                port.0,
                0,
                null_mut(),
                null_mut(),
                buffer.message(),
                &mut length,
                null_mut(),
                &interval,
            )
        };
        if status == STATUS_TIMEOUT {
            return Ok(Received::Nothing);
        }
        if status != STATUS_SUCCESS {
            return Err(status_error("NtAlpcSendWaitReceivePort", status));
        }
        let header = buffer.header();
        Ok(match header.kind & 0xFF {
            LPC_DATAGRAM => Received::Datagram,
            LPC_CONNECTION_REQUEST => Received::Connection(header),
            LPC_PORT_CLOSED | LPC_CLIENT_DIED => Received::Closed,
            _ => Received::Nothing,
        })
    }

    /// Accepts the connection request in `buffer` when it comes from
    /// `expected`, otherwise refuses it.
    fn answer(
        port: &Handle,
        buffer: &mut Buffer,
        header: PortMessage,
        expected: u32,
        largest: usize,
    ) -> io::Result<Option<Handle>> {
        let nt = ntdll()?;
        let accept = header.unique_process == expected as usize;
        let port_attributes = attributes(largest);
        let mut accepted = null_mut();
        // SAFETY: the connection request just received on `port`.
        let status = unsafe {
            (nt.accept)(
                &mut accepted,
                port.0,
                0,
                null(),
                &port_attributes,
                null_mut(),
                buffer.message(),
                null_mut(),
                u8::from(accept),
            )
        };
        if !accept {
            trace!(
                "alpc refused a connection from process {}",
                header.unique_process
            );
            if !accepted.is_null() {
                drop(Handle(accepted));
            }
            return Ok(None);
        }
        if status != STATUS_SUCCESS {
            return Err(status_error("NtAlpcAcceptConnectPort", status));
        }
        Ok(Some(Handle(accepted)))
    }

    /// Receives on `inbox` until `expected` connects; `stop` gives up.
    fn accept_peer(
        inbox: &Handle,
        expected: u32,
        largest: usize,
        deadline: Instant,
        stop: &AtomicBool,
    ) -> io::Result<Handle> {
        let mut buffer = Buffer::new(largest);
        loop {
            if stop.load(Ordering::SeqCst) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "the handshake ended",
                ));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(timed_out());
            }
            if let Received::Connection(header) =
                receive(inbox, &mut buffer, (deadline - now).min(SLICE))?
            {
                if let Some(accepted) = answer(inbox, &mut buffer, header, expected, largest)? {
                    return Ok(accepted);
                }
            }
        }
    }

    #[derive(Debug)]
    struct AlpcLink {
        inbox: Arc<Handle>,
        outbox: Handle,
        /// This side's end of the peer's connection to the inbox.
        accepted: Mutex<Option<Handle>>,
        buffer: Mutex<BufferCell>,
        largest: usize,
        peer: PeerWatch,
    }

    struct BufferCell(Buffer);

    impl std::fmt::Debug for BufferCell {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("Buffer")
        }
    }

    impl Datagrams for AlpcLink {
        fn send(&self, frame: &[u8]) -> io::Result<bool> {
            let nt = ntdll()?;
            let mut message = Buffer::new(frame.len());
            // SAFETY: the buffer holds a header and `frame`.
            unsafe {
                let header = message.message();
                (*header).data_length = frame.len() as u16;
                (*header).total_length = (MESSAGE_HEADER + frame.len()) as u16;
                std::ptr::copy_nonoverlapping(
                    frame.as_ptr(),
                    header.cast::<u8>().add(MESSAGE_HEADER),
                    frame.len(),
                );
            }
            // SAFETY: a connected port and a valid message.
            let status = unsafe {
                (nt.send_wait_receive)(
                    self.outbox.0,
                    0,
                    message.message(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null(),
                )
            };
            match status {
                STATUS_SUCCESS => Ok(true),
                STATUS_PORT_DISCONNECTED => Err(hung_up()),
                _ => {
                    trace!("alpc dropped a frame: NTSTATUS 0x{:08X}", status as u32);
                    Ok(false)
                }
            }
        }

        fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
            let mut cell = self
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let buffer = &mut cell.0;
            match receive(&self.inbox, buffer, timeout)? {
                Received::Datagram => {
                    buf.clear();
                    buf.extend_from_slice(buffer.data());
                    Ok(true)
                }
                Received::Connection(header) => {
                    let mut accepted = self
                        .accepted
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let expected = if accepted.is_none() {
                        self.peer.pid()
                    } else {
                        0
                    };
                    if let Some(handle) =
                        answer(&self.inbox, buffer, header, expected, self.largest)?
                    {
                        *accepted = Some(handle);
                    }
                    Ok(false)
                }
                Received::Closed => Err(hung_up()),
                Received::Nothing => Ok(false),
            }
        }

        fn peer_alive(&self) -> bool {
            self.peer.alive()
        }

        fn close(&self) {
            if let Ok(nt) = ntdll() {
                // SAFETY: this side's connection to the peer's inbox.
                unsafe { (nt.disconnect)(self.outbox.0, 0) };
            }
            drop(
                self.accepted
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take(),
            );
        }
    }

    fn link(
        inbox: Arc<Handle>,
        outbox: Handle,
        accepted: Option<Handle>,
        largest: usize,
        peer: u32,
    ) -> Arc<dyn Datagrams> {
        Arc::new(AlpcLink {
            inbox,
            outbox,
            accepted: Mutex::new(accepted),
            buffer: Mutex::new(BufferCell(Buffer::new(largest))),
            largest,
            peer: PeerWatch::new(peer),
        })
    }

    pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
        let rendezvous = Rendezvous::begin(endpoint, "alpc")?;
        let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
        let largest = own_chunk as usize + HEADER;
        let inbox = Arc::new(create_port(&rendezvous.id, 'c', largest)?);
        let deadline = Instant::now() + endpoint.timeout();
        let stop = Arc::new(AtomicBool::new(false));
        let server_pid = rendezvous.server_pid;
        let acceptor = {
            let inbox = Arc::clone(&inbox);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("clink-alpc-accept".to_string())
                .spawn(move || accept_peer(&inbox, server_pid, largest, deadline, &stop))?
        };
        let ack = rendezvous.request(Object::new().with("chunk", own_chunk));
        let ack = match ack {
            Ok(ack) => ack,
            Err(error) => {
                stop.store(true, Ordering::SeqCst);
                let _ = acceptor.join();
                return Err(error);
            }
        };
        let accepted = acceptor
            .join()
            .map_err(|_| io::Error::other("the alpc accept thread panicked"))??;
        let chunk = agree_chunk(own_chunk, ack.number("chunk"));
        let peer = ack.number("pid").map_or(server_pid, |pid| pid as u32);
        let left = deadline.saturating_duration_since(Instant::now());
        let outbox = connect_port(&rendezvous.id, 's', largest, left)?;
        let session = Session::start(
            "alpc",
            link(inbox, outbox, Some(accepted), largest, peer),
            local_settings(endpoint, chunk)?,
        )?;
        trace!("alpc connected as {}, chunk {chunk}", rendezvous.id);
        Ok(IpcStream::new(session))
    }

    #[derive(Debug)]
    struct AlpcListener {
        rendezvous: RendezvousListener,
        endpoint: Endpoint,
    }

    pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
        ntdll()?;
        Ok(Box::new(AlpcListener {
            rendezvous: RendezvousListener::bind(endpoint, "alpc")?,
            endpoint: endpoint.clone(),
        }))
    }

    impl Listener for AlpcListener {
        fn accept(&mut self) -> io::Result<Option<IpcStream>> {
            let endpoint = self.endpoint.clone();
            accept_with(&self.rendezvous, "alpc", |request| {
                let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
                let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
                let largest = own_chunk.max(request.fields.number("chunk").unwrap_or(own_chunk))
                    as usize
                    + HEADER;
                let inbox = Arc::new(create_port(&request.id, 's', largest)?);
                let outbox = connect_port(&request.id, 'c', largest, endpoint.timeout())?;
                // The session's receiver accepts the client's connection.
                let session = Session::start(
                    "alpc",
                    link(inbox, outbox, None, largest, request.pid()),
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

#[cfg(target_pointer_width = "64")]
pub(crate) use alpc::{connect as connect_alpc, listen as listen_alpc};
