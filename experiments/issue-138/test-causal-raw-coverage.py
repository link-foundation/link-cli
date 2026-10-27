#!/usr/bin/env python3
"""Portable raw Coverlet verifier contracts, including generated methods."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

CHECKER = Path(__file__).with_name('check-causal-raw-coverage.py')
NAMES = ['CausalBatch', 'CausalMerkle', 'CausalProjection', 'CausalReplica', 'CausalVersion']


class RawCoverageTests(unittest.TestCase):
    def check_report(self, windows=False, missing=False, session=False, require_session=False):
        files = {}
        for name in NAMES + (['CausalSession'] if session else []):
            filename = f'C:\\repo\\Engine\\{name}.cs' if windows else f'/repo/Engine/{name}.cs'
            files[filename] = {name: {'GeneratedLambda': {'Lines': {}, 'Branches': [
                {'Line': 61, 'Offset': 8, 'EndOffset': 59, 'Path': 1,
                 'Hits': 0 if missing and name == ('CausalSession' if session else 'CausalBatch') else 1}]}}}
        with tempfile.TemporaryDirectory(prefix='causal-raw-', dir=CHECKER.parent) as temporary:
            path = Path(temporary) / 'coverage.json'
            path.write_text(json.dumps({'Library.dll': files}))
            return subprocess.run([sys.executable, str(CHECKER), str(path)] +
                                  (['--require-session'] if require_session else []),
                                  text=True, capture_output=True, check=False)

    def test_posix_paths_include_methods_without_source_lines(self):
        result = self.check_report()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('CausalBatch.cs: 1/1 raw branches', result.stdout)

    def test_windows_paths_include_methods_without_source_lines(self):
        result = self.check_report(windows=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('CausalBatch.cs: 1/1 raw branches', result.stdout)

    def test_missing_generated_branch_rejects_posix(self):
        result = self.check_report(missing=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('source lines=0', result.stdout)
        self.assertIn('1 compiled causal branches remain uncovered', result.stderr)

    def test_missing_generated_branch_rejects_windows(self):
        result = self.check_report(windows=True, missing=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('source lines=0', result.stdout)
        self.assertIn('1 compiled causal branches remain uncovered', result.stderr)

    def test_new_session_is_included_when_present(self):
        result = self.check_report(session=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('CausalSession.cs: 1/1 raw branches', result.stdout)
        missing = self.check_report(session=True, missing=True)
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn('UNCOVERED CausalSession.cs', missing.stdout)

    def test_required_session_cannot_be_omitted_from_the_report(self):
        complete = self.check_report(session=True, require_session=True)
        self.assertEqual(complete.returncode, 0, complete.stdout + complete.stderr)
        missing = self.check_report(require_session=True)
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn('missing required components', missing.stderr)
        self.assertIn('CausalSession.cs', missing.stderr)


if __name__ == '__main__':
    unittest.main()
