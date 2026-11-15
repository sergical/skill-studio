import argparse
import json
from unittest.mock import patch

from test_gate import GateTests
import metrics


class MetricsTests(GateTests):
    def test_history_counts_review_before_merge_and_behind_main(self):
        self.write('contracts/api.json', '{}')
        self.commit()
        sha = self.git('rev-parse', 'HEAD')
        self.git('remote', 'add', 'origin', '.')
        self.git('branch', '-M', 'main')
        self.git('fetch', '-q', 'origin', 'main')
        cfg = dict(self.cfg, repository='example/repo', base='origin/main')
        pr = {'number': 1, 'title': 'Add API', 'body': '', 'mergedAt': '2026-08-02T12:00:00Z', 'mergeCommit': {'oid': sha}, 'headRefOid': sha, 'author': {'login': 'author'}}
        def api(endpoint):
            if '/compare/' in endpoint:
                return {'behind_by': 1}
            return {'parents': [{'sha': self.base}]}
        def pages(endpoint):
            if '/files?' in endpoint:
                return [[{'filename': 'contracts/api.json'}]]
            if '/statuses?' in endpoint:
                return [[{'context': 'gate/review', 'state': 'success', 'created_at': '2026-08-03T00:00:00Z'}]]
            return [{'workflow_runs': [{'id': 5, 'event': 'push', 'path': 'ci.yml', 'head_repository': {'full_name': 'example/repo'}}]}]
        original = metrics.run
        def command(*args):
            return json.dumps([pr]) if args[:3] == ('gh', 'pr', 'list') else original(*args)
        with patch.object(metrics, 'run', side_effect=command), patch.object(metrics, 'gh', side_effect=api), patch.object(metrics, 'pages', side_effect=pages):
            result = metrics.collect(cfg, '2026-08-01', '2026-08-04')
        self.assertEqual(result['behind_main_at_merge'], 1)
        self.assertEqual(result['without_passing_review'], [1])
        self.assertEqual(result['failed_main_ci_runs'], 1)
        self.assertEqual(result['stop_rate'], 1)

    def test_followups_use_full_window_and_filter_main_ci(self):
        import os
        def commit_at(date, message):
            self.git('add', '.')
            with patch.dict(os.environ, {'GIT_AUTHOR_DATE': date, 'GIT_COMMITTER_DATE': date}):
                self.git('commit', '-qm', message)
            return self.git('rev-parse', 'HEAD')
        self.write('contracts/api.json', '{}')
        original_sha = commit_at('2026-08-02T12:00:00Z', 'Add API')
        self.write('contracts/api.json', '{"fixed":true}')
        fix_sha = commit_at('2026-08-03T12:00:00Z', 'fix: API')
        self.write('code.txt', 'reference-only fix')
        direct_sha = commit_at('2026-08-04T12:00:00Z', 'fix: regression #1')
        self.write('contracts/api.json', '{"late":true}')
        late_sha = commit_at('2026-08-17T12:00:00Z', 'fix: too late')
        self.git('branch', '-M', 'main')
        self.git('remote', 'add', 'origin', '.')
        self.git('fetch', '-q', 'origin', 'main')
        prs = [{'number': n, 'title': title, 'body': '', 'mergedAt': date, 'mergeCommit': {'oid': sha}, 'headRefOid': sha, 'author': {'login': 'app/dependabot'}} for n, title, date, sha in [(1, 'Add API', '2026-08-02T12:00:00Z', original_sha), (2, 'fix: API', '2026-08-03T12:00:00Z', fix_sha), (3, 'fix: too late', '2026-08-17T12:00:00Z', late_sha)]]
        def api(endpoint):
            if '/compare/' in endpoint:
                return {'behind_by': 0}
            sha = endpoint.rsplit('/', 1)[-1]
            return {'parents': [{'sha': self.git('rev-parse', sha + '^')}]}
        def pages(endpoint):
            if '/files?' in endpoint:
                return [[{'filename': 'contracts/api.json'}]]
            if '/statuses?' in endpoint:
                return [[]]
            def run(event, path, repo='example/repo'):
                return {'event': event, 'path': path, 'head_repository': {'full_name': repo}}
            return [{'workflow_runs': [run('push', 'ci.yml'), run('push', 'gate-update.yml'), run('pull_request', 'ci.yml'), run('push', 'ci.yml', 'fork/repo')]}]
        original = metrics.run
        def command(*args):
            return json.dumps(prs) if args[:3] == ('gh', 'pr', 'list') else original(*args)
        cfg = dict(self.cfg, repository='example/repo', base='origin/main', ci_workflows=['ci.yml'])
        with patch.object(metrics, 'run', side_effect=command), patch.object(metrics, 'gh', side_effect=api), patch.object(metrics, 'pages', side_effect=pages):
            result = metrics.collect(cfg, '2026-08-01', '2026-08-02')
        self.assertEqual(result['merged_prs'], 1)
        self.assertEqual(set(result['followup_fixes']['prs']['1']), {'PR #2', direct_sha})
        self.assertEqual(result['failed_main_ci_runs'], 1)
        self.assertEqual(result['stop_per_rule']['contract']['count'], 1)
