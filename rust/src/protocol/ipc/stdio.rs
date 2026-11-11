//! Anonymous pipes: `stdio` (this process's standard input and output) and
//! `exec:COMMAND` (a child process's).

use super::json::parse_strings;
use super::{timed_out, trace, Endpoint, IpcStream, Listener, Transport, Waker};
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use web_time::Instant;

const BUFFERED: usize = 1 << 20;

#[derive(Debug, Default)]
struct ReaderState {
    buffer: VecDeque<u8>,
    ended: bool,
    error: Option<(io::ErrorKind, String)>,
    discarding: bool,
}

/// Reads a blocking source on a thread of its own, so reads can time out
/// and hanging up never waits for the source.
#[derive(Debug, Default)]
struct ThreadedReader {
    state: Mutex<ReaderState>,
    changed: Condvar,
}

impl ThreadedReader {
    fn start(name: &str, mut source: impl Read + Send + 'static) -> io::Result<Arc<Self>> {
        let reader = Arc::new(Self::default());
        let shared = Arc::clone(&reader);
        thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut chunk = vec![0; 64 * 1024];
                loop {
                    let result = source.read(&mut chunk);
                    let mut state = shared.lock();
                    match result {
                        Ok(0) => {
                            state.ended = true;
                            shared.changed.notify_all();
                            return;
                        }
                        Ok(count) => {
                            while !state.discarding && state.buffer.len() >= BUFFERED {
                                state = shared
                                    .changed
                                    .wait(state)
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                            }
                            if !state.discarding {
                                state.buffer.extend(&chunk[..count]);
                            }
                            shared.changed.notify_all();
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        Err(error) => {
                            state.error = Some((error.kind(), error.to_string()));
                            shared.changed.notify_all();
                            return;
                        }
                    }
                }
            })?;
        Ok(reader)
    }

    fn lock(&self) -> MutexGuard<'_, ReaderState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read(
        &self,
        buf: &mut [u8],
        timeout: Option<Duration>,
        closed: &AtomicBool,
    ) -> io::Result<usize> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut state = self.lock();
        loop {
            if !state.buffer.is_empty() {
                let count = buf.len().min(state.buffer.len());
                for (target, byte) in buf.iter_mut().zip(state.buffer.drain(..count)) {
                    *target = byte;
                }
                self.changed.notify_all();
                return Ok(count);
            }
            if state.ended || closed.load(Ordering::SeqCst) {
                return Ok(0);
            }
            if let Some((kind, message)) = &state.error {
                return Err(io::Error::new(*kind, message.clone()));
            }
            let wait = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(timed_out());
                    }
                    (deadline - now).min(Duration::from_millis(100))
                }
                None => Duration::from_millis(100),
            };
            state = self
                .changed
                .wait_timeout(state, wait)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    /// Stops buffering; later input is thrown away.
    fn discard(&self) {
        let mut state = self.lock();
        state.discarding = true;
        state.buffer.clear();
        self.changed.notify_all();
    }
}

// ---------------------------------------------------------------- stdio

/// Signals the end of the single `stdio` connection.
#[derive(Debug, Default)]
struct Ended {
    ended: Mutex<bool>,
    changed: Condvar,
}

impl Ended {
    fn set(&self) {
        *self
            .ended
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.changed.notify_all();
    }

    fn wait_until(&self, closed: &AtomicBool) {
        let mut ended = self
            .ended
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*ended && !closed.load(Ordering::SeqCst) {
            ended = self
                .changed
                .wait_timeout(ended, Duration::from_millis(100))
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }
}

struct StdioTransport {
    input: Arc<ThreadedReader>,
    output: Mutex<Option<io::Stdout>>,
    closed: AtomicBool,
    ended: Arc<Ended>,
}

impl fmt::Debug for StdioTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StdioTransport")
            .finish_non_exhaustive()
    }
}

