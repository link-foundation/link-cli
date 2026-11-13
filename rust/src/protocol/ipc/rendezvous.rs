//! The rendezvous directory where local kinds meet (see the "Rendezvous
//! directory" section of `docs/protocol/ipc-transports-v1.md`).

use super::json::Object;
use super::{process_alive, random_id, trace, Endpoint, IpcStream, Waker};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use web_time::Instant;

const VERSION: u64 = 1;
const LISTENER: &str = "listener.json";
/// File suffixes this protocol creates in a rendezvous directory.
const OWNED_SUFFIXES: &[&str] = &[
    ".req", ".ack", ".tmp", ".c2s", ".s2c", ".mmap", ".ring", ".sig",
];

/// Sleeps with a backoff from 1 ms up to `limit`.
pub(crate) struct Backoff {
    current: Duration,
    limit: Duration,
}

impl Backoff {
    pub(crate) fn new(limit: Duration) -> Self {
        Self {
            current: Duration::from_millis(1).min(limit.max(Duration::from_micros(50))),
            limit: limit.max(Duration::from_micros(50)),
        }
    }

    pub(crate) fn sleep(&mut self) {
        thread::sleep(self.current);
        self.current = (self.current * 2).min(self.limit);
    }
}

fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

fn read_object(path: &Path) -> io::Result<Object> {
    Object::parse(&fs::read_to_string(path)?)
}

fn create_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

fn canonical_kind(kind: &str) -> &str {
    if kind == "pipe" {
        "fifo"
    } else {
        kind
    }
}

/// The listening side of a rendezvous directory.
#[derive(Debug)]
pub(crate) struct RendezvousListener {
    dir: PathBuf,
    kind: &'static str,
    accept_poll: Duration,
    closed: Arc<AtomicBool>,
    endpoint: String,
}

impl RendezvousListener {
    pub(crate) fn bind(endpoint: &Endpoint, kind: &'static str) -> io::Result<Self> {
        let dir = PathBuf::from(endpoint.address());
        create_dir(&dir)?;
        let listener = dir.join(LISTENER);
        if let Ok(existing) = read_object(&listener) {
            if let Some(pid) = existing.number("pid") {
                if process_alive(pid as u32) {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        format!("process {pid} already listens on `{}`", dir.display()),
                    ));
                }
            }
        }
        // A stale directory: clear what this protocol left behind.
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == LISTENER || OWNED_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        write_atomically(
            &listener,
            &Object::new()
                .with("v", VERSION)
                .with("kind", canonical_kind(kind))
                .with("pid", std::process::id())
                .to_json(),
        )?;
        Ok(Self {
            dir,
            kind,
            accept_poll: endpoint.millis("accept-poll", 20)?,
            closed: Arc::new(AtomicBool::new(false)),
            endpoint: endpoint.to_string(),
        })
    }

    pub(crate) fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        Arc::new(move || closed.store(true, Ordering::SeqCst))
    }

    /// Waits for the next request; `None` once closed.
    pub(crate) fn next(&self) -> io::Result<Option<Request>> {
        let mut backoff = Backoff::new(self.accept_poll);
        loop {
            if self.is_closed() {
                return Ok(None);
            }
            let mut names: Vec<String> = fs::read_dir(&self.dir)?
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| name.ends_with(".req"))
                .collect();
            names.sort();
            for name in names {
                let path = self.dir.join(&name);
                let text = match fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                if fs::remove_file(&path).is_err() {
                    continue;
                }
                let id = name.trim_end_matches(".req").to_string();
                let mut request = Request {
                    id,
                    fields: Object::new(),
                    dir: self.dir.clone(),
                    answered: false,
                };
                match Object::parse(&text) {
                    Ok(fields)
                        if fields.number("v") == Some(VERSION)
                            && fields.text("kind") == Some(canonical_kind(self.kind))
                            && fields.number("pid").is_some()
                            && valid_id(&request.id) =>
                    {
                        request.fields = fields;
                        trace!(
                            "{} request {} from process {}",
                            self.kind,
                            request.id,
                            request.pid()
                        );
                        return Ok(Some(request));
                    }
                    _ => {
                        trace!("{} refused malformed request {name}", self.kind);
                        request.refuse("malformed or mismatched request");
                    }
                }
            }
            backoff.sleep();
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 20
        && id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

impl Drop for RendezvousListener {
    fn drop(&mut self) {
        let listener = self.dir.join(LISTENER);
        if read_object(&listener)
            .ok()
            .and_then(|object| object.number("pid"))
            == Some(std::process::id().into())
        {
            let _ = fs::remove_file(listener);
        }
        let _ = fs::remove_dir(&self.dir);
    }
}

/// A request claimed by the listener; dropping it unanswered refuses it.
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) id: String,
    pub(crate) fields: Object,
    dir: PathBuf,
    answered: bool,
}

