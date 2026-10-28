#!/usr/bin/env python3
"""Verify saved complete compaction reports before publishing their counts."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
prefix = 'engine-compaction-'
print('Engine-lock compaction; PR https://github.com/link-foundation/link-cli/pull/139')
print('Baseline: 652ce129a3d7f1032d25da94b2472aceb41dd49e; missing-feature controls in engine-compaction-before.txt.')
print('Validation: bash experiments/issue-138/validate-engine-compaction.sh <stage>')
print('Sequential compilers; Cargo jobs=1, reduced debug, .NET SDK10.0.403/640MiB heap, actual WASM256MiB cap.')
for stage in ('csharp', 'native', 'wasm', 'js', 'repository', 'cluster', 'interop'):
    log = (base / f'{prefix}{stage}-validation.log').read_text()
    assert log.endswith(f'Engine-compaction {stage} stage: passed\n'), (stage, log[-2048:])
    print(f'{stage}: complete stage passed')
for title, stem in [('Native Rust', 'native'), ('Actual WASM', 'wasm')]:
    log = (base / f'{prefix}{stem}-full.log').read_text().partition('   Doc-tests link_cli')[0]
    results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    assert results and all(int(f) == 0 for _, f, _ in results)
    assert 'compaction_preserves_pessimistic_ownership_exclusive_admission_and_stale_dependencies ... ok' in log
    assert 'engine_compaction_composes_with_both_bank_modes_delta_state_history_and_encoding ... ok' in log
    print(f'\n{title}: {sum(int(p) for p, _, _ in results)} passed, {sum(int(i) for _, _, i in results)} ignored child entry points.')
    data = json.loads((base / f'{prefix}{stem}-coverage.json').read_text())['data'][0]
    for metric in ('lines', 'branches', 'functions'):
        value = data['totals'][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for module in ('checkpoint.rs', 'delta.rs'):
        files = [f for f in data['files'] if f['filename'].endswith('/rust/src/engine/' + module)]
        assert len(files) == 1
        for metric in ('lines', 'branches', 'functions'):
            value = files[0]['summary'][metric]
            print(f"engine/{module} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    if stem == 'wasm':
        for name in ('binding_sessions_and_journal_round_trip', 'invalid_policies_are_not_silently_ignored',
                     'executes_queries_with_the_rust_core', 'deleting_a_link_deletes_its_usages_as_the_cli_does',
                     'updating_a_link_into_an_existing_pair_merges_them_as_the_cli_does', 'reset_empties_the_store',
                     'answers_links_protocol_requests_like_the_server', 'rejects_invalid_options',
                     'memory_replacement_trait_rejects_without_mutation_and_keeps_ordinary_io',
                     'external_replacement_trait_preserves_boolean_results_and_rejects_invalid_acknowledgement'):
            assert re.search(r'test [^\n]*::' + name + r' \.\.\. ok', log), name
        print('All ten wrapper unit controls execute in the actual WASM runner; native registration is retained.')
        wrappers = [f for f in data['files'] if '/rust/wasm/src/' in f['filename']]
        assert len(wrappers) == 3
        for metric in ('lines', 'branches', 'functions', 'regions', 'instantiations'):
            covered = sum(f['summary'][metric]['covered'] for f in wrappers)
            count = sum(f['summary'][metric]['count'] for f in wrappers)
            print(f'Wrapper-only {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
print('Doctests: ' + re.findall(r'test result: ok\.[^\n]+', (base / f'{prefix}doctests.log').read_text())[-1])
for mode in ('Debug', 'Release'):
    log = (base / f'{prefix}csharp-{mode}-full.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', log)
    assert match and match[1] == match[2]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', log, re.M)[-1])
    coverage = ET.parse(report).getroot()
    print(f'\nC# {mode}: {match[1]} passed, 0 failed/skipped.')
    for metric in ('lines', 'branches'):
        covered, count = [int(coverage.attrib[f'{metric}-{key}']) for key in ('covered', 'valid')]
        print(f'Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)')
    classes = [c for c in coverage.findall('.//class') if c.attrib['filename'].endswith(('/Engine/JournalCheckpoint.cs', '/Engine/DeltaCommitLog.cs'))]
    assert classes
    for item in classes:
        print(f"{item.attrib['name']}: lines {item.attrib['line-rate']}; branches {item.attrib['branch-rate']}")
    followup = (base / f'{prefix}csharp-{mode}-followup.log').read_text()
    match = re.search(r'Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)', followup)
    assert match and match[1] == match[2]
    report = Path(re.findall(r'^\s+(\S+coverage\.cobertura\.xml)$', followup, re.M)[-1])
    components = [c for c in ET.parse(report).getroot().findall('.//class')
                  if c.attrib['filename'].endswith(('/Engine/JournalCheckpoint.cs', '/Engine/DeltaCommitLog.cs'))]
    assert components
    for item in components:
        assert float(item.attrib['line-rate']) == float(item.attrib['branch-rate']) == 1, item.attrib
    print(f'Final C# {mode} delta/compaction follow-up: {match[1]} passed; both components100% lines/branches.')
    print('Follow-up adds rejection/reuse for a non-replaceable inner log; complete-suite totals above remain the unmerged reports.')
for title, suffix in [('JS', 'js-full'), ('Repository', 'repository')]:
    log = (base / f'{prefix}{suffix}.log').read_text()
    totals = {key: int(re.search(r'(?:#|ℹ) ' + key + r' (\d+)\b', log)[1]) for key in ('tests', 'pass', 'fail', 'skipped')}
    assert totals['tests'] == totals['pass'] and totals['fail'] == totals['skipped'] == 0
    if title == 'JS':
        assert re.search(r'all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|', log)
    print(title + ': ' + str(totals))
reference = None
for runtime in ('node', 'bun', 'deno'):
    composition = json.loads((base / f'{prefix}{runtime}-compact.json').read_text())
    assert composition['cases'] == len(composition['results']) == 72
    if reference is not None:
        assert reference == composition
    reference = composition
    conformance = (base / f'{prefix}{runtime}-conformance.log').read_text()
    assert '# 8 passed, 0 failed, 4 skipped' in conformance
    assert 'ok wasm native pages: 48 independent policies' in conformance
    print(f'{runtime}: identical72-case actual-WASM compaction report; conformance and48 existing independent policies pass.')
print('Engine lock retained across construction/publication; native bounded concurrent commits wait for replacement.')
print('History, pins, committed/private allocators, following groups, locks/admission, read sets and saved branch bases preserved.')
print('Unsupported rejection preserves usability; before/after errors fence engine/codec and reopen complete old/new streams.')
print('All72 bank/state/encoding/history/delta cases pass in native, C# and each actual-WASM runtime.')
print('Cluster checkpoint modes0/1 and1+64KiB cache; existing48 native configurations/288 handoffs pass.')
checkpoint_interop = (base / f'{prefix}checkpoint-interop.log').read_text()
assert 'PASS checkpoint pages: 48 configurations, 288 persistent writer handoffs' in checkpoint_interop
print('Initial-checkpoint native control: another48 configurations/288 handoffs, revision40->52, closed mirrored repair.')
print('Strict format/lint, TypeScript, vector regeneration, optional-SQL, release/source/workflow checks pass.')
print('Native owning factories/process recovery, whole-project100% coverage and every wider unmet issue criterion remain unfinished.')
print('Truthful device synchronization is assumed; cloned images and process exits do not establish hardware power-loss durability.')
