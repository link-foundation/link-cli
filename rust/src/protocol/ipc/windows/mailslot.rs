//! Mailslots: one write-only slot per direction, so the server and the
//! client each create the slot they read and open the one they write.

use super::*;

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
