#!/usr/bin/env python3
"""Record reproductions and complete native/WASM query validation evidence."""
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'experiments/issue-138'
FILES = []
SOURCES = set()


def read(name):
    path = BASE / name
    FILES.append(path)
    return path.read_text()


print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Reproduce: bash experiments/issue-138/validate-query-reservations.sh')
print('Additional release gates: bash experiments/issue-138/validate-query-reservation-release.sh')
driver = read('query-reservation-driver.log')
assert 'ALL PASS ' in driver
print(driver.strip())
for name, message in [
    ('query-reservation-before.log', 'index: 2, source: 1, target: 1'),
    ('query-wide-name-before.log', 'right: Some(1)'),
    ('query-bindings-before.log', 'query result limit exceeded'),
    ('causal-query-name-before.log', 'checked_named_output_can_be_replayed'),
    ('query-session-api-before.log', 'no method named `named_links`'),
]:
    output = read(name)
    assert message in output and ('FAILED' in output or 'error[E0599]' in output)
    print(f'Before: {name}: expected regression reproduced.')

for stem, expected, ignored in [('native', 1257, 22), ('wasm', 785, 0)]:
    output = read(f'query-reservation-{stem}-full.log')
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', output)
    assert results and all(int(failed) == 0 for _, failed, _ in results)
    passed = sum(int(passed) for passed, _, _ in results)
    skipped = sum(int(skipped) for _, _, skipped in results)
    assert passed == expected and skipped == ignored, (stem, passed, skipped)
    print(f'Complete {stem}: {passed} passed, zero failed, {skipped} helper entries ignored by the top-level harness (native parents invoke their finite children).')
    report = json.loads(read(f'query-reservation-{stem}-coverage.json'))['data'][0]
    baseline = json.loads(read(f'causal-session-coverage-{stem}.json'))['data'][0]
    old_files = {entry['filename'] for entry in baseline['files']}
    SOURCES.update(Path(entry['filename']) for entry in report['files'])
    new_files = {entry['filename'] for entry in report['files']}
    assert old_files <= new_files, sorted(old_files - new_files)
    print(f'Complete {stem} production source set retained: {len(old_files)} previous files, {len(new_files)} current files.')
    for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
        value = report['totals'][metric]
        print(f"Complete {stem} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    for component in ['/rust/src/engine/query_scratch.rs', '/rust/src/engine/query_reservation.rs',
                      '/rust/src/engine/causal_named.rs', '/rust/src/engine/causal_session.rs',
                      '/rust/src/query_processor.rs', '/rust/src/query_processor/matching.rs']:
        entry, = [entry for entry in report['files'] if entry['filename'].endswith(component)]
        for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
            value = entry['summary'][metric]
            print(f"{stem} {component} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")

doctests = read('query-reservation-doctests.log')
assert 'test result: ok. 6 passed; 0 failed; 1 ignored;' in doctests
print('Six doctests pass; one existing illustrative example ignored.')
repository = read('query-reservation-repository.log')
for key, count in [('tests', 221), ('pass', 221), ('fail', 0), ('cancelled', 0), ('skipped', 0)]:
    assert re.search(r'(?:#|ℹ) ' + key + ' ' + str(count) + r'\b', repository), key
for suffix in ['format', 'clippy', 'wasm-clippy', 'docs', 'bare-dependencies',
               'selected-dependencies', 'source-size', 'changeset', 'workflows', 'whitespace']:
    read(f'query-reservation-{suffix}.log')
archive_control = read('query-reservation-rust-source-size-archive-before.log')
assert '1325 lines (exceeds 1000)' in archive_control
snapshot = json.loads(read('query-reservation-rust-source-snapshot.json'))
assert snapshot['files']
print('The unchanged source-size checker runs on all Git-visible Rust inputs; two ignored historical checkout copies are retained outside that snapshot.')
release = read('query-reservation-release-driver.log')
assert 'RELEASE PASS ' in release
print(release.strip())
for suffix in ['wasm-host-clippy', 'rust-source-size', 'release-compile', 'release-tests', 'native-metadata', 'wasm-metadata', 'sqlite-optional', 'release-shellcheck']:
    read(f'query-reservation-{suffix}.log')
release_tests = read('query-reservation-release-tests.log')
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', release_tests)
assert results and all(int(failed) == 0 for _, failed, _ in results)
print('Rust release helper tests:', sum(int(passed) for passed, _, _ in results), 'passed.')
print('Strict native/WASM all-target Clippy, both formats, rustdoc, 221 repository tests, actual Rust release helpers, locked metadata, optional-provider/source/workflow/whitespace checks pass.')
kinds = [json.loads(line) for line in read('query-document-kinds.log').splitlines()]
assert len(kinds) == 5 and all(case['kindsLost'] for case in kinds)
print('Five finite canonical-document controls reproduce reference-kind loss. This protocol boundary remains unfinished in this step.')
print('Bare dependency tree has no JSON/SQLite/SHA; selected causal sessions add no SQLite/SHA.')
print('Shared execution previews reserve actual allocation order without publishing probe rows. Rejection/rollback can leave gaps.')
print('An eight-thread native test verifies disjoint ordinary transaction reservations. Actual WASM runs the portable contracts.')
print('Causal queries retain u64::MAX with a checked u128 marker, preserve private staging until explicit-token publication, and bound input/result/binding growth.')
print('Existing recycling stores keep the legacy prediction fallback and its documented gaps. Preview copies rows/names and repeats query execution.')
print('C#/JS/protocol query parity, complete-project 100% and every wider unmet R1-R12 criterion remain unfinished; PR139 stays draft/unmerged.')
for filename in sorted(SOURCES):
    data = filename.read_bytes()
    print(f'Production source {filename.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
for filename in sorted(set(FILES)):
    data = filename.read_bytes()
    print(f'{filename.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
