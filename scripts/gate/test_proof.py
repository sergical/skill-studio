import argparse
import json
from pathlib import Path
from unittest.mock import patch

from test_gate import GateTests
import proof


class ProofTests(GateTests):
    def test_card_preserves_human_text(self):
        new = '<!-- gate:card -->new<!-- /gate:card -->'
        body = 'Human intro\n\n<!-- gate:card -->old<!-- /gate:card -->\nHuman end'
        self.assertEqual(proof.replace_card(body, new), f'Human intro\n\nHuman end\n\n{new}\n')
        self.assertEqual(proof.replace_card('<!-- gate:card -->old<!-- /gate:card -->\n\nHuman text', new), f'Human text\n\n{new}\n')
        self.assertEqual(proof.replace_card('Human text\n\n' + new + '\n', new), f'Human text\n\n{new}\n')
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
        pr = {'number': 1, 'body': 'Human text', 'headRefOid': sha, 'baseRefOid': sha, 'author': {'login': 'author'}}
        proof.state_path().write_text(json.dumps({'author_model': 'writer-model'}))
        cfg = dict(self.cfg, repository='example/repo', check='npm test', ci_jobs=['test'])
        manifest = Path('.gate/out/verify.json')
        manifest.parent.mkdir(parents=True)
        scenario = {'name': 'Example', 'intent': 'See result', 'status': 'pass', 'evidence': [{'kind': 'screenshot', 'path': 'https://example.invalid/evidence.png'}], 'notes': ''}
        for data, expected in [(None, 'verify not run'), ({'scenarios': [scenario]}, 'verify unbound evidence'), ({'head': 'old', 'scenarios': [scenario]}, 'verify not run (stale head)'), ({'head': sha, 'scenarios': [scenario]}, 'verify pass')]:
            if data:
                manifest.write_text(json.dumps(data))
            with patch.object(proof, 'run', return_value=json.dumps(pr)), patch.object(proof, 'gh') as api:
                api.return_value = {'statuses': []}
                proof.card(argparse.Namespace(), cfg)
                body = api.call_args.args[2]['body']
                self.assertTrue(body.startswith('Human text'))
                record = body[body.index(proof.START):]
                summary = record.splitlines()[1]
                self.assertTrue(summary.startswith('<details><summary>Gate record: tier'))
                self.assertIn(expected, summary)
                self.assertIn(f'| Head | `{sha}` |', record)
                self.assertIn('| Writer | writer-model |', record)
                self.assertNotIn('| Check |', record)
                self.assertNotIn('npm test', record)
                self.assertTrue(record.rstrip().endswith('</details>\n' + proof.END))
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

    def test_guard_passes_file_tools(self):
        from io import StringIO
        for tool in sorted(proof.FILE_TOOLS):
            with self.subTest(tool=tool), patch('sys.stdin', StringIO(json.dumps({
                'tool_name': tool, 'tool_input': {'content': 'gate:approved gate/approved'},
            }))):
                proof.guard()

    def test_guard_blocks_github_tools(self):
        from io import StringIO
        with patch('sys.stdin', StringIO(json.dumps({
            'tool_name': 'mcp__github__issue_write',
            'tool_input': {'labels': ['gate:approved']},
        }))):
            with self.assertRaises(SystemExit) as error:
                proof.guard()
            self.assertEqual(error.exception.code, 2)

    def test_guard_checks_other_command_tools(self):
        from io import StringIO
        for tool in ['Bash', 'Monitor', 'exec_command', 'functions.exec', 'mcp__other__run', None]:
            with self.subTest(tool=tool), patch('sys.stdin', StringIO(json.dumps({
                'tool_name': tool, 'tool_input': {'command': 'gh pr edit 1 --add-label gate:approved'},
            }))):
                with self.assertRaises(SystemExit) as error:
                    proof.guard()
                self.assertEqual(error.exception.code, 2)
