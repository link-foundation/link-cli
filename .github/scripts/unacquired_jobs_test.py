"""Offline regression cases, including the annotation from run 42054738038."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('recovery', Path(__file__).with_name('unacquired-jobs.py'))
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


class RecoveryTests(unittest.TestCase):
    def test_only_confirmed_capacity_failure_is_retried(self):
        jobs = [dict(id=1, name='macOS', conclusion='cancelled', steps=[], runner_name=''),
                dict(id=2, name='superseded', conclusion='cancelled', steps=[], runner_name=''),
                dict(id=3, name='test failure', conclusion='failure', steps=[{}], runner_name='hosted')]
        message = 'The job was not acquired by Runner of type hosted even after multiple attempts'
        self.assertEqual(recovery.unacquired(jobs, {1: [message], 3: [message]}), ['macOS'])

    def test_annotations_are_paginated_and_later_pages_are_checked(self):
        job = dict(id=1, name='macOS', conclusion='cancelled', steps=[], runner_name='')
        with patch.object(recovery, 'gh_json', side_effect=[
            [{'jobs': [job]}], [[{'message': 'queued'}], [{'message': recovery.NOT_ACQUIRED}]],
        ]) as api:
            self.assertEqual(recovery.fetch_unacquired('owner/repo', 10, 1), ['macOS'])
        for call in api.call_args_list:
            self.assertIn('--paginate', call.args)
            self.assertIn('--slurp', call.args)

    def test_bounded_retries_never_retry_superseded_or_code_failures(self):
        self.assertEqual(recovery.decide([], 1, True), 'nothing')
        self.assertEqual(recovery.decide(['macOS'], 1, False), 'superseded')
        self.assertEqual(recovery.decide(['macOS'], 2, True), 'rerun')
        self.assertEqual(recovery.decide(['macOS'], 3, True), 'give-up')

    def test_newest_run_matches_branch_and_event(self):
        with patch.object(recovery, 'gh_json', side_effect=[
            {'workflow_id': 5, 'head_branch': 'main', 'event': 'push'},
            {'workflow_runs': [{'id': 11}]},
        ]) as api:
            self.assertFalse(recovery.is_newest_run('owner/repo', 10))
        self.assertIn('branch=main', api.call_args.args)
        self.assertIn('event=push', api.call_args.args)


if __name__ == '__main__':
    unittest.main()
