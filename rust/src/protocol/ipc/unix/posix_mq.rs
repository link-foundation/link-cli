//! POSIX message queues (`mq`, Linux): one queue per direction, named
//! `/lcIDs` and `/lcIDc`.

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
        tv_sec: now.tv_sec
            + timeout.as_secs() as libc::time_t
            + (nanos / 1_000_000_000) as libc::time_t,
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
            libc::mq_timedsend(
                self.send.0,
                frame.as_ptr().cast(),
                frame.len(),
                0,
                &deadline,
            )
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
    let queues = Queue::create(&server_name, maxmsg, size)
        .and_then(|send| Queue::create(&client_name, maxmsg, size).map(|receive| (send, receive)));
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
    let peer = ack
        .number("pid")
        .map_or(rendezvous.server_pid, |pid| pid as u32);
    let session = Session::start(
        "mq",
        link(send, receive, peer)?,
        local_settings(endpoint, chunk)?,
    )?;
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
