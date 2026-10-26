#!/usr/bin/env python3
"""Preserve portable archive contracts and complete official coverage totals."""
import json
import re
import subprocess
from pathlib import Path

base = Path(__file__).resolve().parent
root = base.parent.parent
baseline = '847ed01dd663fa43dfbac67072b812680c56e46c'
production = ['rust/src', 'rust/wasm/src', 'csharp/Foundation.Data.Doublets.Cli.Library',
              'js/client/src']
assert not subprocess.check_output(['git', 'diff', baseline, '--', *production])
old = subprocess.check_output(['git', 'show', baseline + ':rust/tests/store_archive_tests.rs'], text=True)
prior = set(re.findall(r'#\[test\]\nfn (\w+)\(', old))
native = set(re.findall(r'#\[test\]\nfn (\w+)\(', (root / 'rust/tests/store_archive_tests.rs').read_text()))
shared = set(re.findall(r'#\[cfg_attr\(not\(target_arch = "wasm32"\), test\)\]\nfn (\w+)\(',
                       (root / 'rust/tests/store_archive_portable_tests.rs').read_text()))
assert len(prior) == 16 and len(native) == 6 and len(shared) == 10
assert prior == native | shared and not native & shared
focused = (base / 'archive-stream-focused-native.log').read_text()
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;', focused)
assert sorted(int(p) for p, _ in results) == [4, 6, 10] and all(int(f) == 0 for _, f in results)
print('Portable archive and stream contracts; PR https://github.com/link-foundation/link-cli/pull/139')
print('Unchanged production baseline: ' + baseline)
print('Run: bash experiments/issue-138/validate-archive-stream.sh <native|wasm|repository>')
print('One compiler job, reduced debug information, actual WASM capped at 256 MiB.')
print('All 16 original native archive test names are preserved: ten shared and six native-only.')
print('All 20 focused native cases pass before any further production change; no new defect is claimed.')
print('Four new portable groups check 31 reader failures (including final EOF), 30 truncated prefixes,')
print('three trailing bytes, 30 partial-write failures, one-byte reads/writes, retries and following writes.')
print('All probes use a 30-byte independently authored cross-language golden archive and small stores.')
print('One WASM-only file-helper control checks unsupported filesystem errors with exact path context,')
print('unchanged rows/names and successful following in-memory archive use.')
print('Read/parse rejection preserves rows and names; output errors retain only the expected byte prefix.')
print('Atomic rollback after storage errors and output flush durability are separate owner obligations.')
for stage in ('native', 'wasm', 'repository'):
    log = (base / f'archive-stream-{stage}-validation.log').read_text()
    assert log.endswith(f'Archive-stream {stage} stage: passed\n'), (stage, log)
for label, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'archive-stream-{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    print(f'\n{label}: {sum(int(p) for p, _, _ in results)} passed, 0 failed; '
          f'{sum(int(i) for _, _, i in results)} ignored child entry points.')
    report = base / f'archive-stream-{stem}-coverage.json'
    data = json.loads(report.read_text())['data'][0]
    archive = next(f for f in data['files'] if f['filename'].endswith('/protocol/archive.rs'))
    for title, summary in [('Complete production', data['totals']), ('protocol/archive.rs', archive['summary'])]:
        print(title + ':')
        for metric in ('lines', 'branches', 'functions'):
            value = summary[metric]
            print(f"  {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'wasm':
        assert ('store_archive::file_helpers_report_unsupported_filesystem_without_changing_the_store ... ok'
                in log)
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        print('Three wrapper modules, separate from complete linked totals:')
        for metric in ('lines', 'branches', 'functions'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            total = sum(f['summary'][metric]['count'] for f in wrappers)
            assert covered == total
            print(f'  {metric}: {covered}/{total} (100%)')
    print('Report: ' + str(report.relative_to(root)))
print('Native doctests: ' + re.findall(r'test result: ok\.[^\n]+',
      (base / 'archive-stream-doctests.log').read_text())[-1])
log = (base / 'archive-stream-repository.log').read_text()
totals = {field: int(re.search(r'(?:#|ℹ) ' + field + r' (\d+)\b', log)[1])
          for field in ('tests', 'pass', 'fail', 'skipped')}
assert totals['tests'] == totals['pass'] == 221 and totals['fail'] == totals['skipped'] == 0
print('Repository: ' + str(totals))
print('Strict native/WASM Clippy, formatting, source/release checks, optional-SQL guards, ShellCheck and whitespace pass.')
print('Unchanged C#/JS production retains the complete import-allocation.txt validation:')
print('2,172 C# tests per Debug/Release configuration, 209 JS at enforced 100% lines/branches/functions,')
print('Node/Bun/Deno actual-WASM conformance and 48 configurations / 288 cross-language file writers.')
print('These C#/JS checks are the preceding batch, not new executions claimed by this test-only batch.')
print('Whole-project 100% coverage and every wider unfinished issue requirement remain active.')
