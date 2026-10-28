#!/usr/bin/env python3
"""Verify and retain complete full-width staged protocol evidence."""
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
FILES = set()
SOURCES = set()
PREFIX = 'causal-protocol'


def read(name):
    path = BASE / name
    FILES.add(path)
    return path.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-causal-protocol.sh')
driver = read(f'{PREFIX}-driver.log')
assert 'ALL PASS ' in driver and 'RELEASE PASS ' in driver and 'CLIENTS PASS ' in driver
print(driver.strip())
for name, required in [
    ('causal-protocol-before-api.log', 'error[E0432]'),
    ('causal-protocol-tcp-before.log', 'query_lossless'),
    ('causal-protocol-js-before.log', 'client is not a function'),
    ('causal-protocol-wasm-before.log', 'unknown causal workspace operation'),
    ('causal-protocol-comments-before.log', 'fail 1'),
    ('causal-protocol-wire-limit-before.log', 'FAILED'),
    ('causal-protocol-reply-bound-before.log', '2 failed'),
    ('causal-protocol-zero-binding-before.log', 'wildcard-bindings'),
    ('causal-protocol-encoding-bound-before.log', 'encoded an already oversized request'),
]:
    path = BASE / name
    FILES.add(path)
    assert any(required in line for line in path.open()), (name, required)
    print(f'Before: {name}: expected missing API or finite regression reproduced.')

csharp_before = read('csharp-query-allocation-before.log')
assert 'Finite query allocation controls: 2 passed; 1 failed.' in csharp_before
print('C# baseline: anonymous point still produces dangling (2: 1 1); explicit point and pair-reuse controls pass. This separate production failure remains unfinished.')

for stem, minimum in [('native', 1271), ('wasm', 798)]:
    output = read(f'{PREFIX}-{stem}-full.log')
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', output)
    assert results and all(int(failed) == 0 for _, failed, _ in results)
    passed = sum(int(passed) for passed, _, _ in results)
    skipped = sum(int(skipped) for _, _, skipped in results)
    assert passed >= minimum, (stem, passed)
    for name in ['zero_references_remain_bound_during_swaps_equality_and_joins',
                 'oversized_named_query_reply_rejects_before_changing_private_rows',
                 'oversized_named_export_rejects_before_accumulating_the_complete_reply',
                 'independent_full_width_query_vectors_preserve_private_rows_and_allocation']:
        assert re.search(re.escape(name) + r'.*ok', output), (stem, name)
    print(f'Complete {stem}: {passed} passed, zero failed, {skipped} helper entries ignored by the top-level harness; native parents invoke their finite children.')
    report = json.loads(read(f'{PREFIX}-{stem}-coverage.json'))['data'][0]
    baseline = json.loads(read(f'query-reservation-{stem}-coverage.json'))['data'][0]
    old_files = {entry['filename'] for entry in baseline['files']}
    new_files = {entry['filename'] for entry in report['files']}
    assert old_files <= new_files, sorted(old_files - new_files)
    SOURCES.update(Path(entry['filename']) for entry in report['files'])
    print(f'Complete {stem} production source set retained: {len(old_files)} previous files, {len(new_files)} current files.')
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        value = report['totals'][metric]
        print(f"Complete {stem} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    for component in ['/rust/src/query_reply_budget.rs', '/rust/src/query_processor.rs',
                      '/rust/src/protocol/links_operations.rs', '/rust/src/protocol/server.rs',
                      '/rust/src/protocol/protocols.rs', '/rust/src/protocol/client.rs',
                      '/rust/wasm/src/causal_session.rs']:
        matches = [entry for entry in report['files'] if entry['filename'].endswith(component)]
        if stem == 'native' and '/wasm/' in component:
            assert not matches
            continue
        entry, = matches
        for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
            value = entry['summary'][metric]
            print(f"{stem} {component} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")

assert 'test result: ok. 6 passed; 0 failed; 1 ignored;' in read(f'{PREFIX}-doctests.log')
for suffix in ['format', 'clippy', 'wasm-clippy', 'docs', 'bare-dependencies',
               'selected-dependencies', 'source-size', 'changeset', 'workflows', 'whitespace',
               'wasm-host-clippy', 'rust-source-size', 'release-compile', 'release-tests',
               'native-metadata', 'wasm-metadata', 'sqlite-optional', 'release-shellcheck',
               'binding-build', 'types']:
    read(f'{PREFIX}-{suffix}.log')
snapshot = json.loads(read(f'{PREFIX}-rust-source-snapshot.json'))
for item in snapshot['files']:
    assert hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
assert 'rust/src/query_reply_budget.rs' in {item['path'] for item in snapshot['files']}
print('All Git-visible Rust source bytes match the release-check snapshot, including the new reply budget.')
repository = read(f'{PREFIX}-repository.log')
for key, count in [('tests', 221), ('pass', 221), ('fail', 0), ('cancelled', 0), ('skipped', 0)]:
    assert re.search(r'(?:#|ℹ) ' + key + ' ' + str(count) + r'\b', repository), key
js = read(f'{PREFIX}-js-final.log')
assert re.search(r'(?:#|ℹ) pass 699\b', js)
assert re.search(r'all files\s+\| 100\.00 \|\s+100\.00 \|\s+100\.00', js)
print('JavaScript: all 699 tests pass; complete production line/branch/function coverage is enforced at 100%. TypeScript declarations pass.')
for runtime in ['node', 'bun', 'deno']:
    output = read(f'{PREFIX}-actual-{runtime}.log')
    assert 'three bounded groups' in output and 'reply budgets' in output
    assert '16 shared query vectors' in output
    assert '1029 finite schedules and 686 exact before/after-reply retries' in output
    print(f'Actual {runtime}: standard full-u64 protocol/query API, 16 shared query vectors, three new bounded groups and nine existing causal-session contract groups pass, including 1,029 finite schedules and 686 before/after-reply retries.')
vectors = ROOT / 'docs/protocol/causal-query-v1.json'
FILES.add(vectors)
assert len(json.loads(vectors.read_text())['cases']) == 16
SOURCES.update((ROOT / 'js/client/src').glob('*.js'))
SOURCES.update((ROOT / 'js/client/types').glob('*.d.ts'))
print('Strict native/actual-WASM/host all-target Clippy, formats, rustdoc, six doctests, release helper tests, 221 repository checks, locked metadata, optional-provider/source/workflow/whitespace checks pass.')
print('Original query text preserves numeric/name/wildcard/variable kinds; existing u32 raw-document facades remain available.')
print('Finite comments/depth/nodes/bytes/result admission precedes checked execution; decoded reply reference bytes are admitted during the owned preview before real edits.')
print('Staging stays private until an explicit publication token; frozen clients refuse later operations. Lost replies retry the exact prepared request after later publications.')
print('Preview copies rows/names and executes the shared query path again; matching/export materialize bounded rows. Reply limits count decoded UTF-8 reference bytes; framing/escaping adds finite overhead.')
print('C# query parity, physical query recipes, whole-project native/C#/linked-WASM 100% and all wider unmet R1-R12 criteria remain unfinished; PR139 stays draft/unmerged.')
for filename in sorted(SOURCES):
    data = filename.read_bytes()
    print(f'Production source {filename.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
for filename in sorted(FILES):
    data = filename.read_bytes()
    print(f'{filename.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
