"""Minimal ALPC datagram ports; layout follows System Informer's phnt headers.

NtAlpc* is a Windows native API rather than a stable Win32 API. Keep the
adapter optional and test the real ABI on Windows. No handle passing,
impersonation, shared views, synchronous requests or undocumented syscalls.
"""

import ctypes as C
import threading
import time
import uuid
from ctypes import wintypes as W

from .streams import CHUNK

P = C.c_void_p
S = C.c_size_t
N = C.WinDLL("ntdll")
N.NtAlpcCreatePort.argtypes = [C.POINTER(W.HANDLE), P, P]
N.NtAlpcConnectPort.argtypes = [C.POINTER(W.HANDLE), P, P, P, W.ULONG, P, P, P, P, P, P]
N.NtAlpcAcceptConnectPort.argtypes = [
    C.POINTER(W.HANDLE),
    W.HANDLE,
    W.ULONG,
    P,
    P,
    P,
    P,
    P,
    W.BOOLEAN,
]
N.NtAlpcSendWaitReceivePort.argtypes = [W.HANDLE, W.ULONG, P, P, P, P, P, P]
N.NtAlpcDisconnectPort.argtypes = [W.HANDLE, W.ULONG]
N.NtClose.argtypes = [W.HANDLE]
for _name in (
    "NtAlpcCreatePort",
    "NtAlpcConnectPort",
    "NtAlpcAcceptConnectPort",
    "NtAlpcSendWaitReceivePort",
    "NtAlpcDisconnectPort",
    "NtClose",
):
    getattr(N, _name).restype = C.c_long


class UnicodeString(C.Structure):
    _fields_ = [("length", W.USHORT), ("maximum", W.USHORT), ("buffer", P)]


class ObjectAttributes(C.Structure):
    _fields_ = [
        ("length", W.ULONG),
        ("root", W.HANDLE),
        ("name", C.POINTER(UnicodeString)),
        ("attributes", W.ULONG),
        ("security", P),
        ("qos", P),
    ]


class QoS(C.Structure):
    _fields_ = [
        ("length", W.ULONG),
        ("level", C.c_int),
        ("tracking", W.BYTE),
        ("effective", W.BYTE),
    ]


class PortAttributes(C.Structure):
    _fields_ = [
        ("flags", W.ULONG),
        ("qos", QoS),
        ("maximum", S),
        ("bandwidth", S),
        ("pool", S),
        ("section", S),
        ("view", S),
        ("total", S),
        ("duplicates", W.ULONG),
        ("reserved", W.ULONG),
    ]


class PortMessage(C.Structure):
    _fields_ = [
        ("data_length", W.USHORT),
        ("total_length", W.USHORT),
        ("kind", W.USHORT),
        ("data_offset", W.USHORT),
        ("process", P),
        ("thread", P),
        ("message_id", W.ULONG),
        ("view", S),
    ]


def check(status):
    if status == 0x102:
        raise TimeoutError("ALPC operation timed out; outcome unknown")
    if status < 0:
        raise OSError(f"ALPC NTSTATUS 0x{status & 0xFFFFFFFF:08X}; outcome unknown")
    return status


def port_name(text):
    buffer = C.create_unicode_buffer(text)
    encoded = text.encode("utf-16-le")
    return buffer, UnicodeString(len(encoded), len(encoded) + 2, C.cast(buffer, P))


