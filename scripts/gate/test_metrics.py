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
            return [{'workflow_runs': [{'id': 5}]}]
        original = metrics.run
        def command(*args):
            return json.dumps([pr]) if args[:3] == ('gh', 'pr', 'list') else original(*args)
        with patch.object(metrics, 'run', side_effect=command), patch.object(metrics, 'gh', side_effect=api), patch.object(metrics, 'pages', side_effect=pages):
            result = metrics.collect(cfg, '2026-08-01', '2026-08-04')
        self.assertEqual(result['behind_main_at_merge'], 1)
        self.assertEqual(result['without_passing_review'], [1])
        self.assertEqual(result['failed_main_ci_runs'], 1)
        self.assertEqual(result['stop_rate'], 1)
