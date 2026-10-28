#!/usr/bin/env python3
"""Finite real-provider control for selecting a nonzero checkpoint seed."""
import os
import subprocess
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parent.parent.parent
os.chdir(root)
fixture = '--checkpoint-fixture=' + str(root / 'docs/protocol/engine-checkpoint-vectors.json')
commands = {
    'Rust': [str(root / 'rust/target/debug/examples/engine_native_pages')],
    'C#': ['dotnet', str(root / 'examples/native-pages-csharp/bin/Debug/net10.0/NativePagesExample.dll')],
    'JS': ['node', 'js/client/examples/engine-native-pages.mjs'],
}
failed = []
with tempfile.TemporaryDirectory(prefix='checkpoint-paged-entrypoints-') as directory:
    for language, command in commands.items():
        completed = subprocess.run(command + [str(Path(directory) / language.replace('#', 'sharp')), '--delta', fixture],
                                   text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        print(language, 'exit', completed.returncode, flush=True)
        print(completed.stdout, end='', flush=True)
        if completed.returncode or 'Native checkpoint pages group: 40 -> 42 revisions; 32 -> 34 rows;' not in completed.stdout:
            failed.append(language)
if failed:
    raise AssertionError('Checkpoint seed was not verified by: ' + ', '.join(failed))
print('PASS all three real page entry points: initial checkpoint plus following physical delta group.')
