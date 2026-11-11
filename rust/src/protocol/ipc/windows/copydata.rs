//! `WM_COPYDATA`: a hidden message-only window per side, each message
//! carrying one datagram.

use super::*;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, ERROR_INVALID_WINDOW_HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SendMessageTimeoutW, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG,
    SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_CLOSE, WM_COPYDATA, WM_DESTROY, WM_NCDESTROY, WNDCLASSW,
};

/// `cds.dwData` of every frame: `LIP1`.
const MAGIC: usize = 0x4C49_5031;
/// Frames waiting to be received before more are refused.
const QUEUE: usize = 64;

#[cfg(target_pointer_width = "64")]
unsafe fn set_user_data(hwnd: HWND, value: isize) -> isize {
    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, value)
}
#[cfg(target_pointer_width = "64")]
unsafe fn user_data(hwnd: HWND) -> isize {
    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
}
#[cfg(not(target_pointer_width = "64"))]
unsafe fn set_user_data(hwnd: HWND, value: isize) -> isize {
    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongW(hwnd, GWLP_USERDATA, value as i32)
        as isize
}
#[cfg(not(target_pointer_width = "64"))]
unsafe fn user_data(hwnd: HWND) -> isize {
    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongW(hwnd, GWLP_USERDATA) as isize
}

/// Frames the window procedure took, for the session's receiver.
#[derive(Debug)]
struct Inbox {
    frames: Mutex<VecDeque<Vec<u8>>>,
    changed: Condvar,
    /// The only window whose frames are taken; 0 until the handshake.
    peer: AtomicIsize,
    largest: usize,
}

impl Inbox {
    fn lock(&self) -> MutexGuard<'_, VecDeque<Vec<u8>>> {
        self.frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn class_name() -> &'static [u16] {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    NAME.get_or_init(|| "clink-copydata".encode_utf16().chain(Some(0)).collect())
}

fn register_class() -> io::Result<()> {
    static REGISTERED: OnceLock<u32> = OnceLock::new();
    let code = *REGISTERED.get_or_init(|| {
        // SAFETY: an all-zero class with a procedure, instance and name.
        let mut class: WNDCLASSW = unsafe { std::mem::zeroed() };
        class.lpfnWndProc = Some(window_procedure);
        // SAFETY: this module's instance.
        class.hInstance = unsafe { GetModuleHandleW(null()) };
        class.lpszClassName = class_name().as_ptr();
        // SAFETY: a valid class description.
        if unsafe { RegisterClassW(&class) } == 0 {
            match last_code() {
                ERROR_CLASS_ALREADY_EXISTS => 0,
                code => code,
            }
        } else {
            0
        }
    });
    if code == 0 {
        Ok(())
    } else {
        Err(code_error(code))
    }
}