impl Request {
    pub(crate) fn pid(&self) -> u32 {
        self.fields.number("pid").unwrap_or(0) as u32
    }

    pub(crate) fn path(&self, suffix: &str) -> PathBuf {
        self.dir.join(format!("{}.{suffix}", self.id))
    }

    pub(crate) fn ack(mut self, fields: Object) -> io::Result<()> {
        self.answered = true;
        write_atomically(
            &self.path("ack"),
            &fields
                .with("v", VERSION)
                .with("pid", std::process::id())
                .to_json(),
        )
    }

    pub(crate) fn refuse(&mut self, error: &str) {
        self.answered = true;
        let _ = write_atomically(
            &self.path("ack"),
            &Object::new()
                .with("v", VERSION)
                .with("error", error)
                .to_json(),
        );
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if !self.answered {
            self.refuse("the listener could not set the connection up");
        }
    }
}

/// The connecting side of a rendezvous directory.
#[derive(Debug)]
pub(crate) struct Rendezvous {
    dir: PathBuf,
    pub(crate) id: String,
    pub(crate) server_pid: u32,
    kind: &'static str,
    deadline: Instant,
    poll: Duration,
}

impl Rendezvous {
    pub(crate) fn begin(endpoint: &Endpoint, kind: &'static str) -> io::Result<Self> {
        let dir = PathBuf::from(endpoint.address());
        let listener = read_object(&dir.join(LISTENER)).map_err(|error| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("nobody listens on `{}`: {error}", dir.display()),
            )
        })?;
        let pid = listener.number("pid").unwrap_or(0) as u32;
        if listener.number("v") != Some(VERSION) {
            return Err(refused(format!(
                "`{}` uses another rendezvous version",
                dir.display()
            )));
        }
        let found = listener.text("kind").unwrap_or("");
        if found != canonical_kind(kind) {
            return Err(refused(format!(
                "`{}` listens for `{found}:` connections, not `{kind}:`",
                dir.display()
            )));
        }
        if pid == 0 || !process_alive(pid) {
            return Err(refused(format!(
                "the listener of `{}` (process {pid}) is gone",
                dir.display()
            )));
        }
        Ok(Self {
            dir,
            id: random_id(),
            server_pid: pid,
            kind,
            deadline: Instant::now() + endpoint.timeout(),
            poll: endpoint.millis("poll", 5)?,
        })
    }

    pub(crate) fn path(&self, suffix: &str) -> PathBuf {
        self.dir.join(format!("{}.{suffix}", self.id))
    }

    /// Writes the request and waits for the ack.
    pub(crate) fn request(&self, fields: Object) -> io::Result<Object> {
        let request = self.path("req");
        let ack = self.path("ack");
        write_atomically(
            &request,
            &fields
                .with("v", VERSION)
                .with("kind", canonical_kind(self.kind))
                .with("pid", std::process::id())
                .to_json(),
        )?;
        trace!("{} request {} written", self.kind, self.id);
        let mut backoff = Backoff::new(self.poll.max(Duration::from_millis(1)));
        let mut last_check = Instant::now();
        loop {
            match fs::read_to_string(&ack) {
                Ok(text) if !text.is_empty() => {
                    let _ = fs::remove_file(&ack);
                    let object = Object::parse(&text)?;
                    if let Some(error) = object.text("error") {
                        return Err(refused(format!("the listener refused: {error}")));
                    }
                    return Ok(object);
                }
                _ => {}
            }
            let now = Instant::now();
            let gone = now.duration_since(last_check) >= Duration::from_millis(100) && {
                last_check = now;
                !process_alive(self.server_pid)
            };
            if gone || now >= self.deadline {
                let _ = fs::remove_file(&request);
                let _ = fs::remove_file(&ack);
                return Err(if gone {
                    refused(format!(
                        "the listener (process {}) is gone",
                        self.server_pid
                    ))
                } else {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "no answer from the listener of `{}` in time",
                            self.dir.display()
                        ),
                    )
                });
            }
            backoff.sleep();
        }
    }
}

fn refused(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionRefused, message)
}

/// Answers rendezvous requests with `open` until one succeeds.
#[cfg_attr(not(any(unix, windows)), allow(dead_code))] // used by the OS-specific kinds
pub(crate) fn accept_with(
    rendezvous: &RendezvousListener,
    kind: &str,
    mut open: impl FnMut(&Request) -> io::Result<(Object, IpcStream)>,
) -> io::Result<Option<IpcStream>> {
    while let Some(mut request) = rendezvous.next()? {
        match open(&request) {
            Ok((fields, stream)) => {
                let (id, pid) = (request.id.clone(), request.pid());
                request.ack(fields)?;
                trace!("{kind} accepted {id} from process {pid}");
                return Ok(Some(stream));
            }
            Err(error) => {
                trace!("{kind} refused {}: {error}", request.id);
                request.refuse(&error.to_string());
            }
        }
    }
    Ok(None)
}
