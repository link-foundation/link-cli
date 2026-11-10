"""Real clink text/binary client/server conversations through IPC layers.

CLINK_COMMAND is a JSON argv array (Rust executable or dotnet + clink.dll).
No source build is triggered by this test. CI supplies the built command.
"""

import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

from link_ipc import Options, connect

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.environ.get("CLINK_COMMAND"), "CI supplies a built clink")
class ProtocolTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.command = json.loads(os.environ["CLINK_COMMAND"])
        self.addCleanup(self.temp.cleanup)
        self.children = []
        self.gateways = 0
        self.addCleanup(self.close_children)
        log = open(self.root / "server.log", "w+")
        self.addCleanup(log.close)
        self.server = subprocess.Popen(
            [
                *self.command,
                "--db",
                str(self.root / "db.links"),
                "--serve",
                "127.0.0.1:0",
            ],
            stdout=subprocess.PIPE,
            stderr=log,
            text=True,
        )
        self.children.append(self.server)
        lines = queue.Queue(maxsize=1)
        reader = threading.Thread(
            target=lambda: lines.put(self.server.stdout.readline())
        )
        reader.start()
        try:
            banner = lines.get(timeout=20).strip()
        except queue.Empty:
            self.server.kill()
            self.server.wait()
            self.fail("clink server did not announce readiness")
        finally:
            reader.join(2)
        self.assertTrue(banner.startswith("clink server listening on "), banner)
        self.server_address = banner.removeprefix("clink server listening on ")

    def close_children(self):
        for child in reversed(self.children):
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
            if child.stdout:
                child.stdout.close()

    def gateway(self, incoming, outgoing):
        self.gateways += 1
        ready = self.root / f"ready-{self.gateways}.json"
        log = open(self.root / f"gateway-{self.gateways}.log", "w+")
        self.addCleanup(log.close)
        child = subprocess.Popen(
            [
                sys.executable,
                "-m",
                "link_ipc",
                "--listen",
                incoming,
                "--connect",
                outgoing,
                "--ready-file",
                str(ready),
                "--timeout",
                "5",
                "--trace",
            ],
            stderr=log,
            stdout=subprocess.DEVNULL,
            cwd=ROOT,
        )
        self.children.append(child)
        end = time.monotonic() + 10
        while time.monotonic() < end:
            if ready.exists():
                try:
                    return json.loads(ready.read_text())["endpoint"]
                except json.JSONDecodeError:
                    pass  # Writer has opened the exclusively owned file.
            if child.poll() is not None:
                log.seek(0)
                self.fail(log.read())
            time.sleep(0.02)
        self.fail("IPC gateway did not announce readiness")

    def invoke(self, address, query, protocol):
        args = [
            *self.command,
            "--connect",
            address.removeprefix("tcp:"),
            "--protocol",
            protocol,
        ]
        if protocol == "binary":
            args += ["--external-references", "--arity", "1..", "--packed-widths"]
        result = subprocess.run(
            [*args, query], capture_output=True, text=True, timeout=15
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def test_every_native_with_text_and_binary(self):
        kinds = ["tcp", "udp", "shm", "mmap", "file"]
        kinds += (
            ["pipe", "copydata", "mailslot", "alpc"]
            if os.name == "nt"
            else ["unix", "fifo", "mq", "signal"]
        )
        for kind in kinds:
            with self.subTest(kind=kind):
                target = f"{kind}:{self.root / kind}"
                if kind in ("tcp", "udp"):
                    target = f"{kind}:127.0.0.1:0"
                served = self.gateway(target, "tcp:" + self.server_address)
                local = self.gateway("tcp:127.0.0.1:0", served)
                for protocol in ("text", "binary"):
                    self.assertEqual(
                        self.invoke(local, "(count: ())", protocol), "(count: 0)"
                    )
                    self.assertIn(
                        "((1: 0 0))", self.invoke(local, "(create: (0 0))", protocol)
                    )
                    self.assertEqual(
                        self.invoke(local, "(count: ())", protocol), "(count: 1)"
                    )
                    self.assertIn("()", self.invoke(local, "(delete: 1)", protocol))
                self.close_children_except_server()

    def close_children_except_server(self):
        saved, self.children = self.children[0], self.children[1:]
        self.close_children()
        self.children = [saved]

    def test_two_different_native_layers_and_subscription_pushes(self):
        final = self.gateway(
            "shm:" + str(self.root / "memory"), "tcp:" + self.server_address
        )
        middle = self.gateway("mmap:" + str(self.root / "mapping"), final)
        local = self.gateway("tcp:127.0.0.1:0", middle)
        with connect(local, Options(timeout=5)) as watcher:
            watcher.sendall(b"(subscribe: (* * *))\n.\n")
            self.assertIn(b"subscription", read_text(watcher))
            self.invoke(local, "(create: (0 0))", "binary")
            self.assertIn(b"notify", read_text(watcher))
            watcher.sendall(b"(unsubscribe: 1)\n.\n")
            self.assertEqual(read_text(watcher), b"")

    def test_anonymous_pipe_to_server(self):
        argv = [
            sys.executable,
            "-m",
            "link_ipc",
            "--listen",
            "stdio:",
            "--connect",
            "tcp:" + self.server_address,
        ]
        with connect("exec:" + json.dumps(argv), Options(timeout=5)) as stream:
            stream.sendall(b"(count: ())\n.\n")
            self.assertEqual(read_text(stream), b"(count: 0)\n")
            stream.shutdown_write()
            self.assertEqual(stream.recv(), b"")


def read_text(stream):
    # Read precisely one text frame, without consuming the following push.
    result = bytearray()
    while True:
        line = bytearray()
        while not line.endswith(b"\n"):
            byte = stream.recv(1)
            if not byte:
                raise EOFError("server closed inside a text frame")
            line.extend(byte)
        if line in (b".\n", b".\r\n"):
            return bytes(result)
        result.extend(line)


if __name__ == "__main__":
    unittest.main()
