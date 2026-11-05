//! A TCP server exposing a links store through the LiNo protocols.
//!
//! Each connection gets its own thread that only parses messages and another
//! that writes them; every request is executed by the thread that called
//! [`LinksServer::serve`], one at a time, so the store needs neither `Send`
//! nor locking. The protocol is detected per message and the reply uses the
//! same protocol, so text and binary clients can share one server.
//!
//! A connection may `(subscribe: (index source target))` to the links matching
//! a pattern: every later write of another connection that changes such a link
//! is pushed to it as `(notify: N)` and the changes, in the protocol of the
//! subscription request, until it sends `(unsubscribe: N)` or hangs up. The
//! serving thread queues a reply together with the pushes of the same write,
//! so every connection reads its replies and pushes in the order the store
//! changed.
//!
//! Every connection is a session of a [`LinksSessions`], with a subject of
//! its own (issue #149); a subscriber is pushed only what its subject may
//! read.
//!
//! With [`ServerOptions::triplets`] the same requests and pushes are about
//! triplets; see [`TripletsOperation`].

use super::error::ProtocolResult;
use super::links_operations::{
    matches, notification_document, parse_changes, subscription_document, Change, LinksOperation,
    Part,
};
use super::protocols::{read_any_document, MessageFormat, ProtocolLimits};
use super::sessions::{LinksSessions, SessionId};
use super::triplets_operations::{
    matches_triplet, pack_triplets, triplet_notification_document, TripletsOperation,
};
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::permissions::AccessPolicy;
use crate::query_processor::QueryProcessor;
use crate::triplets::{Triplet, TripletChange};
use links_notation::binary::{format_document, LinoDocument};
use links_notation::LiNo;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::{self, BufReader, BufWriter, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;

/// Which protocols a server accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AcceptedProtocols {
    /// Detect the protocol of every message (the default).
    #[default]
    Any,
    /// Only [`TextLinoProtocol`](super::TextLinoProtocol) messages.
    Text,
    /// Only [`BinaryLinoProtocol`](super::BinaryLinoProtocol) messages.
    Binary,
}

impl AcceptedProtocols {
    fn accepts(self, format: MessageFormat) -> bool {
        matches!(
            (self, format),
            (AcceptedProtocols::Any, _)
                | (AcceptedProtocols::Text, MessageFormat::Text)
                | (AcceptedProtocols::Binary, MessageFormat::Binary(_))
        )
    }
}

/// Server configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerOptions {
    /// Print every request and reply to stderr.
    pub trace: bool,
    /// Passed to [`QueryProcessor::with_auto_create_missing_references`].
    pub auto_create_missing_references: bool,
    /// Protocols the server answers.
    pub accept: AcceptedProtocols,
    /// Limits applied to incoming messages.
    pub limits: ProtocolLimits,
    /// Pushes a connection may have waiting before the server closes it as
    /// too slow to read them.
    pub max_pending_notifications: usize,
    /// Read every request as triplets (issue #152): `(create: (type from to))`
    /// and the rest of [`TripletsOperation`], over a store of doublets. A server
    /// without it answers as it always did.
    pub triplets: bool,
}

/// The default of [`ServerOptions::max_pending_notifications`].
pub const DEFAULT_MAX_PENDING_NOTIFICATIONS: usize = 1024;

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            trace: false,
            auto_create_missing_references: false,
            accept: AcceptedProtocols::default(),
            limits: ProtocolLimits::default(),
            max_pending_notifications: DEFAULT_MAX_PENDING_NOTIFICATIONS,
            triplets: false,
        }
    }
}

/// Which links a subscriber may see: see [`LinksServer::with_readable`].
pub type Readable = Box<dyn Fn(&Link) -> bool + Send>;

enum Job {
    /// A request of `connection`; its reply goes to `outbox`, and `done` is
    /// signalled once it is there.
    Request {
        connection: u64,
        outbox: Arc<Outbox>,
        document: LinoDocument,
        format: MessageFormat,
        done: Sender<()>,
    },
    /// `connection` hung up: its subscriptions go with it.
    Close(u64),
    Shutdown,
}

