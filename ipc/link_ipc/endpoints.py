"""Endpoint factories. Native local payloads never traverse the control pipe."""

import contextlib
import hashlib
import json
import math
import os
import select
import shutil
import socket
import struct
import sys
import tempfile
import time
import uuid
from dataclasses import dataclass, replace
from multiprocessing.connection import Client, Listener
from pathlib import Path

from .local import Mailboxes, Session, enable_signals, receive_json, send_json
from .posix import MQ, Fifo, ManagedChannel
from .streams import (
    ConnectionChannel,
    FileStream,
    MessageStream,
    SocketStream,
    child_stream,
)

LOCAL = {"shm", "mmap", "file", "signal", "fifo", "mq", "copydata", "mailslot", "alpc"}


@dataclass(frozen=True)
class Options:
    timeout: float = 30
    poll: float = 0.01
    kind: str = ""

    def __post_init__(self):
        if not math.isfinite(self.timeout) or self.timeout <= 0:
            raise ValueError("timeout must be finite and positive")
        if not math.isfinite(self.poll) or not 0.001 <= self.poll <= 1:
            raise ValueError("poll must be between 0.001 and 1 second")


DEFAULT_OPTIONS = Options()
_CONNECTORS = {}
_LISTENERS = {}


def register(kind, *, connector, listener):
    """Register an explicit application-owned adapter, without global discovery.

    Factories receive (address, Options), returning a Stream or a listener
    with address/accept/close. Both directions must retain the Stream contract.
    Built-in names cannot be replaced accidentally.
    """
    if (
        not kind.isidentifier()
        or kind in LOCAL | {"tcp", "unix", "udp", "pipe", "stdio", "exec"}
        or kind in _CONNECTORS
    ):
        raise ValueError("IPC kind is invalid or already registered")
    if not callable(connector) or not callable(listener):
        raise TypeError("IPC adapter factories must be callable")
    _CONNECTORS[kind], _LISTENERS[kind] = connector, listener


def split_endpoint(endpoint):
    kind, separator, address = endpoint.partition(":")
    if not separator or not kind or (not address and kind != "stdio"):
        raise ValueError("endpoint must be kind:address")
    if kind == "pipe" and os.name != "nt":
        kind = "fifo"
    if kind == "signal":
        enable_signals()
    if kind in {"copydata", "mailslot", "alpc", "pipe"} and os.name != "nt":
        raise OSError(f"{kind} requires Windows")
    return kind, address


def host_port(address):
    host, sep, port = address.rpartition(":")
    if not sep or not host:
        raise ValueError("socket address must be host:port (IPv6: [::1]:port)")
    number = int(port)
    if not 0 <= number <= 65535:
        raise ValueError("port out of range")
    return host.removeprefix("[").removesuffix("]"), number


def pipe_name(address):
    digest = hashlib.sha256(os.path.abspath(address).encode()).hexdigest()[:32]
    return rf"\\.\pipe\clink-ipc-{digest}"


class SocketListener:
    def __init__(self, kind, address, options):
        self.options, self.path, self.closed = options, None, False
        if kind == "unix":
            self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.socket.bind(address)  # Never unlink an existing user's endpoint.
            self.path = Path(address)
            os.chmod(address, 0o600)
            self.address = f"unix:{address}"
        else:
            host, port = host_port(address)
            self.socket = socket.socket(
                socket.AF_INET6 if ":" in host else socket.AF_INET
            )
            self.socket.bind((host, port))
            actual = self.socket.getsockname()
            self.address = f"tcp:{'[' + actual[0] + ']' if ':' in actual[0] else actual[0]}:{actual[1]}"
        self.socket.listen(16)
        self.socket.settimeout(0.2)

    def accept(self):
        while not self.closed:
            try:
                sock, _ = self.socket.accept()
                return SocketStream(sock, self.options.timeout)
            except TimeoutError:
                continue
        raise OSError("IPC listener closed")

    def close(self):
        self.closed = True
        self.socket.close()
        if self.path:
            self.path.unlink(missing_ok=True)


