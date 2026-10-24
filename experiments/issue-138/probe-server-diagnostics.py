"""Temporarily restore only server's panic-on-error traces for reproduction."""
import os
import resource
from pathlib import Path
import subprocess
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
source = Path('rust/src/protocol/server.rs')
original = source.read_text()
assert original.count('crate::diagnostics::stderr!') == 4
try:
    source.write_text(original.replace('crate::diagnostics::stderr!', 'eprintln!'))
    with open('experiments/issue-138/legacy-server-rust-before.log', 'w') as log:
        result = subprocess.run([
            'cargo', 'test', '--manifest-path', 'rust/Cargo.toml', '--all-features',
            '--test', 'diagnostic_output_tests', 'failed_stderr',
        ], env={**os.environ, 'CARGO_BUILD_JOBS':'1', 'CARGO_PROFILE_DEV_DEBUG':'0',
                'CARGO_PROFILE_TEST_DEBUG':'0'}, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode != 0, 'Server regression must fail without best-effort writes'
finally:
    source.write_text(original)
