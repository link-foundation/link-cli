#!/usr/bin/env python3
"""Build the actual current JS binding sequentially with a finite WASM memory cap."""
import os
from pathlib import Path
import resource
import subprocess
import sys

os.chdir(Path(__file__).resolve().parents[2])
environment = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_DEBUG='0',
                   RUSTFLAGS='-Clink-arg=--max-memory=268435456')
result = subprocess.run(['cargo', 'build', '--manifest-path', 'rust/wasm/Cargo.toml',
                         '--target', 'wasm32-unknown-unknown'], env=environment)
print('Build maximum child RSS (KiB):', resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss, flush=True)
if result.returncode:
    sys.exit(result.returncode)
subprocess.run([os.environ.get('WASM_BINDGEN_BIN', 'wasm-bindgen'),
                'rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm', '--target', 'web',
                '--out-dir', 'experiments/issue-138/native-lsm-wasm-package'], check=True)
