#!/usr/bin/env python3
"""Preserve the bounded library-placeholder control and native classification."""
import hashlib
from pathlib import Path

root = Path(__file__).resolve().parent
source = Path('/tmp/issue-138-recovery/rust-1.99-unused-coverage.rs')
parts = [
    'Rust library coverage placeholders, 2026-10-28 UTC.\n'
    'PR: https://github.com/link-foundation/link-cli/pull/139\n'
    'Reproduce: bash experiments/issue-138/coverage-aggregation/library-instantiations/run.sh\n'
    'One tiny library and two finite consumers assert exact values for both\n'
    'branches of each concrete type. Every official library source counter\n'
    'executes, while a zero-count generic placeholder remains. Adding the\n'
    'second concrete type changes instantiations from 2/3 to 3/4.\n'
    'Pinned compiler source documents this library/downstream behavior:\n'
    'https://github.com/rust-lang/rust/blob/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/compiler/rustc_codegen_llvm/src/coverageinfo/mapgen/unused.rs#L79\n'
    f'Downloaded source SHA256 {hashlib.sha256(source.read_bytes()).hexdigest()}\n'
    'The Rust v0 symbol specification defines type p as a placeholder:\n'
    'https://doc.rust-lang.org/rustc/symbol-mangling/v0.html#type\n'
    'The unchanged full focused author-guard export contains 22 executed\n'
    'entries and 16 zero-count names with placeholder provider types. Every\n'
    'executed entry covers its recorded regions. This matches the compiler\n'
    'behavior; it does not establish that unrelated uncovered code executes.\n'
    'Raw 22/38 instantiations remain reported, without exclusions or changed\n'
    'instrumentation. Whole-project coverage and wider criteria stay open.\n\n'
]
for path in [root / 'coverage-aggregation/library-instantiations/versions.txt',
             root / 'ci-logs/library-instantiation-control.log',
             root / 'ci-logs/author-fence-instantiation-classification.log']:
    raw = path.read_bytes()
    parts.extend([f'--- {path.name}; SHA256 {hashlib.sha256(raw).hexdigest()} ---\n', raw.decode(), '\n'])
destination = root.parents[1] / 'dev/log/issues/138/pulls/139/library-instantiations.txt'
destination.write_text('\n'.join(line.rstrip() for line in ''.join(parts).splitlines()).rstrip() + '\n')
