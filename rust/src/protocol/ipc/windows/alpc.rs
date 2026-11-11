//! Advanced Local Procedure Call (`alpc`, 64-bit Windows): the native
//! `NtAlpc*` port calls resolved from `ntdll.dll` at run time.

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

type CreatePort =
    unsafe extern "system" fn(*mut HANDLE, *const ObjectAttributes, *const PortAttributes) -> i32;
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
    trace!(
        "alpc created port clink-{id}-{side}: NTSTATUS 0x{:08X}",
        status as u32
    );
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
    trace!("alpc connecting to port clink-{id}-{side}");
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
    trace!(
        "alpc connected to port clink-{id}-{side}: NTSTATUS 0x{:08X}",
        status as u32
    );
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
        unsafe { std::slice::from_raw_parts(self.0.as_ptr().cast::<u8>().add(MESSAGE_HEADER), len) }
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
        trace!("alpc receive failed: NTSTATUS 0x{:08X}", status as u32);
        return Err(status_error("NtAlpcSendWaitReceivePort", status));
    }
    let header = buffer.header();
    if header.kind & 0xFF != LPC_DATAGRAM {
        trace!(
            "alpc received message type 0x{:04X} from process {}",
            header.kind,
            header.unique_process
        );
    }
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
    trace!(
        "alpc accepted a connection from process {}: NTSTATUS 0x{:08X}",
        header.unique_process,
        status as u32
    );
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
                if let Some(handle) = answer(&self.inbox, buffer, header, expected, self.largest)? {
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
