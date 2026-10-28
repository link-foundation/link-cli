#!/usr/bin/env python3
"""Finite five-runtime wire exchange and reaped process-exit composition."""
import hashlib
import os
from pathlib import Path
import resource
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
RUNTIMES = {
    "node": ["node", "--max-old-space-size=128", "examples/native-lsm-js/merkle-interop.mjs"],
    "bun": ["bun", "examples/native-lsm-js/merkle-interop.mjs"],
    "deno": ["deno", "run", "--allow-all", "--v8-flags=--max-old-space-size=128", "examples/native-lsm-js/merkle-interop.mjs"],
    "csharp": ["dotnet", os.environ.get("CLUSTER_CSHARP_EXAMPLE", "examples/cluster-csharp/bin/Release/net10.0/ClusterExample.dll"), "--native-lsm-merkle"],
    "rust": [os.environ.get("RUST_NATIVE_LSM_EXAMPLE", "rust/target/debug/examples/native_lsm"), "merkle"],
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


def run(runtime, action, directory, mirrors, message=None, expected=0, causal=False):
    command = list(RUNTIMES[runtime])
    if causal:
        command[-1] = "causal" if runtime == "rust" else "--native-lsm-causal" if runtime == "csharp" else "examples/native-lsm-js/causal-interop.mjs"
    def limits():
        resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
        if runtime == "rust":
            resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024,) * 2)
    result = subprocess.run([*command, action, str(directory), mirrors, *([str(message)] if message else [])],
                            env=dict(os.environ, DOTNET_GCHeapHardLimit="0x28000000"), preexec_fn=limits, check=False)
    assert result.returncode == expected, (runtime, action, result.returncode, expected)


with tempfile.TemporaryDirectory(prefix="clink-merkle-interop-") as temporary:
    root = Path(temporary)
    for mirrors in ("plain", "mirrored"):
        for writer in RUNTIMES:
            source = root / f"source-{writer}-{mirrors}"; source.mkdir()
            run(writer, "write", source, mirrors, causal=True)
            original = snapshot(source); message = root / f"{writer}-{mirrors}.messages"
            run(writer, "export", source, mirrors, message)
            assert snapshot(source) == original, "export changed source physical bytes"
            bundle = message.read_bytes(); assert len(bundle) <= 524288
            for reader in RUNTIMES:
                destination = root / f"destination-{writer}-{reader}-{mirrors}"; destination.mkdir()
                run(reader, "seed", destination, mirrors, message)
                run(reader, "pull", destination, mirrors, message)
                received = snapshot(destination)
                run(writer, "read-pulled", destination, mirrors, message)
                run(reader, "pull", destination, mirrors, message)
                assert snapshot(destination) == received, "proof/retry changed already accepted physical bytes"
                run(reader, "following", destination, mirrors, message)
                advanced = snapshot(destination)
                run(writer, "read-following", destination, mirrors, message)
                assert snapshot(destination) == advanced and snapshot(source) == original and message.read_bytes() == bundle
                print(f"PASS Merkle {mirrors} {writer} messages -> {reader} concurrent/following -> {writer}", flush=True)
        # Use one verified immutable bundle for finite before/after publication cuts.
        for writer in RUNTIMES:
            seed = root / f"cut-seed-{writer}-{mirrors}"; seed.mkdir()
            run(writer, "seed", seed, mirrors, message)
            for phase in ("before", "after"):
                cut = root / f"cut-{writer}-{mirrors}-{phase}"; shutil.copytree(seed, cut)
                run(writer, f"cut-{phase}", cut, mirrors, message, expected=73)
                assert (cut / ".lsm-writer").is_file(); crashed = snapshot(cut)
                for reader in RUNTIMES:
                    run(reader, "blocked", cut, mirrors, causal=True)
                    assert snapshot(cut) == crashed, "blocked open changed evidence"
                # Every exited child was reaped above. Preserve stale markers outside the store.
                evidence = root / f"evidence-{writer}-{mirrors}-{phase}"; evidence.mkdir()
                for marker in cut.iterdir():
                    if marker.name == ".lsm-writer" or marker.name.endswith(".writer"):
                        marker.rename(evidence / marker.name)
                assert (evidence / ".lsm-writer").is_file()
                for reader in RUNTIMES:
                    recovered = root / f"recovered-{writer}-{reader}-{mirrors}-{phase}"; shutil.copytree(cut, recovered)
                    run(reader, "pull", recovered, mirrors, message)
                    received = snapshot(recovered)
                    run(writer, "read-pulled", recovered, mirrors, message)
                    run(reader, "pull", recovered, mirrors, message)
                    assert snapshot(recovered) == received, "recovered duplicate delivery republished"
                    run(reader, "following", recovered, mirrors, message)
                    advanced = snapshot(recovered)
                    run(writer, "read-following", recovered, mirrors, message)
                    assert snapshot(recovered) == advanced and message.read_bytes() == bundle
                    print(f"PASS Merkle {mirrors} {writer} exit-{phase} -> {reader} retry/following -> {writer}", flush=True)
print(f"PASS {2 * len(RUNTIMES) ** 2} Merkle message handoffs; {4 * len(RUNTIMES)} reaped exits; {4 * len(RUNTIMES) ** 2} recovered cross-runtime handoffs")
print("Explicit stale-marker quarantine follows reaping; application exits do not certify hardware power loss.")
