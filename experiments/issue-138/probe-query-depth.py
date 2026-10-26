#!/usr/bin/env python3
"""Finite public-command query-depth controls, with capped Linux child resources."""
import os
import resource
import sys
import subprocess
import tempfile
from pathlib import Path

command = sys.argv[1:] or [str(Path('rust/target/debug/clink').resolve())]
is_dotnet = Path(command[0]).name == 'dotnet'
for depth in (2, 32, 62, 63, 128, 2048):
    # Seed two valid points, then build an anonymous source-spine chain.
    tree = '(' * depth + '1' + ' 2)' * depth
    query = '() (' + tree + ')'
    with tempfile.TemporaryDirectory(prefix='finite-query-') as directory:
        output = Path(directory, 'stdout')
        error = Path(directory, 'stderr')
        def limits():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
            if not is_dotnet:
                resource.setrlimit(resource.RLIMIT_AS, (128 * 1024 * 1024,) * 2)
            resource.setrlimit(resource.RLIMIT_STACK, (512 * 1024,) * 2)
            resource.setrlimit(resource.RLIMIT_CPU, (4, 4))
        environment = dict(os.environ, DOTNET_GCHeapHardLimit='0x08000000',
            DOTNET_DbgEnableMiniDump='0', COMPlus_DbgEnableMiniDump='0')
        seed = subprocess.run(command + ['--db', str(Path(directory, 'data.links')),
            '() ((1: 1 1) (2: 2 2))'], capture_output=True, preexec_fn=limits, env=environment, timeout=10)
        assert seed.returncode == 0, seed.stderr
        with output.open('wb') as stdout, error.open('wb') as stderr:
            child = subprocess.Popen(command + ['--db', str(Path(directory, 'data.links')),
                '--auto-create-missing-references', '--after', query],
                stdout=stdout, stderr=stderr, preexec_fn=limits, env=environment)
            try:
                code = child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
                raise
        print(f'depth={depth} exit={code} stdout_bytes={output.stat().st_size} stderr_bytes={error.stat().st_size}')
        print(error.read_bytes()[:2048].decode(errors='replace'))
        if code == 0:
            rows = output.read_text().splitlines()
            assert len(rows) == depth + 2, rows[:3]
            assert rows[-1] == f'({depth + 2}: {depth + 1} 2)', rows[-3:]
        else:
            assert code == 1 and b'nesting' in error.read_bytes().lower(), (code, error.read_text())
            # Reopen the same file: rejection must precede every row effect.
            following = subprocess.run(command + ['--db', str(Path(directory, 'data.links')),
                '--after', '() ((3: 3 3))'], capture_output=True, preexec_fn=limits,
                env=environment, timeout=10)
            assert following.returncode == 0, following.stderr
            assert following.stdout.decode().splitlines() == [
                '(1: 1 1)', '(2: 2 2)', '(3: 3 3)'], following.stdout
            print('following reopening and edit: exact three rows preserved')
