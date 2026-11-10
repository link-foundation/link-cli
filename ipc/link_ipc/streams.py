"""Bounded duplex byte streams and protocol-independent relay composition."""

import contextlib
import os
import select
import socket
import subprocess
import threading
import time

CHUNK = 4096


class Stream:
    """Extension contract. One reader and one writer may run concurrently.

    recv returns at most size bytes; b'' means EOF. sendall either sends all
    bytes or raises (the remote outcome is then unknown). close must unblock
    both operations. shutdown_write sends EOF after all preceding bytes.
    """

    def recv(self, size=CHUNK):
        raise NotImplementedError

    def sendall(self, data):
        raise NotImplementedError

    def shutdown_write(self):
        raise NotImplementedError

    def close(self):
        raise NotImplementedError

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


class SocketStream(Stream):
    def __init__(self, sock, timeout=30):
        self.socket = sock
        self.timeout = timeout
        self.closed = threading.Event()
        sock.settimeout(0.2)

    def recv(self, size=CHUNK):
        while not self.closed.is_set():
            try:
                return self.socket.recv(size)
            except TimeoutError:
                continue
        return b""

    def sendall(self, data):
        end = time.monotonic() + self.timeout
        view = memoryview(data)
        while view and not self.closed.is_set():
            if time.monotonic() >= end:
                raise TimeoutError("IPC socket write stalled; outcome unknown")
            try:
                count = self.socket.send(view)
                if not count:
                    raise ConnectionError("IPC socket closed during write")
                view = view[count:]
            except TimeoutError:
                continue
        if view:
            raise ConnectionError("IPC socket closed during write")

    def shutdown_write(self):
        with contextlib.suppress(OSError):
            self.socket.shutdown(socket.SHUT_WR)

    def close(self):
        self.closed.set()
        with contextlib.suppress(OSError):
            self.socket.shutdown(socket.SHUT_RDWR)
        self.socket.close()


class MessageStream(Stream):
    """Adapt bounded native messages to a stream; an empty message is EOF."""

    def __init__(self, channel):
        self.channel = channel
        self.pending = b""
        self.eof = False

    def recv(self, size=CHUNK):
        if size <= 0:
            raise ValueError("recv size must be positive")
        if not self.pending and not self.eof:
            self.pending = self.channel.receive()
            self.eof = not self.pending
        result, self.pending = self.pending[:size], self.pending[size:]
        return result

    def sendall(self, data):
        for offset in range(0, len(data), CHUNK):
            self.channel.send(data[offset : offset + CHUNK])

    def shutdown_write(self):
        self.channel.send(b"")

    def close(self):
        self.channel.close()


class ConnectionChannel:
    """Python's native Windows named pipe, using bytes only (never pickle)."""

    def __init__(self, connection):
        self.connection = connection
        self.closed = threading.Event()

    def send(self, data):
        self.connection.send_bytes(data)

    def receive(self):
        while not self.closed.is_set():
            if self.connection.poll(0.1):
                try:
                    return self.connection.recv_bytes(CHUNK)
                except EOFError:
                    return b""
        return b""

    def close(self):
        self.closed.set()
        self.connection.close()


class FileStream(Stream):
    """Anonymous pipes / stdio. The owner explicitly provides cancellation."""

    def __init__(self, reader, writer, cancel=None, timeout=30):
        self.reader, self.writer, self.cancel = reader, writer, cancel
        self.timeout = timeout
        self.closed = threading.Event()
        self.write_thread = None
        if os.name == "posix":
            os.set_blocking(reader.fileno(), False)
            os.set_blocking(writer.fileno(), False)

    def cancel_write(self):
        if os.name == "nt" and self.write_thread is not None:
            import ctypes
            from ctypes import wintypes

            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.OpenThread.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            kernel.OpenThread.restype = wintypes.HANDLE
            kernel.CancelSynchronousIo.argtypes = [wintypes.HANDLE]
            kernel.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = kernel.OpenThread(1, False, self.write_thread)
            if handle:
                kernel.CancelSynchronousIo(handle)
                kernel.CloseHandle(handle)

    def recv(self, size=CHUNK):
        while not self.closed.is_set():
            if os.name == "nt":
                import _winapi
                import msvcrt

                try:
                    _, available, _ = _winapi.PeekNamedPipe(
                        msvcrt.get_osfhandle(self.reader.fileno()), 0
                    )
                except BrokenPipeError:
                    return b""
                if available:
                    return os.read(self.reader.fileno(), min(size, available))
                self.closed.wait(0.02)
            elif select.select([self.reader], [], [], 0.1)[0]:
                try:
                    return os.read(self.reader.fileno(), size)
                except BlockingIOError:
                    continue
        return b""

    def sendall(self, data):
        end = time.monotonic() + self.timeout
        timer = None
        if os.name == "nt":
            self.write_thread = threading.get_native_id()
            timer = threading.Timer(self.timeout, self.cancel_write)
            timer.start()
        try:
            view = memoryview(data)
            while view:
                if self.closed.is_set():
                    raise BrokenPipeError("IPC pipe closed")
                if time.monotonic() >= end:
                    raise TimeoutError("IPC pipe write stalled; outcome unknown")
                if (
                    os.name == "posix"
                    and not select.select([], [self.writer], [], 0.1)[1]
                ):
                    continue
                try:
                    count = os.write(self.writer.fileno(), view[:CHUNK])
                except BlockingIOError:
                    continue
                if not count:
                    raise BrokenPipeError("IPC pipe closed")
                view = view[count:]
        finally:
            if timer:
                timer.cancel()
                timer.join()
            self.write_thread = None

    def shutdown_write(self):
        self.writer.close()

    def close(self):
        self.closed.set()
        self.cancel_write()
        if self.cancel:
            self.cancel()
        for file in (self.reader, self.writer):
            with contextlib.suppress(OSError, ValueError):
                file.close()


def child_stream(argv, timeout=30):
    """Start argv without a shell. Own, terminate and reap only this child."""
    child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE)

    def cancel():
        if child.poll() is None:
            child.terminate()
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()

    return FileStream(child.stdout, child.stdin, cancel, timeout)


def relay(left, right, stop=None, trace=None):
    """Copy both directions with backpressure, retaining half-close semantics.

    Memory is bounded by two CHUNK buffers plus the endpoints' own buffers.
    On an error both ends close. There is no reconnect or query replay.
    """
    errors = []
    stop = stop or threading.Event()
    abort = threading.Event()

    def copy(source, target, direction):
        try:
            while not stop.is_set() and not abort.is_set():
                data = source.recv(CHUNK)
                if not data:
                    target.shutdown_write()
                    return
                if trace:
                    trace(f"{direction}: {len(data)} bytes")
                target.sendall(data)
        except (OSError, ValueError, EOFError) as error:
            errors.append(error)
            abort.set()
            left.close()
            right.close()

    threads = [
        threading.Thread(target=copy, args=(left, right, "forward")),
        threading.Thread(target=copy, args=(right, left, "reverse")),
    ]
    try:
        for thread in threads:
            thread.start()
        while any(thread.is_alive() for thread in threads):
            if stop.wait(0.05) or abort.is_set():
                left.close()
                right.close()
            for thread in threads:
                thread.join(0.05)
    finally:
        left.close()
        right.close()
    if errors:
        raise errors[0]
