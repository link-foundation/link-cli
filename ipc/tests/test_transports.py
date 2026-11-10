"""Cross-process byte, EOF, native-resource and composition regressions."""

import faulthandler
import json
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

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
        faulthandler.dump_traceback_later(60, exit=True)
        self.addCleanup(faulthandler.cancel_dump_traceback_later)
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

    def test_close_unblocks_anonymous_pipe_reader(self):
        # The child holds stdout open without producing data.
        endpoint = "exec:" + json.dumps(
            [sys.executable, "-c", "import time; time.sleep(20)"]
        )
        stream = connect(endpoint, OPTIONS)
        received, failures = [], []

        def receive():
            try:
                received.append(stream.recv())
            except Exception as error:
                failures.append(error)

        reader = threading.Thread(target=receive)
        reader.start()
        stream.close()
        reader.join(3)
        self.assertFalse(reader.is_alive())
        self.assertEqual(failures, [])
        self.assertEqual(received, [b""])

    @unittest.skipIf(os.name == "nt", "SIGUSR1 is POSIX-only")
    def test_signal_handler_does_not_acquire_python_locks(self):
        from link_ipc.local import enable_signals, wait_tick

        enable_signals()
        for _ in range(100):
            os.kill(os.getpid(), signal.SIGUSR1)
            wait_tick(0.001)

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

    def pair(self, kind, timeout=3):
        listener = listen(
            f"{kind}:{self.root / ('pair-' + kind)}", Options(timeout=timeout)
        )
        self.addCleanup(listener.close)
        accepted, failures = [], []

        def accept():
            try:
                accepted.append(listener.accept())
            except OSError as error:
                failures.append(error)

        worker = threading.Thread(target=accept)
        worker.start()
        client = connect(listener.address, Options(timeout=timeout))
        worker.join(5)
        self.assertFalse(worker.is_alive(), "native accept did not finish")
        self.assertEqual(failures, [])
        self.addCleanup(client.close)
        self.addCleanup(accepted[0].close)
        return client, accepted[0], listener

    def test_mailbox_payload_uses_requested_native_storage(self):
        import mmap
        from multiprocessing.shared_memory import SharedMemory

        for kind in ("shm", "mmap", "file"):
            with self.subTest(kind=kind):
                client, server, _ = self.pair(kind)
                slot = client.channel.outgoing
                if kind == "shm":
                    self.assertIsInstance(slot.shared, SharedMemory)
                    self.assertFalse(slot.path.exists())
                elif kind == "mmap":
                    self.assertIsInstance(slot.mapping, mmap.mmap)
                    self.assertTrue(slot.path.exists())
                else:
                    self.assertIsNone(slot.mapping)
                    self.assertIsNone(slot.shared)
                    self.assertTrue(slot.path.exists())
                client.sendall(b"native payload")
                self.assertEqual(server.recv(), b"native payload")

    def test_full_mailbox_times_out_without_overwriting_bytes(self):
        client, server, _ = self.pair("shm", timeout=0.2)
        client.sendall(b"first")
        with self.assertRaises(TimeoutError):
            client.sendall(b"second")
        self.assertEqual(server.recv(), b"first")

    def test_close_unblocks_waiting_native_reader(self):
        client, _, _ = self.pair("mmap")
        failures = []

        def read():
            try:
                client.recv()
            except ConnectionResetError:
                failures.append("closed")

        reader = threading.Thread(target=read)
        reader.start()
        client.close()
        reader.join(3)
        self.assertFalse(reader.is_alive())
        self.assertEqual(failures, ["closed"])

    @unittest.skipUnless(os.name == "nt", "Windows defers unlink of open mappings")
    def test_listener_cleanup_waits_for_peer_handles(self):
        client, server, listener = self.pair("mmap")
        server.close()
        failures = []

        def close():
            try:
                listener.close()
            except OSError as error:
                failures.append(error)

        closer = threading.Thread(target=close)
        closer.start()
        # Hold the mapping open after the listener starts removing its paths.
        time.sleep(0.1)
        self.assertTrue(closer.is_alive(), "listener did not wait for mapped handles")
        client.close()
        closer.join(3)
        self.assertFalse(closer.is_alive())
        self.assertEqual(failures, [])
        self.assertFalse(listener.root.exists())

    def test_extension_contract(self):
        from link_ipc import register

        register(
            "application_adapter",
            connector=lambda address, options: connect("tcp:" + address, options),
            listener=lambda address, options: listen("tcp:" + address, options),
        )
        listener = listen("application_adapter:127.0.0.1:0")
        self.addCleanup(listener.close)
        with self.assertRaises(ValueError):
            register("tcp", connector=connect, listener=listen)
        self.assertTrue(listener.address.startswith("tcp:"))

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

    def test_udp_lost_welcome_does_not_require_another_accept(self):
        from link_ipc.udp import UdpChannel

        listener = listen("udp:127.0.0.1:0", OPTIONS)
        accepted, count = [], 0
        original = UdpChannel._transmit

        def drop_welcome(channel, kind, sequence=0, data=b""):
            nonlocal count
            if kind == b"W":
                count += 1
                if count <= 2:
                    return
            original(channel, kind, sequence, data)

        with patch.object(UdpChannel, "_transmit", drop_welcome):
            worker = threading.Thread(target=lambda: accepted.append(listener.accept()))
            worker.start()
            try:
                with connect(listener.address, OPTIONS) as client:
                    worker.join(3)
                    client.sendall(b"connected after loss")
                    self.assertEqual(accepted[0].recv(), b"connected after loss")
                    self.assertGreater(count, 2)
            finally:
                listener.close()
                worker.join(3)


if __name__ == "__main__":
    unittest.main()
