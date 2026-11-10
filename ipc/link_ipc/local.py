"""Native local payload channels with a separate, byte-only rendezvous pipe.

The rendezvous carries only bounded JSON configuration and peer liveness.
Shared memory / mmap / disk payloads use two bounded mailboxes protected by
OS file locks: no assumptions about atomic writes or CPU memory ordering.
"""

import contextlib
import json
import mmap
import os
import select
import shutil
import signal
import struct
import threading
import time
from multiprocessing.shared_memory import SharedMemory
from pathlib import Path

from .streams import CHUNK

EOF = 0xFFFFFFFF
_WAKE_READ = _WAKE_WRITE = None


def enable_signals():
    """Explicit process-wide opt-in; call from the main thread before use."""
    global _WAKE_READ, _WAKE_WRITE
    if not hasattr(signal, "SIGUSR1"):
        raise OSError("signal IPC requires POSIX SIGUSR1")
    previous = signal.getsignal(signal.SIGUSR1)
    if previous == _on_signal:
        return
    if previous not in (signal.SIG_DFL, signal.SIG_IGN, _on_signal):
        raise OSError("SIGUSR1 already has a handler; choose another endpoint")
    _WAKE_READ, _WAKE_WRITE = os.pipe()
    os.set_blocking(_WAKE_READ, False)
    os.set_blocking(_WAKE_WRITE, False)
    signal.set_wakeup_fd(_WAKE_WRITE, warn_on_full_buffer=False)
    signal.signal(signal.SIGUSR1, _on_signal)


def _on_signal(*_):
    # Python synchronization primitives are not reentrant from a signal
    # handler. The interpreter writes the wakeup byte without taking a lock.
    pass


def wait_tick(poll):
    # Coalesced / early / unrelated notifications cannot lose payload: the
    # mailbox is authoritative. Polling also detects a crashed peer.
    if _WAKE_READ is None:
        time.sleep(poll)
    elif select.select([_WAKE_READ], [], [], poll)[0]:
        with contextlib.suppress(BlockingIOError):
            os.read(_WAKE_READ, 4096)


def send_json(connection, value):
    data = json.dumps(value).encode()
    if len(data) > 8192:
        raise ValueError("IPC rendezvous configuration too large")
    connection.send_bytes(data)


def receive_json(connection, timeout):
    if not connection.poll(timeout):
        raise TimeoutError("IPC rendezvous timed out")
    return json.loads(connection.recv_bytes(8192))


class Session:
    def __init__(self, control, options, root, peer_pid, owner=False):
        self.control, self.options = control, options
        self.root, self.peer_pid, self.owner = Path(root), peer_pid, owner
        self.closed = threading.Event()
        self.peer_closed = threading.Event()
        self.monitor = threading.Thread(target=self._watch)
        self.resources = []
        self.close_lock = threading.Lock()

    def start(self):
        self.monitor.start()

    def _watch(self):
        try:
            while not self.closed.is_set():
                if self.control.poll(0.1):
                    # No control traffic is allowed after the handshake.
                    self.control.recv_bytes(0)
                    break
        except (OSError, EOFError):
            pass
        finally:
            self.peer_closed.set()

    def check(self):
        if self.closed.is_set() or self.peer_closed.is_set():
            raise ConnectionResetError("IPC peer disconnected; outcome unknown")

    def notify(self):
        if self.options.kind == "signal":
            with contextlib.suppress(ProcessLookupError):
                os.kill(self.peer_pid, signal.SIGUSR1)

    def close(self):
        with self.close_lock:
            if self.closed.is_set():
                return
            self.closed.set()
            if self.monitor.is_alive():
                self.monitor.join()
            for resource in self.resources:
                with contextlib.suppress(OSError):
                    resource.close()
            self.control.close()
            # Both sides attempt cleanup: Windows may defer deletion until
            # the peer's last mapping or file handle has closed.
            if self.owner or self.peer_closed.is_set():
                shutil.rmtree(self.root, ignore_errors=True)