unsafe extern "system" fn window_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COPYDATA => {
            let pointer = user_data(hwnd);
            if pointer == 0 || lparam == 0 {
                return 0;
            }
            let inbox = &*(pointer as *const Inbox);
            let data = &*(lparam as *const COPYDATASTRUCT);
            let peer = inbox.peer.load(Ordering::SeqCst);
            if data.dwData != MAGIC
                || peer == 0
                || wparam as isize != peer
                || data.cbData as usize > inbox.largest
            {
                return 0;
            }
            let frame = if data.cbData == 0 || data.lpData.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize).to_vec()
            };
            let mut frames = inbox.lock();
            if frames.len() >= QUEUE {
                return 0;
            }
            frames.push_back(frame);
            inbox.changed.notify_all();
            1
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let pointer = set_user_data(hwnd, 0);
            if pointer != 0 {
                drop(Arc::from_raw(pointer as *const Inbox));
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// A hidden message-only window on a message-loop thread of its own.
#[derive(Debug)]
struct Window {
    hwnd: isize,
    inbox: Arc<Inbox>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Window {
    fn spawn(largest: usize) -> io::Result<Self> {
        register_class()?;
        let inbox = Arc::new(Inbox {
            frames: Mutex::new(VecDeque::new()),
            changed: Condvar::new(),
            peer: AtomicIsize::new(0),
            largest,
        });
        let (created, creation) = mpsc::channel::<io::Result<isize>>();
        let shared = Arc::clone(&inbox);
        let thread = thread::Builder::new()
            .name("clink-copydata".to_string())
            .spawn(move || {
                let pointer = Arc::into_raw(shared);
                let name = class_name().as_ptr();
                // SAFETY: a registered class; `HWND_MESSAGE` makes the
                // window message-only.
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        name,
                        name,
                        0,
                        0,
                        0,
                        0,
                        0,
                        HWND_MESSAGE,
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    )
                };
                if hwnd.is_null() {
                    let _ = created.send(Err(last_error()));
                    // SAFETY: the reference this thread took.
                    drop(unsafe { Arc::from_raw(pointer) });
                    return;
                }
                // SAFETY: the window owns the reference from now on and
                // releases it at `WM_NCDESTROY`.
                unsafe { set_user_data(hwnd, pointer as isize) };
                let _ = created.send(Ok(hwnd as isize));
                // SAFETY: an all-zero message is a valid output buffer.
                let mut message: MSG = unsafe { std::mem::zeroed() };
                // SAFETY: this thread's message queue.
                while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
                    // SAFETY: a message just received.
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            })?;
        let hwnd = creation.recv().map_err(|_| {
            io::Error::other("the copydata window thread ended before creating its window")
        })??;
        Ok(Self {
            hwnd,
            inbox,
            thread: Mutex::new(Some(thread)),
        })
    }

    fn close(&self) {
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(thread) = thread {
            // SAFETY: asks the window to destroy itself on its thread.
            unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
            if thread.thread().id() != thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug)]
struct CopydataLink {
    window: Window,
    peer_hwnd: isize,
    peer: PeerWatch,
}

impl Datagrams for CopydataLink {
    fn send(&self, frame: &[u8]) -> io::Result<bool> {
        let data = COPYDATASTRUCT {
            dwData: MAGIC,
            cbData: frame.len() as u32,
            lpData: frame.as_ptr() as *mut c_void,
        };
        let mut result = 0usize;
        // SAFETY: the system copies `data` into the peer's process
        // before this call returns.
        let sent = unsafe {
            SendMessageTimeoutW(
                self.peer_hwnd as HWND,
                WM_COPYDATA,
                self.window.hwnd as WPARAM,
                &data as *const COPYDATASTRUCT as LPARAM,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                1000,
                &mut result,
            )
        };
        if sent == 0 {
            return match last_code() {
                ERROR_INVALID_WINDOW_HANDLE => Err(hung_up()),
                _ => Ok(false),
            };
        }
        Ok(result == 1)
    }

    fn recv(&self, buf: &mut Vec<u8>, timeout: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + timeout;
        let inbox = &self.window.inbox;
        let mut frames = inbox.lock();
        loop {
            if let Some(frame) = frames.pop_front() {
                *buf = frame;
                return Ok(true);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            frames = inbox
                .changed
                .wait_timeout(frames, deadline - now)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    fn peer_alive(&self) -> bool {
        self.peer.alive()
    }

    fn close(&self) {
        self.window.close();
    }
}

fn peer_window(fields: &Object) -> io::Result<isize> {
    let hwnd = fields.require_number("hwnd")? as usize as isize;
    if hwnd == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the peer sent no window",
        ));
    }
    Ok(hwnd)
}

pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<IpcStream> {
    let rendezvous = Rendezvous::begin(endpoint, "copydata")?;
    let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
    let window = Window::spawn(own_chunk as usize + HEADER)?;
    let ack = rendezvous.request(
        Object::new()
            .with("chunk", own_chunk)
            .with("hwnd", window.hwnd as usize as u64),
    )?;
    let peer_hwnd = peer_window(&ack)?;
    window.inbox.peer.store(peer_hwnd, Ordering::SeqCst);
    let chunk = agree_chunk(own_chunk, ack.number("chunk"));
    let peer = ack
        .number("pid")
        .map_or(rendezvous.server_pid, |pid| pid as u32);
    let link = CopydataLink {
        window,
        peer_hwnd,
        peer: PeerWatch::new(peer),
    };
    let session = Session::start("copydata", Arc::new(link), local_settings(endpoint, chunk)?)?;
    trace!("copydata connected as {}, chunk {chunk}", rendezvous.id);
    Ok(IpcStream::new(session))
}

#[derive(Debug)]
struct CopydataListener {
    rendezvous: RendezvousListener,
    endpoint: Endpoint,
}

pub(crate) fn listen(endpoint: &Endpoint) -> io::Result<Box<dyn Listener>> {
    Ok(Box::new(CopydataListener {
        rendezvous: RendezvousListener::bind(endpoint, "copydata")?,
        endpoint: endpoint.clone(),
    }))
}

impl Listener for CopydataListener {
    fn accept(&mut self) -> io::Result<Option<IpcStream>> {
        let endpoint = self.endpoint.clone();
        accept_with(&self.rendezvous, "copydata", |request| {
            let own_chunk = endpoint.number("chunk", 4096, 16, 65000)?;
            let chunk = agree_chunk(own_chunk, request.fields.number("chunk"));
            let peer_hwnd = peer_window(&request.fields)?;
            let window = Window::spawn(own_chunk as usize + HEADER)?;
            window.inbox.peer.store(peer_hwnd, Ordering::SeqCst);
            let hwnd = window.hwnd;
            let link = CopydataLink {
                window,
                peer_hwnd,
                peer: PeerWatch::new(request.pid()),
            };
            let session = Session::start(
                "copydata",
                Arc::new(link),
                local_settings(&endpoint, chunk)?,
            )?;
            Ok((
                Object::new()
                    .with("chunk", chunk as u64)
                    .with("hwnd", hwnd as usize as u64),
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
