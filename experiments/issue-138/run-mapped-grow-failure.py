#!/usr/bin/env python3
"""Run the finite Linux quota probe in isolated, reaped child processes."""
import os
import resource
import signal
import subprocess
import sys
import tempfile
from pathlib import Path

base = Path(__file__).resolve().parent
binary = Path(os.environ.get("LINK_MAPPED_GROWTH_PROBE", str(base / "mapped-grow-failure/target/debug/mapped-grow-failure-probe")))
expect_retained = sys.argv[1:] == ["--expect-retained"]
assert not sys.argv[1:] or expect_retained
if sys.platform != "linux":
    raise SystemExit("This experiment requires Linux RLIMIT_FSIZE semantics")


def limits():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_AS, (256 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_FSIZE, (4096, 4096))
    resource.setrlimit(resource.RLIMIT_CPU, (5, 6))
    signal.signal(signal.SIGXFSZ, signal.SIG_IGN)


with tempfile.TemporaryDirectory(prefix="mapped-growth-", dir=base) as directory:
    for mode in ("within-quota", "rejected-growth"):
        # This is a test-child deadline, not a compiler or tool-command timeout.
        child = subprocess.Popen(
            [str(binary), mode, str(Path(directory) / mode)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            preexec_fn=limits, env={**os.environ, "RUST_BACKTRACE": "0"},
        )
        try:
            stdout, stderr = child.communicate(timeout=30)
        except BaseException:
            child.kill()
            child.communicate()
            raise
        print(f"{mode}: exit={child.returncode}\n{stdout}{stderr}", flush=True)
        assert len(stdout) + len(stderr) < 4096
        if mode == "within-quota":
            assert child.returncode == 0 and "PASS finite growth" in stdout
        else:
            assert "REJECTED growth:" in stderr and "retained length=32" in stderr
            if expect_retained:
                assert child.returncode == 0 and "PASS retained mapping" in stdout
                assert "PASS retained flush descriptor" in stderr
            else:
                assert "REJECTED stale flush descriptor" in stderr
                assert child.returncode in (-signal.SIGSEGV, -signal.SIGBUS), child.returncode
print(("PASS rejected growth preserves the retained allocation" if expect_retained else "REPRODUCED upstream rejected growth leaves the retained allocation unreadable") + "; all children reaped")