class SocketControl:
    """Bounded framing for local configuration; never deserialize pickle."""

    def __init__(self, sock, timeout):
        self.socket, self.timeout = sock, timeout
        sock.settimeout(timeout)

    def send_bytes(self, data):
        self.socket.sendall(struct.pack("<I", len(data)) + data)

    def poll(self, timeout):
        return bool(select.select([self.socket], [], [], timeout)[0])

    def _read(self, count):
        data = bytearray()
        end = time.monotonic() + self.timeout
        while len(data) < count:
            if time.monotonic() >= end:
                raise TimeoutError("incomplete IPC control frame")
            part = self.socket.recv(count - len(data))
            if not part:
                raise EOFError
            data.extend(part)
        return bytes(data)

    def recv_bytes(self, limit):
        count = struct.unpack("<I", self._read(4))[0]
        if count > limit:
            raise OSError("IPC control frame exceeds limit")
        return self._read(count)

    def close(self):
        with contextlib.suppress(OSError):
            self.socket.shutdown(socket.SHUT_RDWR)
        self.socket.close()


class NativeListener:
    def __init__(self, kind, address, options):
        self.kind, self.options = kind, replace(options, kind=kind)
        self.root = Path(address).absolute()
        self.root.mkdir(mode=0o700)  # Exclusive ownership, never remove stale data.
        self.address = f"{kind}:{self.root}"
        try:
            if os.name == "nt":
                self.control = Listener(pipe_name(str(self.root)), family="AF_PIPE")
            else:
                self.control = SocketListener(
                    "unix", str(self.root / "control"), options
                )
        except BaseException:
            self.root.rmdir()
            raise

    def _accept_control(self):
        if os.name == "nt":
            return self.control.accept()
        stream = self.control.accept()
        return SocketControl(stream.socket, self.options.timeout)

    def accept(self):
        control = self._accept_control()
        root = Path(tempfile.mkdtemp(prefix="session-", dir=self.root))
        session = Session(control, self.options, root, 0, owner=True)
        names = ["clink_" + uuid.uuid4().hex for _ in range(2)]
        if self.kind == "mq":
            names = ["/" + name for name in names]
        try:
            channel = make_channel(session, names, True)
            meta = {
                "version": 1,
                "kind": self.kind,
                "root": str(root),
                "names": names,
                "pid": os.getpid(),
                "native": getattr(channel, "address", None),
            }
            send_json(control, meta)
            # ALPC's connection handshake needs the server to receive while
            # the client is connecting; other natives need the peer address.
            if hasattr(channel, "accept_peer"):
                channel.accept_peer()
            peer = receive_json(control, self.options.timeout)
            session.peer_pid = checked_pid(peer["pid"])
            if hasattr(channel, "set_peer"):
                channel.set_peer(peer["native"])
            if hasattr(channel, "activate"):
                channel.activate()
            send_json(control, {"ready": True})
            session.start()
            return MessageStream(manage(channel, session))
        except BaseException:
            session.close()
            raise

    def close(self):
        self.control.close()
        # Windows cannot unlink files while the peer still has mappings or
        # lock handles open. EOF can reach us just before that peer closes.
        # Wait for actual handle release, bounded by the configured deadline.
        end = time.monotonic() + self.options.timeout
        while True:
            try:
                shutil.rmtree(self.root)
                return
            except OSError:
                if not self.root.exists():
                    return
                if os.name != "nt" or time.monotonic() >= end:
                    raise
                time.sleep(self.options.poll)


def checked_pid(pid):
    if not isinstance(pid, int) or pid <= 1:
        raise ValueError("invalid peer pid")
    return pid


def make_channel(session, names, create, native=None):
    kind = session.options.kind
    if kind in ("shm", "mmap", "file", "signal"):
        return Mailboxes(session, names, create)
    if kind == "fifo":
        channel = Fifo(session, names, create)
    elif kind == "mq":
        channel = MQ(session, names, create)
    else:
        from .windows import native_channel

        channel = native_channel(kind, session, native)
    session.resources.append(channel)
    return channel


