//! `AF_UNIX` stream sockets on Windows 10 1803 and later, through Winsock.

use super::*;
use windows_sys::Win32::Networking::WinSock::{
    accept, bind, closesocket, connect, listen, recv, send, setsockopt, shutdown, WSAGetLastError,
    WSASocketW, WSAStartup, AF_UNIX, INVALID_SOCKET, SD_BOTH, SD_RECEIVE, SD_SEND, SOCKADDR,
    SOCKADDR_UN, SOCKET, SOCKET_ERROR, SOCK_STREAM, SOL_SOCKET, SO_RCVTIMEO, WSADATA,
    WSAECONNABORTED, WSAEINTR, WSAESHUTDOWN, WSA_FLAG_NO_HANDLE_INHERIT, WSA_FLAG_OVERLAPPED,
};

fn socket_error() -> io::Error {
    // SAFETY: reads the calling thread's last socket error.
    io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
}

fn winsock() -> io::Result<()> {
    static STARTED: OnceLock<i32> = OnceLock::new();
    let code = *STARTED.get_or_init(|| {
        // SAFETY: an all-zero `WSADATA` is a valid output buffer.
        let mut data: WSADATA = unsafe { std::mem::zeroed() };
        // SAFETY: Winsock 2.2; never cleaned up while the process runs.
        unsafe { WSAStartup(0x0202, &mut data) }
    });
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}

fn address(path: &str) -> io::Result<(SOCKADDR_UN, i32)> {
    let bytes = path.as_bytes();
    // SAFETY: an all-zero address is valid.
    let mut address: SOCKADDR_UN = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{path}` is not a valid socket path (at most 107 bytes, no NUL)"),
        ));
    }
    address.sun_family = AF_UNIX;
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as i8;
    }
    let len = std::mem::size_of_val(&address.sun_family) + bytes.len() + 1;
    Ok((address, len as i32))
}

/// A connected `AF_UNIX` stream socket.
#[derive(Debug)]
pub(super) struct WinUnixStream(SOCKET);

impl WinUnixStream {
    fn open() -> io::Result<Self> {
        winsock()?;
        // SAFETY: a plain stream socket; overlapped so receive timeouts work.
        let socket = unsafe {
            WSASocketW(
                i32::from(AF_UNIX),
                SOCK_STREAM,
                0,
                null(),
                0,
                WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT,
            )
        };
        if socket == INVALID_SOCKET {
            return Err(socket_error());
        }
        Ok(Self(socket))
    }

    fn connect(path: &str) -> io::Result<Self> {
        let (address, len) = address(path)?;
        let stream = Self::open()?;
        // SAFETY: a valid socket and address.
        let result = unsafe {
            connect(
                stream.0,
                (&address as *const SOCKADDR_UN).cast::<SOCKADDR>(),
                len,
            )
        };
        if result == SOCKET_ERROR {
            return Err(socket_error());
        }
        Ok(stream)
    }
}

impl Drop for WinUnixStream {
    fn drop(&mut self) {
        // SAFETY: a socket this value owns.
        unsafe { closesocket(self.0) };
    }
}

impl StreamSocket for WinUnixStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        let value: u32 = timeout.map_or(0, millis);
        // SAFETY: a valid socket and a `DWORD` option value.
        let result = unsafe {
            setsockopt(
                self.0,
                SOL_SOCKET,
                SO_RCVTIMEO,
                (&value as *const u32).cast::<u8>(),
                std::mem::size_of::<u32>() as i32,
            )
        };
        if result == SOCKET_ERROR {
            return Err(socket_error());
        }
        Ok(())
    }

    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(i32::MAX as usize) as i32;
        // SAFETY: a valid socket and buffer.
        let result = unsafe { recv(self.0, buf.as_mut_ptr(), len, 0) };
        if result == SOCKET_ERROR {
            let error = socket_error();
            return match error.raw_os_error() {
                Some(WSAESHUTDOWN | WSAECONNABORTED) => Ok(0),
                _ => Err(error),
            };
        }
        Ok(result as usize)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(i32::MAX as usize) as i32;
        // SAFETY: a valid socket and buffer.
        let result = unsafe { send(self.0, buf.as_ptr(), len, 0) };
        if result == SOCKET_ERROR {
            let error = socket_error();
            return match error.raw_os_error() {
                Some(WSAESHUTDOWN) => Err(broken_pipe()),
                _ => Err(error),
            };
        }
        Ok(result as usize)
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        // SAFETY: a valid socket.
        let result = unsafe { shutdown(self.0, how) };
        if how == SD_BOTH {
            // Wakes a blocked receive on another thread.
            // SAFETY: a socket is a file handle for cancellation.
            unsafe { CancelIoEx(self.0 as HANDLE, null()) };
        }
        if result == SOCKET_ERROR {
            return Err(socket_error());
        }
        Ok(())
    }
}

pub(crate) fn connect_unix(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let stream = WinUnixStream::connect(endpoint.address())?;
    Ok(IpcStream::new(SocketTransport::new(stream, "unix")))
}

#[derive(Debug)]
struct WinUnixListener {
    socket: WinUnixStream,
    name: String,
    path: PathBuf,
    closed: Arc<AtomicBool>,
}

pub(crate) fn listen_unix(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    let name = endpoint.address().to_string();
    if name.starts_with('@') {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "abstract `unix:@name` sockets are Linux-only",
        ));
    }
    let (address, len) = address(&name)?;
    let path = PathBuf::from(&name);
    if path.exists() {
        if WinUnixStream::connect(&name).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("a server already listens on `{name}`"),
            ));
        }
        trace!("unix removes the stale socket `{name}`");
        std::fs::remove_file(&path)?;
    }
    let socket = WinUnixStream::open()?;
    // SAFETY: a valid socket and address.
    if unsafe {
        bind(
            socket.0,
            (&address as *const SOCKADDR_UN).cast::<SOCKADDR>(),
            len,
        )
    } == SOCKET_ERROR
    {
        return Err(socket_error());
    }
    // SAFETY: a bound socket.
    if unsafe { listen(socket.0, 128) } == SOCKET_ERROR {
        let error = socket_error();
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(Box::new(WinUnixListener {
        socket,
        name,
        path,
        closed: Arc::new(AtomicBool::new(false)),
    }))
}

impl Listener for WinUnixListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        loop {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            // SAFETY: a listening socket; the peer address is not needed.
            let socket = unsafe { accept(self.socket.0, null_mut(), null_mut()) };
            if socket == INVALID_SOCKET {
                let error = socket_error();
                if self.closed.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                match error.raw_os_error() {
                    Some(WSAEINTR | WSAECONNABORTED) => continue,
                    _ => return Err(error),
                }
            }
            let stream = WinUnixStream(socket);
            if self.closed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            trace!("unix accepted on {}", self.name);
            return Ok(Some(IpcStream::new(SocketTransport::new(stream, "unix"))));
        }
    }

    fn endpoint(&self) -> String {
        format!("unix:{}", self.name)
    }

    fn closer(&self) -> Waker {
        let closed = Arc::clone(&self.closed);
        let name = self.name.clone();
        Arc::new(move || {
            if !closed.swap(true, Ordering::SeqCst) {
                let _ = WinUnixStream::connect(&name);
            }
        })
    }
}

impl Drop for WinUnixListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
