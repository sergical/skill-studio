import argparse
import json
from pathlib import Path
from unittest.mock import patch

from test_gate import GateTests
import proof


class ProofTests(GateTests):
    def test_card_preserves_human_text(self):
        body = 'Human intro\n\n<!-- gate:card -->old<!-- /gate:card -->\nHuman end'
        self.assertEqual(proof.replace_card(body, '<!-- gate:card -->new<!-- /gate:card -->'), body.replace('old', 'new'))
        with self.assertRaises(ValueError):
            proof.replace_card('<!-- gate:card -->', 'new')

    def test_review_fails_unresolved_high_and_refuses_same_model(self):
        path = Path('findings.json')
        sha = self.git('rev-parse', 'HEAD')
        path.write_text(json.dumps({'head': sha, 'findings': [{'severity': 'high', 'resolved': False}]}))
        args = argparse.Namespace(findings=str(path), verdict='pass', reviewer='review-model')
        proof.state_path().write_text(json.dumps({'author_model': 'author-model'}))
        with patch.object(proof, 'gh') as api:
            proof.review(args, {'repository': 'example/repo'})
            self.assertEqual(api.call_args.args[2]['state'], 'failure')
            self.assertEqual(api.call_args.args[2]['description'], 'fail: review-model, 1 open')
        args.reviewer = 'AUTHOR-MODEL'
        with self.assertRaisesRegex(ValueError, 'differ'):
            proof.review(args, {'repository': 'example/repo'})
        args.reviewer = 'other'
        path.write_text(json.dumps({'head': 'stale', 'findings': []}))
        with self.assertRaisesRegex(ValueError, 'head SHA'):
            proof.review(args, {'repository': 'example/repo'})

    def test_card_absent_unbound_and_stale_verify(self):
        sha = self.git('rev-parse', 'HEAD')
        pr = {'number': 1, 'body': 'Human text', 'headRefOid': sha, 'baseRefOid': sha, 'author': {'login': 'author'}, 'title': 'Result', 'statusCheckRollup': []}
        cfg = dict(self.cfg, repository='example/repo', check='npm test', ci_jobs=['test'])
        manifest = Path('.gate/out/verify.json')
        manifest.parent.mkdir(parents=True)
        scenario = {'name': 'Example', 'intent': 'See result', 'status': 'pass', 'evidence': [{'kind': 'screenshot', 'path': 'https://example.invalid/evidence.png'}], 'notes': ''}
        for data, expected in [(None, 'not run'), ({'scenarios': [scenario]}, 'unbound evidence'), ({'head': 'old', 'scenarios': [scenario]}, 'not run (stale head)'), ({'head': sha, 'scenarios': [scenario]}, '| verify | pass |')]:
            if data:
                manifest.write_text(json.dumps(data))
            with patch.object(proof, 'run', return_value=json.dumps(pr)), patch.object(proof, 'gh') as api:
                api.return_value = {'statuses': []}
                proof.card(argparse.Namespace(result=None), cfg)
                body = api.call_args.args[2]['body']
                self.assertTrue(body.startswith('Human text'))
                self.assertIn(expected, body)
                self.assertIn('| npm test |', body)
                self.assertIn('| gate/approved |', body)
                if data and data.get('head') == sha:
                    self.assertIn('![screenshot](https://example.invalid/evidence.png)', body)

    def bypass_api(self, statuses, runs, merged=True):
        posted = []
        pr = {'number': 7, 'merge_commit_sha': 'm1', 'merged_by': {'login': 'owner'}, 'merged_at': '2026-01-01T10:00:00Z', 'base': {'ref': 'main'}, 'head': {'sha': 'h1'}}
        responses = {
            'repos/example/repo/commits/m1/pulls': [pr] if merged else [],
            'repos/example/repo/pulls/7': pr,
            'repos/example/repo/rules/branches/main': [{'type': 'pull_request', 'parameters': {}}, {'type': 'required_status_checks', 'parameters': {'required_status_checks': [{'context': 'check'}, {'context': 'gate/risk'}, {'context': 'gate/review'}]}}],
        }
        listed = {
            'repos/example/repo/commits/h1/statuses?per_page=100': [statuses],
            'repos/example/repo/commits/h1/check-runs?per_page=100': [{'check_runs': runs}],
        }

        def api(endpoint, method='GET', data=None):
            if method == 'POST':
                posted.append((endpoint, data))
                return {}
            return responses[endpoint]
        with patch.object(proof, 'gh', side_effect=api), patch.object(proof, 'pages', side_effect=lambda endpoint: listed[endpoint]):
            proof.bypass(argparse.Namespace(sha='m1'), {'repository': 'example/repo'})
        return posted

    @staticmethod
    def status(context, state, at='2026-01-01T09:00:00Z'):
        return {'context': context, 'state': state, 'created_at': at}

    @staticmethod
    def check_run(name, conclusion, started='2026-01-01T09:00:00Z', completed='2026-01-01T09:30:00Z'):
        return {'name': name, 'conclusion': conclusion, 'status': 'completed' if completed else 'in_progress', 'started_at': started, 'completed_at': completed}

    def test_bypass_records_red_required_checks(self):
        runs = [self.check_run('check', 'failure', '2026-01-01T08:00:00Z', '2026-01-01T08:10:00Z'), self.check_run('check', 'success')]
        posted = self.bypass_api([self.status('gate/risk', 'failure'), self.status('gate/review', 'success')], runs)
        self.assertEqual(len(posted), 1)
        endpoint, data = posted[0]
        self.assertEqual(endpoint, 'repos/example/repo/issues/7/comments')
        self.assertIn('| gate/risk | failure |', data['body'])
        self.assertIn('**owner**', data['body'])
        self.assertIn('gate/approved on this head: none', data['body'])
        self.assertNotIn('| check |', data['body'])
        self.assertNotIn('gate/review |', data['body'])

    def test_bypass_judges_checks_at_merge_time(self):
        statuses = [self.status('gate/risk', 'failure'), self.status('gate/risk', 'success', '2026-01-01T10:05:00Z'), self.status('gate/review', 'success')]
        runs = [self.check_run('check', 'success', completed='2026-01-01T10:02:00Z'), self.check_run('check', 'success', '2026-01-01T10:03:00Z', '2026-01-01T10:04:00Z')]
        body = self.bypass_api(statuses, runs)[0][1]['body']
        self.assertIn('| gate/risk | failure |', body)
        self.assertIn('| check | not finished at merge |', body)

    def test_bypass_keeps_a_failing_status_that_shares_a_check_run_name(self):
        statuses = [self.status('check', 'failure'), self.status('gate/risk', 'success'), self.status('gate/review', 'success')]
        body = self.bypass_api(statuses, [self.check_run('check', 'success')])[0][1]['body']
        self.assertIn('| check | failure |', body)

    def test_bypass_reports_missing_check_and_skips_green_or_direct_pushes(self):
        posted = self.bypass_api([self.status('gate/risk', 'success')], [self.check_run('check', 'success')])
        self.assertIn('| gate/review | missing |', posted[0][1]['body'])
        green = [self.status('gate/risk', 'success'), self.status('gate/review', 'success')]
        self.assertEqual(self.bypass_api(green, [self.check_run('check', 'success')]), [])
        self.assertEqual(self.bypass_api([], [], merged=False), [])
