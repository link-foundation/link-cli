#!/usr/bin/env python3
"""Generate finite independently specified full-u64 query parity vectors."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]


def row(index, source, target, name=None):
    return dict(index=str(index), source=str(source), target=str(target), name=name)


def case(name, queries, expected, next_id, seed=None, edits=1):
    return dict(name=name, first='1', last='8', seed=seed or [], queries=queries,
                rows=expected, next=str(next_id), edits=edits)


def query(text, error=False):
    return dict(text=text, error=error)


maximum = 18446744073709551615
cases = [
    case('anonymous-point', [query('(() ((1 1)))')], [row(1, 1, 1)], 2),
    case('anonymous-forward-reference', [query('(() ((1 2) (2 2)))')],
         [row(1, 1, 2), row(2, 2, 2)], 3, edits=2),
    case('explicit-high-pair-reuse', [query('() ((4: 4 4) (4 4))')],
         [row(4, 4, 4)], 5),
    case('maximum-retained-identity',
         [query(f'(({maximum}: $s $t)) (({maximum}: $t $s))'),
          query(f'() ((1: {maximum} 1))')],
         [row(1, maximum, 1), row(maximum, maximum, maximum)], 2,
         seed=[row(maximum, maximum, maximum)]),
    case('wildcard-bindings', [query('(($i: $s $t)) (($i: $t $s))')],
         [row(100, 100, 100), row(101, 0, 100)], 1,
         seed=[row(100, 100, 100), row(101, 100, 0)]),
    case('zero-binding-equality', [query('(($i: $x $x)) ()')],
         [row(101, 0, 100)], 1,
         seed=[row(100, 100, 100), row(101, 0, 100)]),
    case('zero-binding-join', [query('((100: $x *) (101: $x *)) ((100: $x 100))')],
         [row(100, 0, 100), row(101, 100, 100)], 1,
         seed=[row(100, 0, 100), row(101, 100, 100)], edits=0),
]
for name, reference in [('negative', '-1'), ('overflow', str(maximum + 1)),
                        ('outside-reservation', '100')]:
    cases.append(case(name, [query(f'() ((1: 1 1) ({reference}: 0 0))', True)],
                      [], 1, edits=0))
for name in ['4294967312', str(maximum), '*', '$x', '', '\'"雪\0\n']:
    reference = '~1{' + name.encode().hex() + '}'
    cases.append(case('literal-name-' + repr(name),
                      [query(f'# ignored )\n(({reference}: $s $t)) (({reference}: $t $s))')],
                      [row(100, 0, 100, name)], 1, seed=[row(100, 100, 0, name)]))
destination = root / 'docs/protocol/causal-query-v1.json'
destination.write_text(json.dumps(dict(version=1, cases=cases), indent=2, ensure_ascii=False) + '\n')
print(f'Wrote {len(cases)} finite cases to {destination.relative_to(root)}')
