//! A TCP server exposing a links store through the LiNo protocols.
//!
//! Each connection gets its own thread that only parses and formats
//! messages; every request is executed by the thread that called
//! [`LinksServer::serve`], one at a time, so the store needs neither `Send`
//! nor locking. The protocol is detected per message and the reply uses the
//! same protocol, so text and binary clients can share one server.

use super::error::ProtocolResult;
use super::links_operations::GenericLinksOperation;
use super::protocols::{read_any_document_with_query_limits, MessageFormat, ProtocolLimits};
use crate::link::GenericLink;
use crate::named_type_links::NamedTypeLinks;
use crate::query_address::QueryAddress;
use crate::query_options::QueryLimits;
use crate::query_processor::{GenericQueryProcessor, QueryProcessor};
use crate::query_reply_budget::QueryReplyBudget;
use links_notation::binary::{format_document, LinoDocument};
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
    pub limits: ProtocolLimits,
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
        self.serve_for(storage)
    }

    /// Serves the selected address type through the same request path.
    /// Checked adapters may use a wider marker to retain their full stored range.
    pub fn serve_for<T: QueryAddress, S: NamedTypeLinks<T>>(
        self,
        storage: &mut S,
    ) -> ProtocolResult<()> {
        let LinksServer {
            listener,
            options,
            jobs,
            queue,
            stopping,
        } = self;
        let query_limits = storage.query_limits();
        let acceptor = {
            let stopping = Arc::clone(&stopping);
            thread::spawn(move || accept_loop(listener, jobs, stopping, options, query_limits))
        };
        let processor = GenericQueryProcessor::<T>::new(false)
            .with_auto_create_missing_references(options.auto_create_missing_references);
        while let Ok(job) = queue.recv() {
            match job {
                Job::Shutdown => break,
                Job::Request { document, reply } => {
                    if options.trace
                        && storage
                            .query_limits()
                            .is_none_or(|limits| admit_document(&document, limits).is_ok())
                    {
                        crate::diagnostics::stderr!(
                            "[server] request: {}",
                            format_document(&document)
                        );
                    }
                    let response = execute_request_for(storage, &processor, &document);
                    if options.trace {
                        crate::diagnostics::stderr!(
                            "[server] reply: {}",
                            format_document(&response)
                        );
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
    query_limits: Option<QueryLimits>,
) {
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let jobs = jobs.clone();
        thread::spawn(move || {
            if let Err(error) = handle_connection(stream, jobs, options, query_limits) {
                if options.trace {
                    crate::diagnostics::stderr!("[server] connection closed: {error}");
                }
            }
        });
    }
}

fn handle_connection(
    stream: TcpStream,
    jobs: Sender<Job>,
    options: ServerOptions,
    query_limits: Option<QueryLimits>,
) -> ProtocolResult<()> {
    let _ = stream.set_nodelay(true);
    let mut writer = BufWriter::new(stream.try_clone()?);
    let mut reader = BufReader::new(stream);
    loop {
        let (document, format) =
            match read_any_document_with_query_limits(&mut reader, &options.limits, query_limits) {
                Ok(Some(message)) => message,
                Ok(None) => {
                    if options.trace {
                        crate::diagnostics::stderr!("[server] client hung up");
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
    execute_request_for(storage, processor, document)
}

/// Executes an owned protocol document with the selected address type.
/// Checked storage policies admit the document before recursive formatting.
pub fn execute_request_for<T: QueryAddress, S: NamedTypeLinks<T>>(
    storage: &mut S,
    processor: &GenericQueryProcessor<T>,
    document: &[LiNo<String>],
) -> LinoDocument {
    match try_execute_request(storage, processor, document) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

fn try_execute_request<T: QueryAddress, S: NamedTypeLinks<T>>(
    storage: &mut S,
    processor: &GenericQueryProcessor<T>,
    document: &[LiNo<String>],
) -> anyhow::Result<LinoDocument> {
    if let Some(limits) = storage.query_limits() {
        admit_document(document, limits)?;
    }
    // A canonical document does not retain quoted-reference kinds. The envelope
    // carries original query text as one value through both text and binary.
    let original = match document {
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "query" => {
            let [LiNo::Ref(text)] = values.as_slice() else {
                anyhow::bail!("expected (query: original-query-text)");
            };
            Some(text.as_str())
        }
        _ => None,
    };
    if original.is_none() {
        if let Some(operation) = GenericLinksOperation::<T>::from_document(document)? {
            return operation.execute(storage);
        }
    }
    if document.is_empty() || original.is_some_and(|text| text.trim().is_empty()) {
        let mut budget = QueryReplyBudget::new(storage.query_limits());
        let mut links = storage.all_links();
        if let Some(limits) = storage.query_limits() {
            anyhow::ensure!(
                links.len() <= limits.max_results,
                "protocol result limit exceeded"
            );
        }
        links.sort_by_key(|link| link.index);
        return links
            .iter()
            .map(|link| link_lino(storage, link, &mut budget))
            .collect();
    }
    let legacy_text;
    let text = match original {
        Some(text) => text,
        None => {
            legacy_text = format_document(document);
            &legacy_text
        }
    };
    let changes = processor.process_query(storage, text)?;
    if !changes.is_empty() {
        storage.save()?;
    }
    let mut budget = QueryReplyBudget::new(storage.query_limits());
    changes
        .iter()
        .map(|(before, after)| {
            Ok(LiNo::Link {
                id: None,
                values: vec![
                    change_side(storage, before.as_ref(), &mut budget)?,
                    change_side(storage, after.as_ref(), &mut budget)?,
                ],
            })
        })
        .collect()
}

/// `(index: source target)`, naming every reference that has a name.
fn link_lino<T: QueryAddress, S: NamedTypeLinks<T> + ?Sized>(
    storage: &mut S,
    link: &GenericLink<T>,
    budget: &mut QueryReplyBudget,
) -> anyhow::Result<LiNo<String>> {
    Ok(LiNo::Link {
        id: Some(budget.reference(storage, link.index)?),
        values: vec![
            LiNo::Ref(budget.reference(storage, link.source)?),
            LiNo::Ref(budget.reference(storage, link.target)?),
        ],
    })
}

/// `()` for a missing side of a change, `((index: source target))` otherwise.
fn change_side<T: QueryAddress, S: NamedTypeLinks<T> + ?Sized>(
    storage: &mut S,
    link: Option<&GenericLink<T>>,
    budget: &mut QueryReplyBudget,
) -> anyhow::Result<LiNo<String>> {
    let values = match link {
        Some(link) => vec![link_lino(storage, link, budget)?],
        None => Vec::new(),
    };
    Ok(LiNo::Link { id: None, values })
}

/// Carries the exact original query text, including quoted-reference kinds.
/// Older peers must explicitly support this envelope before a client uses it.
pub fn query_document(text: &str) -> LinoDocument {
    vec![LiNo::Link {
        id: Some("query".into()),
        values: vec![LiNo::Ref(text.into())],
    }]
}

fn admit_document(document: &[LiNo<String>], limits: QueryLimits) -> anyhow::Result<()> {
    anyhow::ensure!(
        document.len() <= limits.max_nodes,
        "protocol node limit exceeded"
    );
    let mut pending: Vec<_> = document.iter().map(|node| (node, 0usize)).collect();
    let mut admitted = document.len();
    let mut bytes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        anyhow::ensure!(depth <= limits.max_depth, "protocol depth limit exceeded");
        match node {
            LiNo::Ref(value) => bytes = bytes.saturating_add(value.len()),
            LiNo::Link { id, values } => {
                bytes = bytes.saturating_add(id.as_ref().map_or(0, String::len));
                admitted = admitted.saturating_add(values.len());
                anyhow::ensure!(admitted <= limits.max_nodes, "protocol node limit exceeded");
                pending.extend(values.iter().map(|node| (node, depth + 1)));
            }
        }
        anyhow::ensure!(bytes <= limits.max_bytes, "protocol byte limit exceeded");
    }
    Ok(())
}

/// Parses bounded transport text before converting or formatting its document.
/// The upstream grammar enforces depth before building its recursive model.
pub fn parse_document_with_limits(text: &str, limits: QueryLimits) -> ProtocolResult<LinoDocument> {
    use links_notation::{parse_lino_to_links_with_config, ParserConfig};
    if text.len() > limits.max_bytes {
        return Err(super::error::ProtocolError::LimitExceeded(
            "protocol byte limit exceeded".into(),
        ));
    }
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let document = parse_lino_to_links_with_config(
        text,
        &ParserConfig::new().with_max_depth(limits.max_depth),
    )
    .map_err(|error| super::error::ProtocolError::InvalidLino(error.to_string()))?;
    admit_document(&document, limits)
        .map_err(|error| super::error::ProtocolError::LimitExceeded(error.to_string()))?;
    Ok(document
        .into_iter()
        .map(links_notation::binary::canonical)
        .collect())
}
