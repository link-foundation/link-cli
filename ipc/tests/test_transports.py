"""Cross-process byte, EOF, native-resource and composition regressions."""

import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

from link_ipc import Options, connect, listen, relay
from link_ipc.endpoints import split_endpoint

BASE = Path(__file__).resolve().parents[1]
OPTIONS = Options(timeout=3)
# Larger than every mailbox/queue, includes every byte and text framing,
# UTF-8 split points, binary zeroes, CRLF and dot-stuffed multiline names.
PAYLOAD = (
    bytes(range(256)) * 43 + b"\r\n.\r\n..\n('quoted \\ name')\n" + "你好".encode()
)


def read_all(stream):
    result = bytearray()
    while data := stream.recv(601):
        result.extend(data)
    return bytes(result)


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)

    def peer(self, endpoint):
        stderr = open(self.root / "peer.log", "w+")
        self.addCleanup(stderr.close)
        child = subprocess.Popen(
            [sys.executable, "-m", "tests.peer", endpoint],
            cwd=BASE,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
        )

        def reap():
            if child.poll() is None:
                child.kill()
            child.wait()
            child.stdout.close()

        self.addCleanup(reap)
        address = child.stdout.readline().strip()
        if not address:
            child.wait()
            stderr.seek(0)
            self.fail(stderr.read())
        return child, address

    def check_kind(self, kind):
        endpoint = (
            f"{kind}:{self.root / kind}"
            if kind not in ("tcp", "udp")
            else f"{kind}:127.0.0.1:0"
        )
        child, endpoint = self.peer(endpoint)
        with connect(endpoint, OPTIONS) as stream:
            # Drain concurrently so bounded queues never depend on payload size.
            replies, errors = [], []

            def receive():
                try:
                    replies.append(read_all(stream))
                except OSError as error:
                    errors.append(error)

            reader = threading.Thread(target=receive)
            reader.start()
            stream.sendall(PAYLOAD)
            stream.shutdown_write()
            reader.join(8)
            if reader.is_alive():
                stream.close()
                reader.join()
                self.fail(f"{kind}: reader did not finish")
            self.assertEqual(errors, [])
            self.assertEqual(replies, [PAYLOAD])
        self.assertEqual(child.wait(timeout=8), 0)
        if kind not in ("tcp", "udp"):
            self.assertFalse((self.root / kind).exists(), f"{kind}: leaked endpoint")

    def test_native_cross_process(self):
        kinds = ["tcp", "udp", "shm", "mmap", "file"]
        if os.name == "nt":
            kinds += ["pipe", "copydata", "mailslot", "alpc"]
        else:
            kinds += ["unix", "fifo", "signal"]
            if sys.platform.startswith("linux"):
                kinds += ["mq"]
        for kind in kinds:
            with self.subTest(kind=kind):
                self.check_kind(kind)

    def test_anonymous_pipes(self):
        # exec and stdio are the two sides of real inherited anonymous pipes.
        command = [sys.executable, "-m", "link_ipc", "--listen", "stdio:", "--connect"]
        child, address = self.peer("tcp:127.0.0.1:0")
        with connect("exec:" + json.dumps([*command, address]), OPTIONS) as stream:
            stream.sendall(PAYLOAD[:1000])
            stream.shutdown_write()
            self.assertEqual(read_all(stream), PAYLOAD[:1000])
        self.assertEqual(child.wait(timeout=8), 0)

    def test_stacking_two_native_layers(self):
        child, address = self.peer("shm:" + str(self.root / "stack"))
        listener = listen("tcp:127.0.0.1:0", OPTIONS)
        errors = []

        def decorate():
            try:
                with (
                    listener.accept() as incoming,
                    connect(address, OPTIONS) as outgoing,
                ):
                    relay(incoming, outgoing)
            except OSError as error:
                errors.append(error)

        worker = threading.Thread(target=decorate)
        worker.start()
        try:
            with connect(listener.address, OPTIONS) as stream:
                stream.sendall(PAYLOAD[:1000])
                stream.shutdown_write()
                self.assertEqual(read_all(stream), PAYLOAD[:1000])
        finally:
            listener.close()
            worker.join(8)
        self.assertFalse(worker.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(child.wait(timeout=8), 0)

    def test_existing_endpoint_is_not_removed(self):
        root = self.root / "owned"
        root.mkdir()
        (root / "precious").write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            listen(f"shm:{root}", OPTIONS)
        self.assertEqual((root / "precious").read_bytes(), b"keep")

    def test_peer_crash_unblocks_reader(self):
        child, address = self.peer(f"shm:{self.root / 'crash'}")
        with connect(address, OPTIONS) as stream:
            child.kill()
            child.wait()
            with self.assertRaises(ConnectionResetError):
                stream.recv()

    def test_configuration_errors(self):
        for value in (0, -1, float("nan"), float("inf")):
            with self.subTest(value=value), self.assertRaises(ValueError):
                Options(timeout=value)
        with self.assertRaises(ValueError):
            connect("unknown:x")
        with self.assertRaises(ValueError):
            connect("udp:0.0.0.0:1")
        if os.name != "nt":
            with self.assertRaisesRegex(OSError, "Windows"):
                split_endpoint("alpc:x")

    def test_udp_loss_and_duplicate_are_not_duplicate_bytes(self):
        listener = listen("udp:127.0.0.1:0", OPTIONS)
        accepted = []
        worker = threading.Thread(target=lambda: accepted.append(listener.accept()))
        worker.start()
        client = connect(listener.address, OPTIONS)
        worker.join(5)
        server = accepted[0]
        try:
            original = server.channel._transmit
            count = 0

            def drop_first_ack(kind, sequence=0, data=b""):
                nonlocal count
                if kind == b"A":
                    count += 1
                    if count == 1:
                        return
                original(kind, sequence, data)

            server.channel._transmit = drop_first_ack
            client.sendall(b"one mutation")
            self.assertEqual(server.recv(), b"one mutation")
            client.shutdown_write()
            self.assertEqual(server.recv(), b"")
            self.assertGreaterEqual(count, 3)
        finally:
            client.close()
            server.close()
            listener.close()


if __name__ == "__main__":
    unittest.main()