class LockedBuffer:
    def __init__(self, root, name, kind, create):
        self.path = Path(root) / name
        self.kind, self.create = kind, create
        self.guard = threading.RLock()
        self.closed = False
        self.lock = open(str(self.path) + ".lock", "a+b")
        if os.fstat(self.lock.fileno()).st_size == 0:
            self.lock.write(b"\0")
            self.lock.flush()
        self.file = self.mapping = self.shared = None
        if kind in ("shm", "signal"):
            self.shared = SharedMemory(
                name=name, create=create, size=CHUNK + 4, track=False
            )
            self.buffer = self.shared.buf
        else:
            self.file = open(self.path, "w+b" if create else "r+b", buffering=0)
            if create:
                self.file.truncate(CHUNK + 4)
            if kind == "mmap":
                self.mapping = mmap.mmap(self.file.fileno(), CHUNK + 4)
                self.buffer = self.mapping
        if create:
            self.store(b"\0\0\0\0", 0)

    def __enter__(self):
        self.guard.acquire()
        try:
            if self.closed:
                raise ConnectionResetError("IPC buffer closed")
            return self._lock()
        except BaseException:
            self.guard.release()
            raise

    def _lock(self):
        if os.name == "nt":
            import msvcrt

            self.lock.seek(0)
            # LK_LOCK retries for 10 seconds. LK_NBLCK keeps the configured
            # bounded wait policy in Mailboxes rather than blocking here.
            msvcrt.locking(self.lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl

            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return self

    def __exit__(self, *_):
        try:
            if os.name == "nt":
                import msvcrt

                self.lock.seek(0)
                msvcrt.locking(self.lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.lock, fcntl.LOCK_UN)
        finally:
            self.guard.release()

    def load(self, size, offset=0):
        if self.kind == "file":
            self.file.seek(offset)
            return self.file.read(size)
        return bytes(self.buffer[offset : offset + size])

    def store(self, data, offset):
        if self.kind == "file":
            self.file.seek(offset)
            self.file.write(data)
        else:
            self.buffer[offset : offset + len(data)] = data

    def close(self):
        with self.guard:
            if self.closed:
                return
            self.closed = True
            if self.shared:
                self.buffer.release()
                self.shared.close()
                with contextlib.suppress(FileNotFoundError):
                    self.shared.unlink()
            if self.mapping:
                self.mapping.close()
            if self.file:
                self.file.close()
            self.lock.close()


class Mailboxes:
    def __init__(self, session, names, create=False):
        self.session = session
        self.outgoing = LockedBuffer(
            session.root, names[0], session.options.kind, create
        )
        session.resources.append(self.outgoing)
        self.incoming = LockedBuffer(
            session.root, names[1], session.options.kind, create
        )
        session.resources.append(self.incoming)

    def send(self, data):
        if len(data) > CHUNK:
            raise ValueError("IPC mailbox frame exceeds limit")
        end = time.monotonic() + self.session.options.timeout
        while True:
            self.session.check()
            try:
                with self.outgoing as slot:
                    length = struct.unpack("<I", slot.load(4))[0]
                    if length == EOF:
                        raise BrokenPipeError("IPC write side already closed")
                    if length == 0:
                        slot.store(data, 4)
                        slot.store(struct.pack("<I", len(data) if data else EOF), 0)
                        self.session.notify()
                        return
            except BlockingIOError:
                pass
            except PermissionError:
                if os.name != "nt":
                    raise
            if time.monotonic() >= end:
                raise TimeoutError("IPC mailbox write stalled; outcome unknown")
            wait_tick(self.session.options.poll)

    def receive(self):
        while True:
            try:
                with self.incoming as slot:
                    length = struct.unpack("<I", slot.load(4))[0]
                    if length == EOF:
                        return b""
                    if length > CHUNK:
                        raise OSError("invalid IPC mailbox length")
                    if length:
                        data = slot.load(length, 4)
                        slot.store(b"\0\0\0\0", 0)
                        self.session.notify()
                        return data
            except BlockingIOError:
                pass
            except PermissionError:
                if os.name != "nt":
                    raise
            self.session.check()
            wait_tick(self.session.options.poll)

    def close(self):
        self.session.close()
