#!/usr/bin/env python3
"""Two focused physical handoffs; complete five-runtime matrix stays in CI."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
commands = {
    'rust': ['rust/target/debug/examples/native_lsm', 'allocation'],
    'csharp': ['dotnet', 'examples/cluster-csharp/bin/Release/net10.0/ClusterExample.dll', '--native-lsm-allocation'],
    'node': ['node', '--max-old-space-size=128', 'examples/native-lsm-js/allocation-interop.mjs'],
}

def snapshot(directory):
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(directory.iterdir())}

def run(runtime, action, directory, mirrors):
    subprocess.run([*commands[runtime], action, str(directory), mirrors], check=True,
                   env=dict(os.environ, DOTNET_GCHeapHardLimit='0x28000000', DOTNET_PROCESSOR_COUNT='2'))

with tempfile.TemporaryDirectory(prefix='clink-address-allocation-') as temporary:
    for mirrors in ['plain', 'mirrored']:
        directory = Path(temporary) / mirrors
        directory.mkdir()
        run('rust', 'write', directory, mirrors)
        initial = snapshot(directory)
        run('csharp', 'read', directory, mirrors)
        assert snapshot(directory) == initial
        run('node', 'mutate', directory, mirrors)
        advanced = snapshot(directory)
        for reader in ['rust', 'csharp']:
            run(reader, 'read-advanced', directory, mirrors)
            assert snapshot(directory) == advanced
        print(f'PASS {mirrors} Rust -> C# -> Node -> Rust/C# with unchanged retained-retry hashes', flush=True)
print('PASS 2 focused complete-store allocation handoffs')
