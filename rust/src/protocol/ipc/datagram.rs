//! The reliable datagram session of `udp`, `mq`, `sysvmq`, `mailslot`,
//! `copydata` and `alpc` (see the "Reliable datagram session" section of
//! `docs/protocol/ipc-transports-v1.md`).

use super::{broken_pipe, hung_up, timed_out, trace, Transport};
use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use web_time::Instant;

pub(crate) const HEADER: usize = 8;
pub(crate) const DATA: u8 = 1;
pub(crate) const ACK: u8 = 2;
pub(crate) const EOF: u8 = 3;
pub(crate) const PING: u8 = 4;
pub(crate) const ABORT: u8 = 5;
pub(crate) const HELLO: u8 = 6;
pub(crate) const WELCOME: u8 = 7;

/// At most this many received bytes wait to be read.
const WINDOW: usize = 1 << 20;
const MAX_RTO: Duration = Duration::from_millis(1000);
const TICK: Duration = Duration::from_millis(100);
/// How long hanging up waits for the end-of-stream frame to be acknowledged.
const LINGER: Duration = Duration::from_millis(1000);

/// Builds one frame.
pub(crate) fn frame(kind: u8, seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER + payload.len());
    frame.extend_from_slice(&[kind, 0, 0, 0]);
    frame.extend_from_slice(&seq.to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Splits a frame into its type, sequence number and payload.
pub(crate) fn parse(frame: &[u8]) -> Option<(u8, u32, &[u8])> {
    if frame.len() < HEADER {
        return None;
    }
    let seq = u32::from_le_bytes(frame[4..8].try_into().expect("four bytes"));
    Some((frame[0], seq, &frame[HEADER..]))
}

/// A mechanism that moves whole frames, possibly losing some.
pub(crate) trait Datagrams: Send + Sync + fmt::Debug {
    /// Sends one frame; `Ok(false)` when it was dropped (a full queue).
    fn send(&self, frame: &[u8]) -> io::Result<bool>;
    /// Receives one frame into `buf`, waiting up to `timeout`; `Ok(false)`
    /// when none arrived in time.
    fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool>;
    /// Whether the peer may still answer (process liveness for local kinds).
    fn peer_alive(&self) -> bool {
        true
    }
    /// Releases the mechanism; called once, after the session stopped.
    fn close(&self) {}
}

/// Session settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    pub(crate) chunk: usize,
    pub(crate) rto: Duration,
    /// Send `PING` after this much silence (`udp`).
    pub(crate) heartbeat: Option<Duration>,
    /// The peer is gone after this much silence (`udp`).
    pub(crate) dead: Option<Duration>,
}

#[derive(Debug)]
struct State {
    inbound: VecDeque<u8>,
    /// The last accepted incoming sequence number.
    received: u32,
    /// The incoming direction ended.
    ended: bool,
    /// The peer hung up or is gone.
    aborted: bool,
    /// The last outgoing sequence number used.
    sent: u32,
    /// The highest acknowledged outgoing sequence number.
    acknowledged: u32,
    /// The outgoing direction ended.
    write_closed: bool,
    /// This side hung up.
    closed: bool,
    /// The receiver thread should stop.
    stopping: bool,
    last_heard: Instant,
    last_sent: Instant,
}

#[derive(Debug)]
struct Shared {
    kind: &'static str,
    link: Arc<dyn Datagrams>,
    settings: Settings,
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn send(&self, frame: &[u8]) -> io::Result<bool> {
        let sent = self.link.send(frame);
        self.lock().last_sent = Instant::now();
        sent
    }

    fn receive_loop(self: Arc<Self>) {
        let mut buffer = Vec::with_capacity(self.settings.chunk + HEADER);
        let tick = match self.settings.heartbeat {
            Some(heartbeat) => TICK.min(heartbeat / 2).max(Duration::from_millis(1)),
            None => TICK,
        };
        let mut last_check = Instant::now();
        loop {
            if self.lock().stopping {
                break;
            }
            match self.link.recv(&mut buffer, tick) {
                Ok(true) => self.handle(&buffer),
                Ok(false) => {}
                Err(error) => {
                    trace!("{} receive failed: {error}", self.kind);
                    self.hang_up();
                    thread::sleep(tick);
                }
            }
            let now = Instant::now();
            if now.duration_since(last_check) >= tick {
                last_check = now;
                self.housekeeping(now);
            }
        }
        self.link.close();
        trace!("{} session stopped", self.kind);
    }

