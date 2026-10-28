"""Bounded regression: remove only pending guards, then restore them in finally."""
import os
from pathlib import Path
import subprocess
source = Path('csharp/Foundation.Data.Doublets.Cli.Library/TransactionsDecorator.cs')
original = source.read_text()
guards = [
    '                if (!_committed.Contains(head.TransactionId)) break;\n',
    '                if (!_committed.Contains(transition.TransactionId)) return;\n',
]
assert all(original.count(guard) == 1 for guard in guards)
try:
    probe = original
    for guard in guards:
        probe = probe.replace(guard, '')
    source.write_text(probe)
    with open('experiments/issue-138/retention-pending-csharp-before.log', 'w') as log:
        result = subprocess.run([
            'dotnet', 'test', 'csharp', '--configuration', 'Release',
            '--disable-build-servers', '-m:1', '--filter',
            'FullyQualifiedName~BackgroundRetentionLeavesPendingTransactionsUnapplied',
        ], env={**os.environ, 'DOTNET_GCHeapHardLimit': '0x28000000'}, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode != 0, 'Both regressions must fail without the guards'
finally:
    source.write_text(original)
