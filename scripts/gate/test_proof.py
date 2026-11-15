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