def manage(channel, session):
    if isinstance(channel, Mailboxes):
        return channel
    # The native resource was already registered before the handshake.
    managed = object.__new__(ManagedChannel)
    managed.channel, managed.session = channel, session
    return managed


def connect_local(kind, address, options):
    root = Path(address).absolute()
    if os.name == "nt":
        control = Client(pipe_name(str(root)), family="AF_PIPE")
    else:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(options.timeout)
        try:
            sock.connect(str(root / "control"))
        except BaseException:
            sock.close()
            raise
        control = SocketControl(sock, options.timeout)
    session = None
    try:
        meta = receive_json(control, options.timeout)
        if meta.get("version") != 1 or meta.get("kind") != kind:
            raise OSError("IPC endpoint kind/version mismatch")
        # Trust is confined to the explicitly selected, owner-controlled local
        # endpoint. No remote paths or names are accepted via TCP/UDP.
        session = Session(
            control, replace(options, kind=kind), meta["root"], checked_pid(meta["pid"])
        )
        channel = make_channel(
            session, list(reversed(meta["names"])), False, meta["native"]
        )
        if hasattr(channel, "set_peer"):
            channel.set_peer(meta["native"])
        send_json(
            control, {"pid": os.getpid(), "native": getattr(channel, "address", None)}
        )
        if receive_json(control, options.timeout) != {"ready": True}:
            raise OSError("IPC peer did not complete handshake")
        if hasattr(channel, "activate"):
            channel.activate()
        session.start()
        return MessageStream(manage(channel, session))
    except BaseException:
        if session:
            session.close()
        else:
            control.close()
        raise


class PipeListener:
    def __init__(self, address):
        self.listener = Listener(pipe_name(address), family="AF_PIPE")
        self.address = f"pipe:{address}"

    def accept(self):
        return MessageStream(ConnectionChannel(self.listener.accept()))

    def close(self):
        self.listener.close()


class StdioListener:
    address = "stdio:"

    def __init__(self, options):
        self.used = False
        self.options = options

    def accept(self):
        if self.used:
            raise OSError("stdio accepts one connection")
        self.used = True
        return FileStream(
            sys.stdin.buffer, sys.stdout.buffer, timeout=self.options.timeout
        )

    def close(self):
        pass


def listen(endpoint, options=DEFAULT_OPTIONS):
    kind, address = split_endpoint(endpoint)
    if kind in _LISTENERS:
        return _LISTENERS[kind](address, options)
    if kind in ("tcp", "unix"):
        return SocketListener(kind, address, options)
    if kind in LOCAL:
        return NativeListener(kind, address, options)
    if kind == "pipe":
        return PipeListener(address)
    if kind == "stdio":
        return StdioListener(options)
    if kind == "udp":
        from .udp import UdpListener

        return UdpListener(address, options)
    raise ValueError(f"unknown listener kind: {kind}")


def connect(endpoint, options=DEFAULT_OPTIONS):
    kind, address = split_endpoint(endpoint)
    if kind in _CONNECTORS:
        return _CONNECTORS[kind](address, options)
    if kind == "tcp":
        return SocketStream(
            socket.create_connection(host_port(address), options.timeout),
            options.timeout,
        )
    if kind == "unix":
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(options.timeout)
        try:
            sock.connect(address)
            return SocketStream(sock, options.timeout)
        except BaseException:
            sock.close()
            raise
    if kind in LOCAL:
        return connect_local(kind, address, options)
    if kind == "pipe":
        return MessageStream(
            ConnectionChannel(Client(pipe_name(address), family="AF_PIPE"))
        )
    if kind == "exec":
        argv = json.loads(address)
        if (
            not isinstance(argv, list)
            or not argv
            or any(not isinstance(arg, str) for arg in argv)
        ):
            raise ValueError("exec address must be a nonempty JSON argv array")
        return child_stream(argv, options.timeout)
    if kind == "udp":
        from .udp import connect_udp

        return connect_udp(address, options)
    raise ValueError(f"unknown connector kind: {kind}")
