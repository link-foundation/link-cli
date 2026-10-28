#!/usr/bin/env python3
"""Independent, finite final-row session vectors; no production imports."""
import itertools
import json
from pathlib import Path
import resource
import sys

resource.setrlimit(resource.RLIMIT_AS, (192 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_STACK, (8 * 1024 * 1024,) * 2)
initial = {1: (0, 2, 'old'), 3: (1, 1, None), 4: (0, 2, None)}
cases = []
for sequence in itertools.product(('create', 'rename', 'update', 'delete', 'restore', 'cancel-create', 'noop'), repeat=3):
    rows, cursor, errors = dict(initial), 32, []
    for operation in sequence:
        invalid = False
        if operation == 'create':
            if cursor > 33:
                invalid = True
            else:
                rows[cursor] = (0, 2, None)
                cursor += 1
        elif operation in ('rename', 'update', 'delete'):
            if 1 not in rows:
                invalid = True
            elif operation == 'rename':
                rows[1] = (*rows[1][:2], 'new\ufeff\0🙂')
            elif operation == 'update':
                rows[1] = (2, 0, rows[1][2])
            else:
                del rows[1]
        elif operation == 'restore':
            rows[1] = initial[1]
        elif operation == 'cancel-create':
            # An absent row is a no-op in this reference operation.
            rows.pop(32, None)
        errors.append(invalid)
    changed = [address for address in sorted(initial.keys() | rows.keys()) if initial.get(address) != rows.get(address)]
    cases.append({'operations': sequence, 'errors': errors, 'rows': [[str(address), str(source), str(target), name] for address, (source, target, name) in sorted(rows.items())], 'changed': list(map(str, changed))})
result = {'format': 'link-cli-causal-session-v1', 'cases': cases}
Path(sys.argv[1]).write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(f'PASS generated {len(cases)} independent finite three-operation schedules.')