/// Stops a running [`LinksServer::serve`] from another thread.
#[derive(Clone, Debug)]
pub struct ShutdownHandle {
    jobs: Sender<Job>,
    stopping: Arc<AtomicBool>,
    address: SocketAddr,
}

impl ShutdownHandle {
    /// Asks the server to stop; `serve` returns after the current request.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.jobs.send(Job::Shutdown);
        // Wake the accept loop so it notices the flag.
        let _ = TcpStream::connect(self.address);
    }
}

/// A bound, not yet serving, LiNo server.
pub struct LinksServer {
    listener: TcpListener,
    options: ServerOptions,
    readable: Option<Readable>,
    jobs: Sender<Job>,
    queue: Receiver<Job>,
    stopping: Arc<AtomicBool>,
}

impl fmt::Debug for LinksServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinksServer")
            .field("listener", &self.listener)
            .field("options", &self.options)
            .field("readable", &self.readable.is_some())
            .finish_non_exhaustive()
    }
}

impl LinksServer {
    /// Binds to `address` (use port `0` for an ephemeral port).
    pub fn bind(address: impl ToSocketAddrs, options: ServerOptions) -> ProtocolResult<Self> {
        let listener = TcpListener::bind(address)?;
        let (jobs, queue) = mpsc::channel();
        Ok(Self {
            listener,
            options,
            readable: None,
            jobs,
            queue,
            stopping: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Lets subscribers see only the links `readable` accepts; without it,
    /// every subscriber sees every link.
    ///
    /// A side of a change the subscriber may not read is left out of its push,
    /// so a link that becomes unreadable arrives as a deletion and one that
    /// becomes readable as a creation. This is where a permission layer plugs in.
    /// On a server in triplet mode `readable` judges the outer doublet of a
    /// triplet, as `(index: type index)`.
    pub fn with_readable(mut self, readable: impl Fn(&Link) -> bool + Send + 'static) -> Self {
        self.readable = Some(Box::new(readable));
        self
    }

    /// The address the server listens on.
    pub fn local_addr(&self) -> ProtocolResult<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// A handle that stops [`serve`](Self::serve).
    pub fn shutdown_handle(&self) -> ProtocolResult<ShutdownHandle> {
        Ok(ShutdownHandle {
            jobs: self.jobs.clone(),
            stopping: Arc::clone(&self.stopping),
            address: self.local_addr()?,
        })
    }

    /// Serves `storage` until a [`ShutdownHandle`] stops the server; every
    /// connection sees and changes the whole store.
    pub fn serve<S: NamedTypeLinks>(self, storage: &mut S) -> ProtocolResult<()> {
        let auto_create = self.options.auto_create_missing_references;
        let mut sessions =
            LinksSessions::new(storage).with_auto_create_missing_references(auto_create);
        self.serve_sessions(&mut sessions)
    }

    /// Serves `sessions` until a [`ShutdownHandle`] stops the server: every
    /// connection is a session of its own, opened with the default subject
    /// of `sessions` and closed when the connection closes (issue #149).
    pub fn serve_sessions<S: NamedTypeLinks, P: AccessPolicy>(
        self,
        sessions: &mut LinksSessions<S, P>,
    ) -> ProtocolResult<()> {
        let LinksServer {
            listener,
            options,
            readable,
            jobs,
            queue,
            stopping,
        } = self;
        if options.triplets {
            sessions
                .ensure_triplets()
                .map_err(|error| io::Error::other(format!("{error:#}")))?;
        }
        let acceptor = {
            let stopping = Arc::clone(&stopping);
            thread::spawn(move || accept_loop(listener, jobs, stopping, options))
        };
        let mut dispatcher = Dispatcher::new(options, readable);
        while let Ok(job) = queue.recv() {
            if !dispatcher.run(sessions, job) {
                break;
            }
        }
        dispatcher.close_all(sessions);
        stopping.store(true, Ordering::SeqCst);
        drop(queue);
        let _ = acceptor.join();
        Ok(())
    }
}

/// Runs the jobs of [`LinksServer::serve`] and keeps the subscriptions of
/// every connection.
struct Dispatcher {
    options: ServerOptions,
    readable: Option<Readable>,
    connections: BTreeMap<u64, Connection>,
}

impl Dispatcher {
    fn new(options: ServerOptions, readable: Option<Readable>) -> Self {
        Self {
            options,
            readable,
            connections: BTreeMap::new(),
        }
    }

    /// Runs `job`; false once the server should stop.
    fn run<S: NamedTypeLinks, P: AccessPolicy>(
        &mut self,
        sessions: &mut LinksSessions<S, P>,
        job: Job,
    ) -> bool {
        match job {
            Job::Shutdown => return false,
            Job::Close(connection) => {
                if let Some(closed) = self.connections.remove(&connection) {
                    sessions.close(closed.session);
                }
            }
            Job::Request {
                connection,
                outbox,
                document,
                format,
                done,
            } => {
                if self.options.trace {
                    eprintln!("[server] request: {}", format_document(&document));
                }
                let current = self
                    .connections
                    .entry(connection)
                    .or_insert_with(|| Connection::new(outbox, sessions.open(None)));
                let triplets = sessions.has_triplets();
                let mut changes = Vec::new();
                let mut triplet_changes = Vec::new();
                let reply = current
                    .execute(&document, format, triplets)
                    .unwrap_or_else(|| {
                        if triplets {
                            sessions.execute_triplets_observed(
                                current.session,
                                &document,
                                &mut triplet_changes,
                            )
                        } else {
                            sessions.execute_observed(current.session, &document, &mut changes)
                        }
                    });
                if self.options.trace {
                    eprintln!("[server] reply: {}", format_document(&reply));
                }
                let reply = if triplets {
                    pack_triplets(reply, format)
                } else {
                    reply
                };
                current.outbox.reply(reply, format);
                let readable = self.readable.as_ref();
                let others = self.connections.iter().filter(|(id, _)| **id != connection);
                if !changes.is_empty() || !triplet_changes.is_empty() {
                    for (_, other) in others {
                        let session = other.session;
                        let mut can_read = |link: &Link| {
                            readable.is_none_or(|readable| readable(link))
                                && sessions.can_read(session, link)
                        };
                        other.notify(&changes, &mut can_read);
                        other.notify_triplets(&triplet_changes, &mut can_read);
                    }
                }
                let _ = done.send(());
            }
        }
        true
    }

    /// Closes the session of every connection still open.
    fn close_all<S: NamedTypeLinks, P: AccessPolicy>(
        &mut self,
        sessions: &mut LinksSessions<S, P>,
    ) {
        for (_, connection) in std::mem::take(&mut self.connections) {
            sessions.close(connection.session);
        }
    }
}

/// The subscriptions of one connection, and the session its requests run
/// as; only the serving thread touches them.
struct Connection {
    outbox: Arc<Outbox>,
    session: SessionId,
    subscriptions: BTreeMap<u32, (Vec<Part>, MessageFormat)>,
    next_subscription: u32,
}

impl Connection {
    fn new(outbox: Arc<Outbox>, session: SessionId) -> Self {
        Self {
            outbox,
            session,
            subscriptions: BTreeMap::new(),
            next_subscription: 1,
        }
    }

    /// The reply to a `subscribe` or an `unsubscribe`; `None` for any other
    /// request. Subscription numbers start at 1 on every connection.
    fn execute(
        &mut self,
        document: &[LiNo<String>],
        format: MessageFormat,
        triplets: bool,
    ) -> Option<LinoDocument> {
        if triplets {
            match TripletsOperation::from_document(document) {
                Ok(Some(TripletsOperation::Subscribe(restriction))) => {
                    return Some(self.subscribe(restriction, format));
                }
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => return None,
            }
        }
        match LinksOperation::from_document(document) {
            Ok(Some(LinksOperation::Subscribe(restriction))) => {
                Some(self.subscribe(restriction, format))
            }
            Ok(Some(LinksOperation::Unsubscribe(subscription))) => {
                // An unknown number is ignored.
                self.subscriptions.remove(&subscription);
                Some(Vec::new())
            }
            _ => None,
        }
    }

    fn subscribe(&mut self, restriction: Vec<Part>, format: MessageFormat) -> LinoDocument {
        let subscription = self.next_subscription;
        self.next_subscription = self.next_subscription.wrapping_add(1);
        self.subscriptions
            .insert(subscription, (restriction, format));
        subscription_document(subscription)
    }

    /// Pushes `changes` to every subscription they fit, one `(notify: N)`
    /// document each; a side that does not fit (or that `readable` refuses) is
    /// left out, and a change left with no side is not pushed.
    fn notify(&self, changes: &[Change], readable: &mut dyn FnMut(&Link) -> bool) {
        for (subscription, (restriction, format)) in &self.subscriptions {
            let mut side = |link: &Link| {
                if !link.is_null() && matches(link, restriction) && readable(link) {
                    *link
                } else {
                    Link::null()
                }
            };
            let seen: Vec<Change> = changes
                .iter()
                .map(|(before, after)| (side(before), side(after)))
                .filter(|(before, after)| !before.is_null() || !after.is_null())
                .collect();
            if !seen.is_empty()
                && !self
                    .outbox
                    .push(notification_document(*subscription, &seen), *format)
            {
                return;
            }
        }
    }
}

impl Connection {
    /// [`notify`](Self::notify) for the changes of a server in triplet mode.
    fn notify_triplets(&self, changes: &[TripletChange], readable: &mut dyn FnMut(&Link) -> bool) {
        if changes.is_empty() {
            return;
        }
        for (subscription, (restriction, format)) in &self.subscriptions {
            let mut side = |triplet: Option<Triplet>| {
                triplet.filter(|triplet| {
                    matches_triplet(triplet, restriction)
                        && readable(&Link::new(triplet.index, triplet.type_id, triplet.index))
                })
            };
            let seen: Vec<TripletChange> = changes
                .iter()
                .map(|change| TripletChange {
                    before: side(change.before),
                    after: side(change.after),
                })
                .filter(|change| change.before.is_some() || change.after.is_some())
                .collect();
            if !seen.is_empty()
                && !self.outbox.push(
                    pack_triplets(triplet_notification_document(*subscription, &seen), *format),
                    *format,
                )
            {
                return;
            }
        }
    }
}

/// The documents waiting to be written to one connection, replies and pushes
/// in one queue, so a push never interleaves with a reply and a slow
/// subscriber never holds up the store.
struct Outbox {
    state: Mutex<OutboxState>,
    changed: Condvar,
    max_pending_notifications: usize,
    /// Stops both threads of the connection.
    hang_up: Box<dyn Fn() + Send + Sync>,
}

#[derive(Default)]
struct OutboxState {
    queue: VecDeque<(LinoDocument, MessageFormat, bool)>,
    pending_replies: usize,
    pending_notifications: usize,
    closed: bool,
}

impl Outbox {
    fn new(max_pending_notifications: usize, hang_up: Box<dyn Fn() + Send + Sync>) -> Self {
        Self {
            state: Mutex::default(),
            changed: Condvar::new(),
            max_pending_notifications,
            hang_up,
        }
    }

    fn lock(&self) -> MutexGuard<'_, OutboxState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Queues a reply; [`wait_for_replies`](Self::wait_for_replies) waits
    /// until it is written.
    fn reply(&self, document: LinoDocument, format: MessageFormat) {
        let mut state = self.lock();
        if state.closed {
            return;
        }
        state.queue.push_back((document, format, true));
        state.pending_replies += 1;
        self.changed.notify_all();
    }

    /// Queues a push; closes the connection instead when too many are already
    /// waiting, since a subscriber that does not read its pushes would make
    /// the server hold every change for it. False once the outbox is closed.
    fn push(&self, document: LinoDocument, format: MessageFormat) -> bool {
        {
            let mut state = self.lock();
            if state.closed {
                return false;
            }
            if state.pending_notifications < self.max_pending_notifications {
                state.queue.push_back((document, format, false));
                state.pending_notifications += 1;
                self.changed.notify_all();
                return true;
            }
        }
        self.close();
        false
    }

    /// Waits until every queued reply is written, so a client that never reads
    /// its replies stops being read from, as before the server had pushes.
    fn wait_for_replies(&self) {
        let mut state = self.lock();
        while state.pending_replies > 0 && !state.closed {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// Writes the queue to `writer`, in order, until the outbox closes.
    fn write_all(&self, writer: &mut impl Write, limits: ProtocolLimits) -> ProtocolResult<()> {
        let result = self.try_write_all(writer, limits);
        if result.is_err() {
            // The connection broke: its reader stops too.
            self.close();
        }
        result
    }

    fn try_write_all(&self, writer: &mut impl Write, limits: ProtocolLimits) -> ProtocolResult<()> {
        loop {
            let (document, format) = {
                let mut state = self.lock();
                while state.queue.is_empty() && !state.closed {
                    state = self
                        .changed
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                if state.closed {
                    return Ok(());
                }
                let (document, format, _) = &state.queue[0];
                (document.clone(), *format)
            };
            format.protocol(limits).write_document(writer, &document)?;
            writer.flush()?;
            let mut state = self.lock();
            if let Some((_, _, reply)) = state.queue.pop_front() {
                if reply {
                    state.pending_replies -= 1;
                } else {
                    state.pending_notifications -= 1;
                }
            }
            self.changed.notify_all();
        }
    }

    /// Stops the outbox and hangs up; called again, it does nothing.
    fn close(&self) {
        {
            let mut state = self.lock();
            if state.closed {
                return;
            }
            state.closed = true;
            state.queue.clear();
            self.changed.notify_all();
        }
        (self.hang_up)();
    }
}

fn accept_loop(
    listener: TcpListener,
    jobs: Sender<Job>,
    stopping: Arc<AtomicBool>,
    options: ServerOptions,
) {
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let jobs = jobs.clone();
        thread::spawn(move || {
            if let Err(error) = handle_connection(stream, jobs, options) {
                if options.trace {
                    eprintln!("[server] connection closed: {error}");
                }
            }
        });
    }
}

fn handle_connection(
    stream: TcpStream,
    jobs: Sender<Job>,
    options: ServerOptions,
) -> ProtocolResult<()> {
    static CONNECTIONS: AtomicU64 = AtomicU64::new(0);
    let connection = CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    let _ = stream.set_nodelay(true);
    let outbox = {
        let stream = stream.try_clone()?;
        Arc::new(Outbox::new(
            options.max_pending_notifications,
            Box::new(move || {
                let _ = stream.shutdown(Shutdown::Both);
            }),
        ))
    };
    let writer = {
        let outbox = Arc::clone(&outbox);
        let mut stream = BufWriter::new(stream.try_clone()?);
        thread::spawn(move || {
            if let Err(error) = outbox.write_all(&mut stream, options.limits) {
                if options.trace {
                    eprintln!("[server] connection closed: {error}");
                }
            }
        })
    };
    let result = serve_connection(stream, connection, &outbox, &jobs, options);
    // Its subscriptions go with it.
    let _ = jobs.send(Job::Close(connection));
    outbox.close();
    let _ = writer.join();
    result
}

fn serve_connection(
    stream: TcpStream,
    connection: u64,
    outbox: &Arc<Outbox>,
    jobs: &Sender<Job>,
    options: ServerOptions,
) -> ProtocolResult<()> {
    let mut reader = BufReader::new(stream);
    loop {
        let (document, format) = match read_any_document(&mut reader, &options.limits) {
            Ok(Some(message)) => message,
            Ok(None) => {
                if options.trace {
                    eprintln!("[server] client hung up");
                }
                return Ok(());
            }
            Err(_) if outbox.is_closed() => return Ok(()),
            Err(error) => {
                // The stream may be out of sync; answer in text and hang up.
                outbox.reply(error_document(&error.to_string()), MessageFormat::Text);
                outbox.wait_for_replies();
                return Err(error);
            }
        };
        if options.accept.accepts(format) {
            let (done, finished) = mpsc::channel();
            let job = Job::Request {
                connection,
                outbox: Arc::clone(outbox),
                document,
                format,
                done,
            };
            // Once `serve` returns, no request may touch the store.
            if jobs.send(job).is_err() || finished.recv().is_err() {
                outbox.reply(error_document("server is shutting down"), format);
            }
        } else {
            outbox.reply(
                error_document("this server does not accept this protocol"),
                format,
            );
        }
        outbox.wait_for_replies();
        if outbox.is_closed() {
            if options.trace {
                eprintln!("[server] connection closed");
            }
            return Ok(());
        }
    }
}

/// The reply document for a failed request: `(error: 'message')`.
pub fn error_document(message: &str) -> LinoDocument {
    vec![LiNo::Link {
        id: Some("error".to_string()),
        values: vec![LiNo::Ref(message.to_string())],
    }]
}

/// Returns the message of an `(error: 'message')` reply.
pub fn error_message(document: &[LiNo<String>]) -> Option<&str> {
    match document {
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "error" => match values.as_slice() {
            [LiNo::Ref(message)] => Some(message),
            _ => None,
        },
        _ => None,
    }
}

/// Executes one request against `storage`.
///
/// A non-empty document is a substitution query; the reply holds one
/// `(before) (after)` line per change, exactly like `clink --changes`. The
/// empty document asks for every link, one `(index: source target)` per line.
/// Failures produce an [`error_document`].
pub fn execute_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
) -> LinoDocument {
    execute_observed_request(storage, processor, document, &mut Vec::new())
}

/// Like [`execute_request`], adding the net change of every link the request
/// changed to `changed`, with plain numbers.
pub(super) fn execute_observed_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
    changed: &mut Vec<Change>,
) -> LinoDocument {
    match try_execute_request(storage, processor, document, changed) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

fn try_execute_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
    changed: &mut Vec<Change>,
) -> anyhow::Result<LinoDocument> {
    if let Some(operation) = LinksOperation::from_document(document)? {
        let reply = operation.execute(storage)?;
        if matches!(
            operation,
            LinksOperation::Create { .. }
                | LinksOperation::Update { .. }
                | LinksOperation::Delete(_)
        ) {
            changed.extend(parse_changes(&reply)?);
        }
        return Ok(reply);
    }
    if document.is_empty() {
        let mut links = storage.all_links();
        links.sort_by_key(|link| link.index);
        return links.iter().map(|link| link_lino(storage, link)).collect();
    }
    let changes = processor.process_query(storage, &format_document(document))?;
    changed.extend(changes.iter().map(|(before, after)| {
        (
            before.unwrap_or_else(Link::null),
            after.unwrap_or_else(Link::null),
        )
    }));
    if !changes.is_empty() {
        storage.save()?;
    }
    changes
        .iter()
        .map(|(before, after)| {
            Ok(LiNo::Link {
                id: None,
                values: vec![
                    change_side(storage, before.as_ref())?,
                    change_side(storage, after.as_ref())?,
                ],
            })
        })
        .collect()
}

/// `(index: source target)`, naming every reference that has a name.
fn link_lino<S: NamedTypeLinks>(storage: &mut S, link: &Link) -> anyhow::Result<LiNo<String>> {
    Ok(LiNo::Link {
        id: Some(reference_name(storage, link.index)?),
        values: vec![
            LiNo::Ref(reference_name(storage, link.source)?),
            LiNo::Ref(reference_name(storage, link.target)?),
        ],
    })
}

/// `()` for a missing side of a change, `((index: source target))` otherwise.
fn change_side<S: NamedTypeLinks>(
    storage: &mut S,
    link: Option<&Link>,
) -> anyhow::Result<LiNo<String>> {
    let values = match link {
        Some(link) => vec![link_lino(storage, link)?],
        None => Vec::new(),
    };
    Ok(LiNo::Link { id: None, values })
}

pub(super) fn reference_name<S: NamedTypeLinks + ?Sized>(
    storage: &mut S,
    id: u32,
) -> anyhow::Result<String> {
    Ok(storage.get_name(id)?.unwrap_or_else(|| id.to_string()))
}

#[cfg(test)]
mod tests;
