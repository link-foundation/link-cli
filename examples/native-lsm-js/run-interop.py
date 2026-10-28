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
LAYOUT = os.environ.get("NATIVE_LSM_LAYOUT", "whole")
assert LAYOUT in ("whole", "chunks", "reclaiming-chunks", "causal", "batch", "session", "allocation")
if LAYOUT != "whole":
    for name, command in RUNTIMES.items():
        if name == "rust":
            command[-1] = LAYOUT
        elif name == "csharp":
            command[-1] = "--native-lsm-" + LAYOUT
        else:
            command[-1] = f"examples/native-lsm-js/{LAYOUT if LAYOUT in ('causal', 'batch', 'session', 'allocation') else 'chunks'}-interop.mjs"


def snapshot(directory):
    entries = sorted(directory.rglob("*"))
    assert all(not path.is_symlink() and (path.is_file() or path.is_dir()) for path in entries)
    files = [path for path in entries if path.is_file()]
    assert all(path.name != ".lsm-writer" and not path.name.endswith(".writer") for path in files)
    return {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}


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
                print(f"PASS {LAYOUT} complete-store {mirrors} {writer} -> {reader} -> {writer}", flush=True)
print(f"PASS {2 * len(RUNTIMES) ** 2} {LAYOUT} complete-store handoffs with physical hash preservation", flush=True)
