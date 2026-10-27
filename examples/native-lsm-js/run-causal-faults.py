#!/usr/bin/env python3
"""Finite real process exits around causal local publication; reap before recovery."""
import hashlib
import os
from pathlib import Path
import resource
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
CSHARP = os.environ.get("CLUSTER_CSHARP_EXAMPLE", "examples/cluster-csharp/bin/Release/net10.0/ClusterExample.dll")
RUNTIMES = {
    "node": ["node", "--max-old-space-size=128", "examples/native-lsm-js/causal-interop.mjs"],
    "bun": ["bun", "examples/native-lsm-js/causal-interop.mjs"],
    "deno": ["deno", "run", "--allow-all", "--v8-flags=--max-old-space-size=128", "examples/native-lsm-js/causal-interop.mjs"],
    "csharp": ["dotnet", CSHARP, "--native-lsm-causal"],
    "rust": [os.environ.get("RUST_NATIVE_LSM_EXAMPLE", "rust/target/debug/examples/native_lsm"), "causal"],
}
if selected := os.environ.get("NATIVE_LSM_RUNTIMES"):
    RUNTIMES = {name: RUNTIMES[name] for name in selected.split(",")}


def snapshot(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        assert not path.is_symlink()
        if path.is_file():
            result[str(path.relative_to(directory))] = hashlib.sha256(path.read_bytes()).hexdigest()
        else:
            assert path.is_dir()
    return result


def run(runtime, action, directory, mirrors, expected=0):
    def limits():
        resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
        if runtime == "rust":
            resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024,) * 2)
    environment = dict(os.environ, DOTNET_GCHeapHardLimit="0x28000000")
    result = subprocess.run([*RUNTIMES[runtime], action, str(directory), mirrors],
                            env=environment, preexec_fn=limits, check=False)
    assert result.returncode == expected, (runtime, action, result.returncode, expected)
    # subprocess.run has reaped the child. Recovery below never steals live ownership.


with tempfile.TemporaryDirectory(prefix="clink-causal-process-") as temporary:
    root = Path(temporary)
    for mirrors in ("plain", "mirrored"):
        for writer in RUNTIMES:
            source = root / f"{writer}-{mirrors}"
            source.mkdir()
            run(writer, "write", source, mirrors)
            for phase in ("before", "after"):
                cut = root / f"{writer}-{mirrors}-{phase}"
                shutil.copytree(source, cut)
                run(writer, f"cut-{phase}", cut, mirrors, expected=73)
                assert (cut / ".lsm-writer").is_file()
                crashed = snapshot(cut)
                for reader in RUNTIMES:
                    run(reader, "blocked", cut, mirrors)
                    assert snapshot(cut) == crashed, (writer, reader, "blocked open changed evidence")
                # This is explicit operator recovery in the finite experiment,
                # after the process has exited and been reaped. Preserve markers.
                evidence = root / f"evidence-{writer}-{mirrors}-{phase}"
                evidence.mkdir()
                for marker in cut.iterdir():
                    if marker.name == ".lsm-writer" or marker.name.endswith(".writer"):
                        marker.rename(evidence / marker.name)
                assert (evidence / ".lsm-writer").is_file()
                recovered = snapshot(cut)
                for reader in RUNTIMES:
                    target = root / f"{writer}-{reader}-{mirrors}-{phase}"
                    shutil.copytree(cut, target)
                    run(reader, "read-advanced" if phase == "after" else "read", target, mirrors)
                    assert snapshot(target) == recovered, "read changed recovered bytes"
                    run(reader, f"retry-{phase}", target, mirrors)
                    if phase == "after":
                        assert snapshot(target) == recovered, "durable retry republished"
                    run(reader, "following", target, mirrors)
                    advanced = snapshot(target)
                    run(writer, "read-following", target, mirrors)
                    assert snapshot(target) == advanced, "return read changed following bytes"
                    assert not any(name == ".lsm-writer" or name.endswith(".writer") for name in advanced)
                    print(f"PASS causal {mirrors} {writer} exit-{phase} -> {reader} retry/following -> {writer}", flush=True)
print(f"PASS {4 * len(RUNTIMES)} reaped process exits; {4 * len(RUNTIMES) ** 2} cross-runtime retry/reopen/following checks", flush=True)
print("Process exits establish application-crash recovery after explicit operator marker recovery, not hardware power-loss certification.")
