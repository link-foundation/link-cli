"""POSIX FIFO and message-queue payload channels (no third-party modules)."""

import contextlib
import ctypes
import ctypes.util
import errno
import os
import struct
import time

from .local import wait_tick
from .streams import CHUNK


class Fifo:
    def __init__(self, session, names, create):
        self.session, self.anchors = session, []
        self.read_fd = self.write_fd = None
        if not hasattr(os, "mkfifo"):
            raise OSError("FIFO endpoints require POSIX; use pipe: on Windows")
        for name in names:
            path = session.root / name
            if create:
                os.mkfifo(path, 0o600)
            self.anchors.append(os.open(path, os.O_RDWR | os.O_NONBLOCK))
        self.write_fd = os.open(session.root / names[0], os.O_WRONLY | os.O_NONBLOCK)
        self.read_fd = os.open(session.root / names[1], os.O_RDONLY | os.O_NONBLOCK)

    def activate(self):
        for fd in self.anchors:
            os.close(fd)
        self.anchors.clear()

    def send(self, data):
        view = memoryview(struct.pack("<I", len(data)) + data)
        end = time.monotonic() + self.session.options.timeout
        while view:
            self.session.check()
            try:
                view = view[os.write(self.write_fd, view) :]
            except BlockingIOError:
                if time.monotonic() >= end:
                    raise TimeoutError(
                        "IPC FIFO write stalled; outcome unknown"
                    ) from None
                wait_tick(self.session.options.poll)

    def _read(self, size):
        result = bytearray()
        while len(result) < size:
            try:
                data = os.read(self.read_fd, size - len(result))
                if data:
                    result.extend(data)
                    continue
            except BlockingIOError:
                pass
            self.session.check()
            wait_tick(self.session.options.poll)
        return bytes(result)

    def receive(self):
        size = struct.unpack("<I", self._read(4))[0]
        if size > CHUNK:
            raise OSError("invalid IPC FIFO frame length")
        return self._read(size)

    def close(self):
        for fd in [self.read_fd, self.write_fd, *self.anchors]:
            if fd is not None:
                with contextlib.suppress(OSError):
                    os.close(fd)


class MQ:
    def __init__(self, session, names, create):
        self.session, self.names, self.owner = session, names, create
        # Linux/glibc exports mq_* from libc since 2.34; older systems use rt.
        self.lib = ctypes.CDLL(ctypes.util.find_library("rt") or None, use_errno=True)
        if not hasattr(self.lib, "mq_open"):
            raise OSError("POSIX message queues are unavailable on this OS")
        self.lib.mq_open.restype = ctypes.c_int
        self.lib.mq_send.argtypes = [
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_uint,
        ]
        self.lib.mq_receive.argtypes = [
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
        ]
        self.lib.mq_receive.restype = ctypes.c_ssize_t
        self.lib.mq_close.argtypes = [ctypes.c_int]
        self.lib.mq_unlink.argtypes = [ctypes.c_char_p]
        self.handles = []
        try:
            for name in names:
                # mq_attr is four longs followed by four reserved longs on
                # Linux. Size/count stay below unprivileged system defaults.
                attrs = (ctypes.c_long * 8)(0, 4, CHUNK + 1, 0)
                flags = os.O_RDWR | os.O_NONBLOCK
                if create:
                    flags |= os.O_CREAT | os.O_EXCL
                fd = self.lib.mq_open(name.encode(), flags, 0o600, ctypes.byref(attrs))
                self._check(fd)
                self.handles.append(fd)
        except BaseException:
            self.close()
            raise

    @staticmethod
    def _check(value):
        if value == -1:
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code))
        return value

    def send(self, data):
        # A tag makes EOF a normal nonempty native message.
        packet = b"\1" + data if data else b"\0"
        end = time.monotonic() + self.session.options.timeout
        while True:
            self.session.check()
            value = self.lib.mq_send(self.handles[0], packet, len(packet), 0)
            if value == 0:
                return
            if ctypes.get_errno() not in (errno.EAGAIN, errno.EINTR):
                self._check(value)
            if time.monotonic() >= end:
                raise TimeoutError("IPC message queue write stalled; outcome unknown")
            wait_tick(self.session.options.poll)

    def receive(self):
        buffer = ctypes.create_string_buffer(CHUNK + 1)
        while True:
            count = self.lib.mq_receive(self.handles[1], buffer, len(buffer), None)
            if count >= 1:
                packet = buffer.raw[:count]
                if packet == b"\0":
                    return b""
                if packet[0] != 1:
                    raise OSError("invalid IPC message queue frame")
                return packet[1:]
            if ctypes.get_errno() not in (errno.EAGAIN, errno.EINTR):
                self._check(count)
            self.session.check()
            wait_tick(self.session.options.poll)

    def close(self):
        for handle in self.handles:
            self.lib.mq_close(handle)
        self.handles.clear()
        if self.owner:
            for name in self.names:
                self.lib.mq_unlink(name.encode())


class ManagedChannel:
    def __init__(self, channel, session):
        self.channel, self.session = channel, session
        session.resources.append(channel)

    def send(self, data):
        return self.channel.send(data)

    def receive(self):
        return self.channel.receive()

    def close(self):
        self.session.close()
