"""Loopback UDP streams: per-session sequence numbers, ACKs and backpressure.

Only datagrams are retransmitted. Duplicate bytes never reach the application;
a failed session is never reconnected and a LiNo request is never replayed.
This is a local IPC adapter, not an Internet transport or authentication layer.
"""

import ipaddress
import queue
import socket
import struct
import threading
import time
import uuid

from .streams import CHUNK, MessageStream

MAGIC = b"LIP1"
HEADER = struct.Struct("!4s16scQ")


def loopback_address(address):
    from .endpoints import host_port

    host, port = host_port(address)
    ip = ipaddress.ip_address(host)
    if not ip.is_loopback:
        raise ValueError("UDP IPC requires a numeric loopback address")
    return str(ip), port


class UdpChannel:
    def __init__(self, sock, peer, token, options, on_close=None, welcome=False):
        self.socket, self.peer, self.token, self.options = sock, peer, token, options
        self.on_close = on_close
        self.closed = threading.Event()
        self.incoming = queue.Queue(maxsize=1)
        self.condition = threading.Condition()
        self.send_lock = threading.Lock()
        self.sent = self.received = self.acked = 0
        self.last_seen = time.monotonic()
        self.error = None
        self.welcome = welcome
        sock.settimeout(0.1)
        self.thread = threading.Thread(target=self._receive)
        self.thread.start()

    def _transmit(self, kind, sequence=0, data=b""):
        self.socket.sendto(
            HEADER.pack(MAGIC, self.token, kind, sequence) + data, self.peer
        )

    def _receive(self):
        last_ping = 0
        try:
            while not self.closed.is_set():
                if self.welcome:
                    self._transmit(b"W")
                now = time.monotonic()
                if now - self.last_seen > self.options.timeout:
                    raise TimeoutError(
                        "UDP IPC peer stopped responding; outcome unknown"
                    )
                if now - last_ping > min(1, self.options.timeout / 3):
                    self._transmit(b"P")
                    last_ping = now
                try:
                    packet, source = self.socket.recvfrom(HEADER.size + CHUNK + 1)
                except TimeoutError:
                    continue
                if (
                    source != self.peer
                    or not HEADER.size <= len(packet) <= HEADER.size + CHUNK
                ):
                    continue
                magic, token, kind, sequence = HEADER.unpack_from(packet)
                if magic != MAGIC or token != self.token:
                    continue
                self.last_seen = time.monotonic()
                self.welcome = False
                if kind == b"P":
                    self._transmit(b"Q")
                elif kind == b"A":
                    with self.condition:
                        if sequence == self.sent:
                            self.acked = sequence
                            self.condition.notify_all()
                elif kind == b"D":
                    if sequence == self.received + 1:
                        try:
                            self.incoming.put_nowait(packet[HEADER.size :])
                            self.received = sequence
                        except queue.Full:
                            continue  # No ACK until bounded receiver space exists.
                    if sequence == self.received:
                        self._transmit(b"A", sequence)
        except OSError as error:
            self.error = error
        finally:
            self.closed.set()
            with self.condition:
                self.condition.notify_all()

    def send(self, data):
        with self.send_lock:
            if len(data) > CHUNK:
                raise ValueError("UDP IPC frame exceeds limit")
            self.sent += 1
            end = time.monotonic() + self.options.timeout
            with self.condition:
                while self.acked != self.sent:
                    if self.closed.is_set():
                        raise self.error or ConnectionResetError(
                            "UDP IPC session closed"
                        )
                    if time.monotonic() >= end:
                        self.closed.set()
                        raise TimeoutError("UDP IPC write stalled; outcome unknown")
                    self._transmit(b"D", self.sent, data)
                    self.condition.wait(min(0.1, max(0, end - time.monotonic())))

    def receive(self):
        while True:
            try:
                return self.incoming.get(timeout=0.1)
            except queue.Empty:
                if self.closed.is_set():
                    raise self.error or ConnectionResetError(
                        "UDP IPC session closed"
                    ) from None

    def close(self):
        self.closed.set()
        self.socket.close()
        if self.thread is not threading.current_thread():
            self.thread.join()
        if self.on_close:
            self.on_close()
            self.on_close = None


class UdpListener:
    def __init__(self, address, options):
        self.host, port = loopback_address(address)
        self.options, self.closed = options, False
        self.socket = socket.socket(
            socket.AF_INET6 if ":" in self.host else socket.AF_INET, socket.SOCK_DGRAM
        )
        self.socket.bind((self.host, port))
        self.socket.settimeout(0.2)
        self.address = f"udp:{'[' + self.host + ']' if ':' in self.host else self.host}:{self.socket.getsockname()[1]}"
        self.sessions = {}
        self.lock = threading.Lock()

    def accept(self):
        while not self.closed:
            try:
                packet, peer = self.socket.recvfrom(HEADER.size + 1)
            except TimeoutError:
                continue
            if (
                len(packet) != HEADER.size
                or not ipaddress.ip_address(peer[0]).is_loopback
            ):
                continue
            magic, token, kind, sequence = HEADER.unpack(packet)
            if magic != MAGIC or kind != b"H" or sequence:
                continue
            with self.lock:
                previous = self.sessions.get((peer, token))
                if previous:
                    previous._transmit(b"W")
                    continue
                if len(self.sessions) >= 64:
                    continue
                sock = socket.socket(self.socket.family, socket.SOCK_DGRAM)
                sock.bind((self.host, 0))

                def remove(key=(peer, token)):
                    with self.lock:
                        self.sessions.pop(key, None)

                channel = UdpChannel(
                    sock, peer, token, self.options, remove, welcome=True
                )
                self.sessions[(peer, token)] = channel
                channel._transmit(b"W")
                return MessageStream(channel)
        raise OSError("UDP IPC listener closed")

    def close(self):
        self.closed = True
        self.socket.close()
        with self.lock:
            channels = list(self.sessions.values())
        for channel in channels:
            channel.close()


def connect_udp(address, options):
    host, port = loopback_address(address)
    sock = socket.socket(
        socket.AF_INET6 if ":" in host else socket.AF_INET, socket.SOCK_DGRAM
    )
    token = uuid.uuid4().bytes
    sock.settimeout(0.1)
    end = time.monotonic() + options.timeout
    try:
        while time.monotonic() < end:
            sock.sendto(HEADER.pack(MAGIC, token, b"H", 0), (host, port))
            try:
                packet, peer = sock.recvfrom(HEADER.size + 1)
            except TimeoutError:
                continue
            if peer[0] == host and packet == HEADER.pack(MAGIC, token, b"W", 0):
                return MessageStream(UdpChannel(sock, peer, token, options))
        raise TimeoutError("UDP IPC handshake timed out")
    except BaseException:
        sock.close()
        raise
