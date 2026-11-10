"""Windows native payload adapters. Imported only when explicitly selected.

All handle and pointer signatures are declared for 64-bit correctness. Local
mailslots and WM_COPYDATA run as paired one-way channels. Neither broadcasts
payloads. ALPC uses two private ports and one-way messages, avoiding the
request/reply coupling that would otherwise prevent subscription pushes.
"""

import ctypes as C
import hashlib
import queue
import threading
import time
import uuid
from ctypes import wintypes as W

from .local import wait_tick
from .streams import CHUNK

K = C.WinDLL("kernel32", use_last_error=True)
U = C.WinDLL("user32", use_last_error=True)
PTR = C.c_void_p
SIZE = C.c_size_t
LRESULT = C.c_ssize_t
K.CloseHandle.argtypes = [W.HANDLE]
K.CloseHandle.restype = W.BOOL
K.CreateFileW.argtypes = [W.LPCWSTR, W.DWORD, W.DWORD, PTR, W.DWORD, W.DWORD, W.HANDLE]
K.CreateFileW.restype = W.HANDLE
K.ReadFile.argtypes = [W.HANDLE, PTR, W.DWORD, C.POINTER(W.DWORD), PTR]
K.WriteFile.argtypes = [W.HANDLE, PTR, W.DWORD, C.POINTER(W.DWORD), PTR]
K.CreateMailslotW.argtypes = [W.LPCWSTR, W.DWORD, W.DWORD, PTR]
K.CreateMailslotW.restype = W.HANDLE


def checked(value):
    if not value or value == C.c_void_p(-1).value:
        raise C.WinError(C.get_last_error())
    return value


class RawMailslot:
    def __init__(self, session):
        self.session = session
        self.address = rf"\\.\mailslot\clink-ipc-{uuid.uuid4().hex}"
        self.reader = checked(K.CreateMailslotW(self.address, CHUNK + 64, 100, None))
        self.writer = None

    def set_peer(self, address):
        if not isinstance(address, str) or not address.startswith(
            "\\\\.\\mailslot\\clink-ipc-"
        ):
            raise ValueError("invalid local mailslot peer")
        self.writer = checked(K.CreateFileW(address, 0x40000000, 3, None, 3, 0, None))

    def settimeout(self, _):
        pass  # ReadFile uses the finite timeout supplied to CreateMailslotW.

    def sendto(self, data, _):
        self.session.check()
        count = W.DWORD()
        checked(K.WriteFile(self.writer, data, len(data), C.byref(count), None))
        if count.value != len(data):
            raise OSError("short IPC mailslot write; outcome unknown")

    def recvfrom(self, limit):
        buffer, count = C.create_string_buffer(limit), W.DWORD()
        if K.ReadFile(self.reader, buffer, len(buffer), C.byref(count), None):
            return buffer.raw[: count.value], 0
        code = C.get_last_error()
        if code == 121:
            raise TimeoutError("mailslot read poll")
        raise C.WinError(code)

    def close(self):
        for handle in (self.reader, self.writer):
            if handle:
                K.CloseHandle(handle)
        self.reader = self.writer = None


class Mailslot:
    def __init__(self, session):
        self.session, self.raw = session, RawMailslot(session)
        self.address, self.reliable = self.raw.address, None

    def set_peer(self, address):
        from .udp import UdpChannel

        self.raw.set_peer(address)
        token = hashlib.sha256(str(self.session.root).encode()).digest()[:16]
        # ACKs, sequence numbers and a single receive slot bound buffering
        # even though the native mailslot itself has an unbounded queue.
        self.reliable = UdpChannel(self.raw, 0, token, self.session.options)

    def send(self, data):
        self.reliable.send(data)

    def receive(self):
        return self.reliable.receive()

    def close(self):
        if self.reliable:
            self.reliable.close()
        else:
            self.raw.close()


WNDPROC = C.WINFUNCTYPE(LRESULT, W.HWND, W.UINT, SIZE, C.c_ssize_t)


class WindowClass(C.Structure):
    _fields_ = [
        ("style", W.UINT),
        ("procedure", WNDPROC),
        ("class_extra", C.c_int),
        ("window_extra", C.c_int),
        ("instance", W.HINSTANCE),
        ("icon", W.HICON),
        ("cursor", W.HANDLE),
        ("background", W.HBRUSH),
        ("menu", W.LPCWSTR),
        ("name", W.LPCWSTR),
    ]


class CopyDataStruct(C.Structure):
    _fields_ = [("tag", SIZE), ("size", W.DWORD), ("data", PTR)]