    fn handle(&self, frame: &[u8]) {
        let Some((kind, seq, payload)) = parse(frame) else {
            return;
        };
        let reply = {
            let mut state = self.lock();
            state.last_heard = Instant::now();
            match kind {
                DATA | EOF => {
                    if seq == state.received.wrapping_add(1)
                        && !state.ended
                        && state.inbound.len() <= WINDOW
                        && payload.len() <= self.settings.chunk
                    {
                        state.received = seq;
                        if kind == EOF {
                            state.ended = true;
                        } else {
                            state.inbound.extend(payload);
                        }
                        self.changed.notify_all();
                        Some(state.received)
                    } else if seq.wrapping_sub(state.received) > u32::MAX / 2
                        || seq == state.received
                    {
                        // A retransmission of something already accepted.
                        Some(state.received)
                    } else {
                        None
                    }
                }
                ACK => {
                    if seq.wrapping_sub(state.acknowledged) <= u32::MAX / 2 {
                        state.acknowledged = seq;
                        self.changed.notify_all();
                    }
                    None
                }
                ABORT => {
                    if !state.aborted {
                        trace!("{} peer hung up", self.kind);
                    }
                    state.aborted = true;
                    self.changed.notify_all();
                    None
                }
                _ => None,
            }
        };
        if let Some(seq) = reply {
            let _ = self.send(&frame_ack(seq));
        }
    }

    fn housekeeping(&self, now: Instant) {
        let (ping, silent) = {
            let state = self.lock();
            if state.aborted || state.stopping {
                return;
            }
            let ping = self
                .settings
                .heartbeat
                .is_some_and(|heartbeat| now.duration_since(state.last_sent) >= heartbeat);
            let silent = self
                .settings
                .dead
                .is_some_and(|dead| now.duration_since(state.last_heard) >= dead);
            (ping, silent)
        };
        if silent || !self.link.peer_alive() {
            trace!("{} peer is gone", self.kind);
            self.hang_up();
        } else if ping {
            let _ = self.send(&frame(PING, 0, &[]));
        }
    }

    fn hang_up(&self) {
        let mut state = self.lock();
        state.aborted = true;
        self.changed.notify_all();
    }

    /// Sends a DATA or EOF frame and waits for its acknowledgement.
    fn send_reliably(&self, kind: u8, payload: &[u8], linger: Option<Instant>) -> io::Result<()> {
        let seq = {
            let mut state = self.lock();
            state.sent = state.sent.wrapping_add(1);
            state.sent
        };
        let frame = frame(kind, seq, payload);
        let mut rto = self.settings.rto.max(Duration::from_millis(1));
        loop {
            self.send(&frame)?;
            let deadline = Instant::now() + rto;
            let mut state = self.lock();
            loop {
                if state.acknowledged.wrapping_sub(seq) <= u32::MAX / 2 {
                    return Ok(());
                }
                if state.aborted {
                    return Err(broken_pipe());
                }
                if state.closed && linger.is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "the connection was closed",
                    ));
                }
                let now = Instant::now();
                if linger.is_some_and(|linger| now >= linger) {
                    return Err(timed_out());
                }
                if now >= deadline {
                    break;
                }
                state = self
                    .changed
                    .wait_timeout(state, deadline - now)
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0;
            }
            drop(state);
            rto = (rto * 2).min(MAX_RTO);
        }
    }
}

fn frame_ack(seq: u32) -> Vec<u8> {
    frame(ACK, seq, &[])
}

/// A reliable byte stream over [`Datagrams`].
pub(crate) struct Session {
    shared: Arc<Shared>,
    sending: Mutex<()>,
}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("kind", &self.shared.kind)
            .field("link", &self.shared.link)
            .field("settings", &self.shared.settings)
            .finish()
    }
}

impl Session {
    pub(crate) fn start(
        kind: &'static str,
        link: Arc<dyn Datagrams>,
        settings: Settings,
    ) -> io::Result<Self> {
        let now = Instant::now();
        let shared = Arc::new(Shared {
            kind,
            link,
            settings,
            state: Mutex::new(State {
                inbound: VecDeque::new(),
                received: 0,
                ended: false,
                aborted: false,
                sent: 0,
                acknowledged: 0,
                write_closed: false,
                closed: false,
                stopping: false,
                last_heard: now,
                last_sent: now,
            }),
            changed: Condvar::new(),
        });
        let receiver = Arc::clone(&shared);
        thread::Builder::new()
            .name(format!("clink-{kind}-session"))
            .spawn(move || receiver.receive_loop())?;
        trace!("{kind} session started, chunk {}", settings.chunk);
        Ok(Self {
            shared,
            sending: Mutex::new(()),
        })
    }

