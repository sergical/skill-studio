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
        args.reviewer = 'AUTHOR-MODEL'
        with self.assertRaisesRegex(ValueError, 'differ'):
            proof.review(args, {'repository': 'example/repo'})
        args.reviewer = 'other'
        path.write_text(json.dumps({'head': 'stale', 'findings': []}))
        with self.assertRaisesRegex(ValueError, 'head SHA'):
            proof.review(args, {'repository': 'example/repo'})

    def test_guard_blocks_label_mutations(self):
        from io import StringIO
        with patch('sys.stdin', StringIO(json.dumps({'tool_input': {'command': 'gh pr edit 1 --add-label gate:approved'}}))):
            with self.assertRaises(SystemExit) as error:
                proof.guard()
            self.assertEqual(error.exception.code, 2)

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

    def test_guard_blocks_shell_quoting_and_malformed_events(self):
        from io import StringIO
        for command in ['gh pr edit 1 --add-label "gate:"approved', r'gh pr edit 1 --add-label gate\:approved']:
            with patch('sys.stdin', StringIO(json.dumps({'tool_input': {'command': command}}))):
                with self.assertRaises(SystemExit) as error:
                    proof.guard()
                self.assertEqual(error.exception.code, 2)
        with patch('sys.stdin', StringIO('bad JSON')):
            with self.assertRaises(SystemExit) as error:
                proof.guard()
            self.assertEqual(error.exception.code, 2)

    def test_guard_blocks_commit_approval_status(self):
        from io import StringIO
        for command in ['gh api repos/example/repo/statuses/HEAD -f context=gate/approved -f state=success', 'gh api statuses -f context=gate%2Fapproved', 'gh api statuses -f context="gate/"approved']:
            with patch('sys.stdin', StringIO(json.dumps({'tool_input': {'command': command}}))):
                with self.assertRaises(SystemExit) as error:
                    proof.guard()
                self.assertEqual(error.exception.code, 2)

    def test_guard_blocks_merges_past_required_checks(self):
        from io import StringIO
        for command in ['gh pr merge 5 --squash --admin', 'gh api -X PUT repos/example/repo/pulls/5/merge', 'gh api graphql -f query="mutation { mergePullRequest(input: {}) { clientMutationId } }"']:
            with patch('sys.stdin', StringIO(json.dumps({'tool_input': {'command': command}}))):
                with self.assertRaises(SystemExit) as error:
                    proof.guard()
                self.assertEqual(error.exception.code, 2)
        for command in ['gh pr merge 5 --squash', 'gh pr view 5 --json mergeStateStatus', 'gh pr merge 5 --squash && echo --admin-free']:
            with patch('sys.stdin', StringIO(json.dumps({'tool_input': {'command': command}}))):
                self.assertIsNone(proof.guard())

    def bypass_api(self, statuses, runs, merged=True):
        posted = []
        pr = {'number': 7, 'merge_commit_sha': 'm1', 'merged_by': {'login': 'owner'}, 'merged_at': 'T', 'base': {'ref': 'main'}, 'head': {'sha': 'h1'}}
        responses = {
            'repos/example/repo/commits/m1/pulls': [pr] if merged else [],
            'repos/example/repo/pulls/7': pr,
            'repos/example/repo/rules/branches/main': [{'type': 'pull_request', 'parameters': {}}, {'type': 'required_status_checks', 'parameters': {'required_status_checks': [{'context': 'check'}, {'context': 'gate/risk'}, {'context': 'gate/review'}]}}],
            'repos/example/repo/commits/h1/status': {'statuses': statuses},
            'repos/example/repo/commits/h1/check-runs?per_page=100': {'check_runs': runs},
        }
        def api(endpoint, method='GET', data=None):
            if method == 'POST':
                posted.append((endpoint, data))
                return {}
            return responses[endpoint]
        with patch.object(proof, 'gh', side_effect=api):
            proof.bypass(argparse.Namespace(sha='m1'), {'repository': 'example/repo'})
        return posted

    def test_bypass_records_red_required_checks(self):
        runs = [{'id': 1, 'name': 'check', 'conclusion': 'failure', 'status': 'completed'}, {'id': 2, 'name': 'check', 'conclusion': 'success', 'status': 'completed'}]
        posted = self.bypass_api([{'context': 'gate/risk', 'state': 'failure'}, {'context': 'gate/review', 'state': 'success'}], runs)
        self.assertEqual(len(posted), 1)
        endpoint, data = posted[0]
        self.assertEqual(endpoint, 'repos/example/repo/issues/7/comments')
        self.assertIn('| gate/risk | failure |', data['body'])
        self.assertIn('**owner**', data['body'])
        self.assertIn('gate/approved on this head: none', data['body'])
        self.assertNotIn('| check |', data['body'])
        self.assertNotIn('gate/review |', data['body'])

    def test_bypass_reports_missing_check_and_skips_green_or_direct_pushes(self):
        posted = self.bypass_api([{'context': 'gate/risk', 'state': 'success'}], [{'id': 1, 'name': 'check', 'conclusion': 'success', 'status': 'completed'}])
        self.assertIn('| gate/review | missing |', posted[0][1]['body'])
        green = [{'context': 'gate/risk', 'state': 'success'}, {'context': 'gate/review', 'state': 'success'}]
        self.assertEqual(self.bypass_api(green, [{'id': 1, 'name': 'check', 'conclusion': 'success', 'status': 'completed'}]), [])
        self.assertEqual(self.bypass_api([], [], merged=False), [])
