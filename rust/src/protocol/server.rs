//! A TCP server exposing a links store through the LiNo protocols.
//!
//! Each connection gets its own thread that only parses and formats
//! messages; every request is executed by the thread that called
//! [`LinksServer::serve`], one at a time, so the store needs neither `Send`
//! nor locking. The protocol is detected per message and the reply uses the
//! same protocol, so text and binary clients can share one server.

use super::error::ProtocolResult;
use super::format::format_document;
use super::links_operations::LinksOperation;
use super::mapping::LinoDocument;
use super::packet::DecodeLimits;
use super::protocols::{read_any_document, MessageFormat};
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::query_processor::QueryProcessor;
use links_notation::LiNo;
use std::io::{BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerOptions {
    /// Print every request and reply to stderr.
    pub trace: bool,
    /// Passed to [`QueryProcessor::with_auto_create_missing_references`].
    pub auto_create_missing_references: bool,
    /// Protocols the server answers.
    pub accept: AcceptedProtocols,
    /// Limits applied to incoming messages.
    pub limits: DecodeLimits,
}

enum Job {
    Request {
        document: LinoDocument,
        reply: Sender<LinoDocument>,
    },
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
#[derive(Debug)]
pub struct LinksServer {
    listener: TcpListener,
    options: ServerOptions,
    jobs: Sender<Job>,
    queue: Receiver<Job>,
    stopping: Arc<AtomicBool>,
}

impl LinksServer {
    /// Binds to `address` (use port `0` for an ephemeral port).
    pub fn bind(address: impl ToSocketAddrs, options: ServerOptions) -> ProtocolResult<Self> {
        let listener = TcpListener::bind(address)?;
        let (jobs, queue) = mpsc::channel();
        Ok(Self {
            listener,
            options,
            jobs,
            queue,
            stopping: Arc::new(AtomicBool::new(false)),
        })
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

    /// Serves `storage` until a [`ShutdownHandle`] stops the server.
    pub fn serve<S: NamedTypeLinks>(self, storage: &mut S) -> ProtocolResult<()> {
        let LinksServer {
            listener,
            options,
            jobs,
            queue,
            stopping,
        } = self;
        let acceptor = {
            let stopping = Arc::clone(&stopping);
            thread::spawn(move || accept_loop(listener, jobs, stopping, options))
        };
        let processor = QueryProcessor::new(false)
            .with_auto_create_missing_references(options.auto_create_missing_references);
        while let Ok(job) = queue.recv() {
            match job {
                Job::Shutdown => break,
                Job::Request { document, reply } => {
                    if options.trace {
                        eprintln!("[server] request: {}", format_document(&document));
                    }
                    let response = execute_request(storage, &processor, &document);
                    if options.trace {
                        eprintln!("[server] reply: {}", format_document(&response));
                    }
                    let _ = reply.send(response);
                }
            }
        }
        stopping.store(true, Ordering::SeqCst);
        drop(queue);
        let _ = acceptor.join();
        Ok(())
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
    let _ = stream.set_nodelay(true);
    let mut writer = BufWriter::new(stream.try_clone()?);
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
            Err(error) => {
                // The stream may be out of sync; answer in text and hang up.
                let reply = error_document(&error.to_string());
                let _ = MessageFormat::Text
                    .protocol(options.limits)
                    .write_document(&mut writer, &reply);
                return Err(error);
            }
        };
        let response = if options.accept.accepts(format) {
            let (reply, response) = mpsc::channel();
            // Once `serve` returns, no request may touch the store.
            jobs.send(Job::Request { document, reply })
                .ok()
                .and_then(|()| response.recv().ok())
                .unwrap_or_else(|| error_document("server is shutting down"))
        } else {
            error_document("this server does not accept this protocol")
        };
        format
            .protocol(options.limits)
            .write_document(&mut writer, &response)?;
        writer.flush()?;
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
    match try_execute_request(storage, processor, document) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

fn try_execute_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
) -> anyhow::Result<LinoDocument> {
    if let Some(operation) = LinksOperation::from_document(document)? {
        return operation.execute(storage);
    }
    if document.is_empty() {
        let mut links = storage.all_links();
        links.sort_by_key(|link| link.index);
        return links.iter().map(|link| link_lino(storage, link)).collect();
    }
    let changes = processor.process_query(storage, &format_document(document))?;
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

fn reference_name<S: NamedTypeLinks>(storage: &mut S, id: u32) -> anyhow::Result<String> {
    Ok(storage.get_name(id)?.unwrap_or_else(|| id.to_string()))
}