class Alpc:
    def __init__(self, session, peer):
        if C.sizeof(P) != 8:
            raise OSError("ALPC adapter requires 64-bit Windows")
        self.session = session
        self.address = "\\RPC Control\\clink-ipc-" + uuid.uuid4().hex
        self.port, self.outgoing, self.accepted = W.HANDLE(), W.HANDLE(), W.HANDLE()
        self.accept_thread = None
        self.error = None
        self.attrs = PortAttributes()
        self.attrs.qos.length = C.sizeof(QoS)
        self.attrs.maximum = CHUNK + C.sizeof(PortMessage)
        self.attrs.pool = 65536
        buffer, name = port_name(self.address)
        obj = ObjectAttributes(
            C.sizeof(ObjectAttributes), None, C.pointer(name), 0x40, None, None
        )
        check(N.NtAlpcCreatePort(C.byref(self.port), C.byref(obj), C.byref(self.attrs)))
        if peer:
            self.accept_thread = threading.Thread(target=self._accept_in_thread)
            self.accept_thread.start()

    def _receive_raw(self, timeout):
        buffer = C.create_string_buffer(CHUNK + C.sizeof(PortMessage))
        size = S(len(buffer))
        interval = C.c_longlong(-max(1, int(timeout * 10_000_000)))
        status = N.NtAlpcSendWaitReceivePort(
            self.port, 0, None, None, buffer, C.byref(size), None, C.byref(interval)
        )
        if status == 0x102:  # STATUS_TIMEOUT
            return None
        check(status)
        header = PortMessage.from_buffer(buffer)
        if (
            header.data_length > CHUNK
            or header.total_length != C.sizeof(PortMessage) + header.data_length
        ):
            raise OSError("invalid ALPC message length")
        return buffer

    def _accept_in_thread(self):
        try:
            self.accept_peer()
        except OSError as error:
            self.error = error

    def accept_peer(self):
        end = time.monotonic() + self.session.options.timeout
        while not self.session.closed.is_set():
            message = self._receive_raw(0.1)
            if message:
                header = PortMessage.from_buffer(message)
                if header.kind & 0xFFF != 10:
                    raise OSError("expected ALPC connection request")
                check(
                    N.NtAlpcAcceptConnectPort(
                        C.byref(self.accepted),
                        self.port,
                        0,
                        None,
                        C.byref(self.attrs),
                        None,
                        message,
                        None,
                        True,
                    )
                )
                return
            if time.monotonic() >= end:
                raise TimeoutError("ALPC connection accept timed out")

    def set_peer(self, address):
        if not isinstance(address, str) or not address.startswith(
            "\\RPC Control\\clink-ipc-"
        ):
            raise ValueError("invalid ALPC peer port")
        buffer, name = port_name(address)
        interval = C.c_longlong(-int(self.session.options.timeout * 10_000_000))
        check(
            N.NtAlpcConnectPort(
                C.byref(self.outgoing),
                C.byref(name),
                None,
                C.byref(self.attrs),
                0,
                None,
                None,
                None,
                None,
                None,
                C.byref(interval),
            )
        )

    def send(self, data):
        self.session.check()
        buffer = C.create_string_buffer(C.sizeof(PortMessage) + len(data))
        header = PortMessage.from_buffer(buffer)
        header.data_length, header.total_length, header.kind = len(data), len(buffer), 3
        C.memmove(C.addressof(buffer) + C.sizeof(PortMessage), data, len(data))
        interval = C.c_longlong(-int(self.session.options.timeout * 10_000_000))
        check(
            N.NtAlpcSendWaitReceivePort(
                self.outgoing, 0, buffer, None, None, None, None, C.byref(interval)
            )
        )

    def receive(self):
        if self.accept_thread:
            self.accept_thread.join()
            self.accept_thread = None
        while True:
            if self.error:
                raise self.error
            message = self._receive_raw(0.1)
            if message:
                header = PortMessage.from_buffer(message)
                if header.kind & 0xFFF in (5, 6):
                    raise ConnectionResetError("ALPC port closed")
                return message.raw[C.sizeof(PortMessage) : header.total_length]
            self.session.check()

    def close(self):
        if self.accept_thread:
            self.accept_thread.join()
            self.accept_thread = None
        for handle in (self.outgoing, self.accepted, self.port):
            if handle.value:
                N.NtAlpcDisconnectPort(handle, 0)
                N.NtClose(handle)
                handle.value = None
