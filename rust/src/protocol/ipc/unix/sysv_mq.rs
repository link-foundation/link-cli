//! System V message queues (`sysvmq`, Linux and macOS): one private queue
//! per direction, whose ids travel in the rendezvous request.

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

/// The `struct msqid_ds` of Apple platforms (`<sys/msg.h>`, packed to 4).
#[cfg(target_vendor = "apple")]
#[repr(C, packed(4))]
#[allow(dead_code)] // only `qbytes` is read; the rest gives the layout
struct MsqidDs {
    /// `struct ipc_perm`: uid, gid, cuid, cgid, mode and seq, key.
    perm: [u32; 6],
    first: i32,
    last: i32,
    cbytes: libc::c_ulong,
    qnum: libc::c_ulong,
    qbytes: libc::c_ulong,
    lspid: libc::pid_t,
    lrpid: libc::pid_t,
    stime: libc::time_t,
    pad1: i32,
    rtime: libc::time_t,
    pad2: i32,
    ctime: libc::time_t,
    pad3: i32,
    pad4: [i32; 4],
}

#[cfg(target_os = "linux")]
type MsqidDs = libc::msqid_ds;

/// The most bytes `queue` holds (`msg_qbytes`).
fn queue_bytes(queue: libc::c_int) -> io::Result<usize> {
    // SAFETY: plain old data that `IPC_STAT` fills.
    let mut status: MsqidDs = unsafe { std::mem::zeroed() };
    // SAFETY: a valid buffer for `IPC_STAT`.
    check(unsafe { msgctl(queue, libc::IPC_STAT, (&raw mut status).cast()) })?;
    #[cfg(target_vendor = "apple")]
    let bytes = status.qbytes;
    #[cfg(target_os = "linux")]
    let bytes = status.msg_qbytes;
    Ok(usize::try_from(bytes).unwrap_or(usize::MAX))
}

/// The longest message the system accepts (`msgmax`), if it says.
fn system_message_limit() -> Option<usize> {
    if cfg!(target_os = "linux") {
        let limit = std::fs::read_to_string("/proc/sys/kernel/msgmax").ok()?;
        limit.trim().parse().ok()
    } else {
        // XNU fixes `msgmax` at 16384, above the 2048 a queue holds.
        None
    }
}

/// The largest chunk whose frame fits every one of `queues` and the system
/// limit: a longer `msgsnd` fails with `EINVAL`, and macOS queues hold only
/// 2048 bytes.
fn fitting_chunk(queues: &[libc::c_int]) -> io::Result<usize> {
    let mut largest = system_message_limit().unwrap_or(usize::MAX);
    for &queue in queues {
        largest = largest.min(queue_bytes(queue)?);
    }
    let chunk = largest.saturating_sub(super::super::datagram::HEADER);
    if chunk < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a System V message queue holds only {largest} bytes"),
        ));
    }
    Ok(chunk)
}

/// The chunk this side offers: the `chunk` option, made to fit the queues.
fn own_chunk(endpoint: &Endpoint, queues: &[libc::c_int]) -> io::Result<u64> {
    let wanted = endpoint.number("chunk", 1024, 16, 65000)?;
    let fitting = u64::try_from(fitting_chunk(queues)?).unwrap_or(u64::MAX);
    if fitting < wanted {
        trace!("sysvmq chunk {wanted} lowered to {fitting} to fit the queues");
    }
    Ok(wanted.min(fitting))
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
    let server_queue = create_queue()?;
    let client_queue = match create_queue() {
        Ok(queue) => queue,
        Err(error) => {
            remove_queue(server_queue);
            return Err(error);
        }
    };
    let own_chunk = match own_chunk(endpoint, &[server_queue, client_queue]) {
        Ok(chunk) => chunk,
        Err(error) => {
            remove_queue(server_queue);
            remove_queue(client_queue);
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
    let peer = ack
        .number("pid")
        .map_or(rendezvous.server_pid, |pid| pid as u32);
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
            let queue = |name: &str| -> io::Result<libc::c_int> {
                let id = request.fields.require_number(name)?;
                libc::c_int::try_from(id)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "an invalid queue id"))
            };
            let (receive, send) = (queue("server_queue")?, queue("client_queue")?);
            let own_chunk = own_chunk(&endpoint, &[receive, send])?;
            let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
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