impl StdioTransport {
    fn output(&self) -> MutexGuard<'_, Option<io::Stdout>> {
        self.output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Transport for StdioTransport {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        self.input.read(buf, timeout, &self.closed)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self.output().as_mut() {
            Some(output) => output.write(buf),
            None => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "standard output was closed",
            )),
        }
    }

    fn flush(&self) -> io::Result<()> {
        match self.output().as_mut() {
            Some(output) => output.flush(),
            None => Ok(()),
        }
    }

    fn close_write(&self) -> io::Result<()> {
        let mut output = self.output();
        if let Some(mut stdout) = output.take() {
            stdout.flush()?;
            // The reader of our standard output sees its end only when the
            // descriptor itself is closed.
            #[cfg(unix)]
            super::unix::close_stdout()?;
            #[cfg(windows)]
            super::windows::close_stdout()?;
        }
        Ok(())
    }

    fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        trace!("stdio connection ended");
        let _ = self.close_write();
        self.input.discard();
        self.ended.set();
    }

    fn kind(&self) -> &'static str {
        "stdio"
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
struct StdioListener {
    taken: bool,
    ended: Arc<Ended>,
    closed: Arc<AtomicBool>,
}

pub(crate) fn listen_stdio(_endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(StdioListener {
        taken: false,
        ended: Arc::new(Ended::default()),
        closed: Arc::new(AtomicBool::new(false)),
    }))
}

impl Listener for StdioListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if !self.taken {
            self.taken = true;
            let transport = StdioTransport {
                input: ThreadedReader::start("clink-stdin", io::stdin())?,
                output: Mutex::new(Some(io::stdout())),
                closed: AtomicBool::new(false),
                ended: Arc::clone(&self.ended),
            };
            trace!("stdio connection started");
            return Ok(Some(IpcStream::new(transport)));
        }
        // Only one connection: wait for it to end, then stop.
        self.ended.wait_until(&self.closed);
        Ok(None)
    }

    fn endpoint(&self) -> String {
        "stdio".to_string()
    }

    fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        let ended = Arc::clone(&self.ended);
        Arc::new(move || {
            closed.store(true, Ordering::SeqCst);
            ended.changed.notify_all();
        })
    }
}

// ---------------------------------------------------------------- exec

/// The argument vector of `exec:COMMAND`: a JSON array of strings, or words
/// separated by whitespace.
pub(crate) fn command_line(address: &str) -> io::Result<Vec<String>> {
    let address = address.trim();
    let argv = if address.starts_with('[') {
        parse_strings(address)?
    } else {
        address.split_whitespace().map(str::to_string).collect()
    };
    if argv.is_empty() || argv[0].is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "`exec:` needs a command",
        ));
    }
    Ok(argv)
}

struct ExecTransport {
    child: Mutex<Child>,
    input: Mutex<Option<ChildStdin>>,
    output: Arc<ThreadedReader>,
    closed: AtomicBool,
    program: String,
}

impl fmt::Debug for ExecTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecTransport")
            .field("program", &self.program)
            .finish_non_exhaustive()
    }
}

impl ExecTransport {
    fn input(&self) -> MutexGuard<'_, Option<ChildStdin>> {
        self.input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Transport for ExecTransport {
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        self.output.read(buf, timeout, &self.closed)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self.input().as_mut() {
            Some(input) => input.write(buf),
            None => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the outgoing direction was closed",
            )),
        }
    }

    fn flush(&self) -> io::Result<()> {
        match self.input().as_mut() {
            Some(input) => input.flush(),
            None => Ok(()),
        }
    }

    fn close_write(&self) -> io::Result<()> {
        drop(self.input().take());
        Ok(())
    }

    fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        drop(self.input().take());
        self.output.discard();
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    trace!("exec `{}` exited: {status}", self.program);
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    trace!("exec `{}` killed after 5 s", self.program);
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    }

    fn kind(&self) -> &'static str {
        "exec"
    }
}

impl Drop for ExecTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn connect_exec(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let argv = command_line(endpoint.address())?;
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| {
            io::Error::new(error.kind(), format!("cannot start `{}`: {error}", argv[0]))
        })?;
    let input = child.stdin.take();
    let output = child.stdout.take().expect("piped standard output");
    trace!("exec started `{}` as process {}", argv[0], child.id());
    Ok(IpcStream::new(ExecTransport {
        output: ThreadedReader::start("clink-exec-stdout", output)?,
        child: Mutex::new(child),
        input: Mutex::new(input),
        closed: AtomicBool::new(false),
        program: argv[0].clone(),
    }))
}
