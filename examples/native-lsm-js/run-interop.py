#!/usr/bin/env python3
"""Finite complete-store handoffs; every process exits before the next owner."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
CSHARP = os.environ.get("CLUSTER_CSHARP_EXAMPLE", "examples/cluster-csharp/bin/Release/net10.0/ClusterExample.dll")
RUNTIMES = {
    "node": ["node", "examples/native-lsm-js/interop.mjs"],
    "bun": ["bun", "examples/native-lsm-js/interop.mjs"],
    "deno": ["deno", "run", "--allow-all", "examples/native-lsm-js/interop.mjs"],
    "csharp": ["dotnet", CSHARP, "--native-lsm-files"],
    "rust": [os.environ.get("RUST_NATIVE_LSM_EXAMPLE", "rust/target/debug/examples/native_lsm"), "files"],
}
if selected := os.environ.get("NATIVE_LSM_RUNTIMES"):
    RUNTIMES = {name: RUNTIMES[name] for name in selected.split(",")}


def snapshot(directory):
    files = sorted(directory.iterdir())
    assert all(path.is_file() and not path.is_symlink() for path in files)
    assert all(path.name != ".lsm-writer" and not path.name.endswith(".writer") for path in files)
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in files}


def run(runtime, action, directory, mirrors):
    subprocess.run([*RUNTIMES[runtime], action, str(directory), mirrors], check=True)


with tempfile.TemporaryDirectory(prefix="clink-native-lsm-interop-") as temporary:
    root = Path(temporary)
    for mirrors in ["plain", "mirrored"]:
        for writer in RUNTIMES:
            source = root / f"{mirrors}-{writer}"
            source.mkdir()
            run(writer, "write", source, mirrors)
            original = snapshot(source)
            for reader in RUNTIMES:
                directory = root / f"{mirrors}-{writer}-{reader}"
                shutil.copytree(source, directory)
                run(reader, "read", directory, mirrors)
                assert snapshot(directory) == original, (writer, reader, "read changed physical bytes")
                run(reader, "mutate", directory, mirrors)
                advanced = snapshot(directory)
                run(writer, "read-advanced", directory, mirrors)
                assert snapshot(directory) == advanced, (writer, reader, "return read changed physical bytes")
                print(f"PASS complete-store {mirrors} {writer} -> {reader} -> {writer}", flush=True)
print(f"PASS {2 * len(RUNTIMES) ** 2} complete-store handoffs with physical hash preservation", flush=True)