    fn sending(&self) -> MutexGuard<'_, ()> {
        self.sending.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Transport for Session {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut state = self.shared.lock();
        loop {
            if !state.inbound.is_empty() {
                let count = buf.len().min(state.inbound.len());
                for (target, byte) in buf.iter_mut().zip(state.inbound.drain(..count)) {
                    *target = byte;
                }
                return Ok(count);
            }
            if state.ended || state.closed {
                return Ok(0);
            }
            if state.aborted {
                return Err(hung_up());
            }
            let wait = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(timed_out());
                    }
                    deadline - now
                }
                None => TICK * 10,
            };
            state = self
                .shared
                .changed
                .wait_timeout(state, wait)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let _sending = self.sending();
        {
            let state = self.shared.lock();
            if state.write_closed || state.closed {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the outgoing direction was closed",
                ));
            }
            if state.aborted {
                return Err(broken_pipe());
            }
        }
        let count = buf.len().min(self.shared.settings.chunk);
        self.shared.send_reliably(DATA, &buf[..count], None)?;
        Ok(count)
    }

    fn close_write(&self) -> io::Result<()> {
        let _sending = self.sending();
        {
            let mut state = self.shared.lock();
            if state.write_closed || state.aborted || state.closed {
                return Ok(());
            }
            state.write_closed = true;
        }
        self.shared.send_reliably(EOF, &[], None)
    }

    fn shutdown(&self) {
        {
            let mut state = self.shared.lock();
            if state.closed {
                return;
            }
            state.closed = true;
            self.shared.changed.notify_all();
        }
        trace!("{} session hanging up", self.shared.kind);
        // Waits for a write in progress, which now fails quickly.
        let _sending = self.sending();
        let end = {
            let mut state = self.shared.lock();
            let end = !state.write_closed && !state.aborted;
            state.write_closed = true;
            end
        };
        if end {
            let _ = self
                .shared
                .send_reliably(EOF, &[], Some(Instant::now() + LINGER));
        }
        let aborted = self.shared.lock().aborted;
        if !aborted {
            for _ in 0..3 {
                let _ = self.shared.send(&frame(ABORT, 0, &[]));
            }
        }
        let mut state = self.shared.lock();
        state.stopping = true;
        self.shared.changed.notify_all();
    }

    fn kind(&self) -> &'static str {
        self.shared.kind
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The chunk both sides use: the smaller proposal.
pub(crate) fn agree_chunk(own: u64, peer: Option<u64>) -> usize {
    peer.map_or(own, |peer| own.min(peer)).clamp(16, 65000) as usize
}

/// A pair of in-process queues, for tests of the session itself.
#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::sync::mpsc;

    #[derive(Debug)]
    pub(crate) struct MemoryLink {
        outgoing: Mutex<mpsc::Sender<Vec<u8>>>,
        incoming: Mutex<mpsc::Receiver<Vec<u8>>>,
        /// Drops every n-th outgoing frame when non-zero.
        pub(crate) lose_every: u32,
        counter: std::sync::atomic::AtomicU32,
    }

    pub(crate) fn pair(lose_every: u32) -> (MemoryLink, MemoryLink) {
        let (a_out, b_in) = mpsc::channel();
        let (b_out, a_in) = mpsc::channel();
        let link = |outgoing, incoming| MemoryLink {
            outgoing: Mutex::new(outgoing),
            incoming: Mutex::new(incoming),
            lose_every,
            counter: Default::default(),
        };
        (link(a_out, a_in), link(b_out, b_in))
    }

    impl Datagrams for MemoryLink {
        fn send(&self, frame: &[u8]) -> io::Result<bool> {
            let count = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if self.lose_every != 0 && count % self.lose_every == 0 {
                return Ok(false);
            }
            Ok(self
                .outgoing
                .lock()
                .unwrap()
                .send(frame.to_vec())
                .is_ok())
        }

        fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
            match self.incoming.lock().unwrap().recv_timeout(timeout) {
                Ok(frame) => {
                    buf.clear();
                    buf.extend_from_slice(&frame);
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        }
    }
}
