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
            let own_chunk = endpoint.number("chunk", 1024, 16, 65000)?;
            let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
            let queue = |name: &str| -> io::Result<libc::c_int> {
                let id = request.fields.require_number(name)?;
                libc::c_int::try_from(id)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "an invalid queue id"))
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
