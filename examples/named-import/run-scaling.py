#!/usr/bin/env python3
"""Bounded issue-110 benchmark; build named_import_scaling in release mode first.

Usage: python3 examples/named-import/run-scaling.py [path/to/named_import_scaling]
Each child has a 25-second deadline. Run memory-stress probes separately with
an OS memory limit; this fixture is always limited to at most 10,000 links.
"""
import pathlib
import subprocess
import sys

binary = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else
                      'rust/target/release/examples/named_import_scaling').resolve()
for algorithm in ('scan', 'indexed'):
    for mode in ('numeric', 'named'):
        for count in (100, 1000, 10000):
            args = [str(binary), mode, str(count)]
            if algorithm == 'indexed':
                args.append('indexed')
            try:
                result = subprocess.run(args, capture_output=True, text=True,
                                        timeout=25, check=True)
                print(algorithm, result.stdout.strip().splitlines()[-1], flush=True)
            except subprocess.TimeoutExpired:
                print(f'{algorithm} {mode},{count}: exceeded 25s', flush=True)