U.RegisterClassW.argtypes = [C.POINTER(WindowClass)]
U.RegisterClassW.restype = W.ATOM
U.UnregisterClassW.argtypes = [W.LPCWSTR, W.HINSTANCE]
U.CreateWindowExW.argtypes = [
    W.DWORD,
    W.LPCWSTR,
    W.LPCWSTR,
    W.DWORD,
    C.c_int,
    C.c_int,
    C.c_int,
    C.c_int,
    W.HWND,
    W.HMENU,
    W.HINSTANCE,
    PTR,
]
U.CreateWindowExW.restype = W.HWND
U.DefWindowProcW.argtypes = [W.HWND, W.UINT, SIZE, C.c_ssize_t]
U.DefWindowProcW.restype = LRESULT
U.DestroyWindow.argtypes = [W.HWND]
U.PostMessageW.argtypes = [W.HWND, W.UINT, SIZE, C.c_ssize_t]
U.GetMessageW.argtypes = [C.POINTER(W.MSG), W.HWND, W.UINT, W.UINT]
U.TranslateMessage.argtypes = [C.POINTER(W.MSG)]
U.DispatchMessageW.argtypes = [C.POINTER(W.MSG)]
U.DispatchMessageW.restype = LRESULT
U.SendMessageTimeoutW.argtypes = [
    W.HWND,
    W.UINT,
    SIZE,
    C.c_ssize_t,
    W.UINT,
    W.UINT,
    C.POINTER(SIZE),
]
U.SendMessageTimeoutW.restype = LRESULT


class CopyData:
    def __init__(self, session):
        self.session, self.peer = session, None
        self.incoming = queue.Queue(maxsize=1)
        self.ready = threading.Event()
        self.address = self.error = None
        self.callback = WNDPROC(self._procedure)
        self.thread = threading.Thread(target=self._window)
        self.thread.start()
        if not self.ready.wait(session.options.timeout):
            raise TimeoutError("IPC WM_COPYDATA window creation stalled")
        if self.error:
            raise self.error

    def _procedure(self, window, message, wparam, lparam):
        if message == 0x4A:  # WM_COPYDATA: copy while sender memory is valid.
            packet = C.cast(lparam, C.POINTER(CopyDataStruct)).contents
            if packet.tag != 0x4C495031 or packet.size > CHUNK:
                return 0
            try:
                self.incoming.put_nowait(C.string_at(packet.data, packet.size))
                return 1
            except queue.Full:
                return 0  # Sender may retry: nothing was accepted.
        if message == 0x10:  # WM_CLOSE, on the owning window thread.
            U.DestroyWindow(window)
            return 0
        if message == 2:  # WM_DESTROY
            U.PostQuitMessage(0)
            return 0
        return U.DefWindowProcW(window, message, wparam, lparam)

    def _window(self):
        name = "ClinkIPC_" + uuid.uuid4().hex
        registered = False
        try:
            cls = WindowClass()
            cls.procedure, cls.name = self.callback, name
            checked(U.RegisterClassW(C.byref(cls)))
            registered = True
            self.address = checked(
                U.CreateWindowExW(0, name, name, 0, 0, 0, 0, 0, None, None, None, None)
            )
            self.ready.set()
            message = W.MSG()
            while True:
                result = U.GetMessageW(C.byref(message), None, 0, 0)
                if result == -1:
                    raise C.WinError(C.get_last_error())
                if result == 0:
                    break
                U.TranslateMessage(C.byref(message))
                U.DispatchMessageW(C.byref(message))
        except OSError as error:
            self.error = error
            self.ready.set()
        finally:
            if registered:
                U.UnregisterClassW(name, None)

    def set_peer(self, address):
        if not isinstance(address, int) or address <= 0:
            raise ValueError("invalid WM_COPYDATA peer window")
        self.peer = address

    def send(self, data):
        buffer = C.create_string_buffer(data)
        packet = CopyDataStruct(0x4C495031, len(data), C.cast(buffer, PTR))
        end = time.monotonic() + self.session.options.timeout
        while True:
            self.session.check()
            result = SIZE()
            # A timeout has an unknown delivery outcome. Never retry it.
            checked(
                U.SendMessageTimeoutW(
                    self.peer,
                    0x4A,
                    0,
                    C.addressof(packet),
                    2,
                    max(1, int(self.session.options.timeout * 1000)),
                    C.byref(result),
                )
            )
            if result.value:
                return
            if time.monotonic() >= end:
                raise TimeoutError("IPC WM_COPYDATA receiver full")
            wait_tick(self.session.options.poll)

    def receive(self):
        while True:
            try:
                return self.incoming.get(timeout=0.1)
            except queue.Empty:
                self.session.check()
                if self.error:
                    raise self.error from None

    def close(self):
        if self.address:
            U.PostMessageW(self.address, 0x10, 0, 0)
            self.thread.join()
            self.address = None


def native_channel(kind, session, native):
    if kind == "copydata":
        return CopyData(session)
    if kind == "mailslot":
        return Mailslot(session)
    if kind == "alpc":
        from .windows_alpc import Alpc

        return Alpc(session, native)
    raise ValueError(f"unknown Windows native IPC kind: {kind}")
