import json
import os
from pathlib import Path
import subprocess
import textwrap
import unittest

from approval import is_approved

from test_gate import GateTests

WORKFLOWS = Path(__file__).resolve().parents[2] / '.github/workflows'


def workflow_script(filename, step):
    text = (WORKFLOWS / filename).read_text()
    block = text.split('- name: ' + step + '\n', 1)[1].split('        run: |\n', 1)[1]
    lines = []
    for line in block.splitlines():
        if line.strip() and not line.startswith('          '):
            break
        lines.append(line[10:])
    return '\n'.join(lines) + '\n'


class ApprovalTests(GateTests):
    def setUp(self):
        super().setUp()
        self.old = self.git('rev-parse', 'HEAD')
        self.write('code.txt', 'new push')
        self.commit()
        self.head = self.git('rev-parse', 'HEAD')
        self.database = Path('api.json').resolve()
        self.database.write_text(json.dumps({'statuses': {}, 'calls': []}))
        self.write('approval.sh', workflow_script('gate-approve.yml', 'Record approval and remove trigger'))
        self.write('risk.sh', workflow_script('gate.yml', 'Test and classify'))
        self.write('scripts/gate/approval.py', (WORKFLOWS.parents[1] / 'scripts/gate/approval.py').read_text())
        self.write('scripts/gate/gate', '#!/bin/sh\nprintf \'{"tier":"stop"}\\n\'\n')
        Path('scripts/gate/gate').chmod(0o755)
        self.write('bin/git', '#!/bin/sh\nexit 0\n')
        self.write('bin/base64', '#!/bin/sh\nprintf stub-auth\n')
        self.write('bin/gh', '#!/usr/bin/env python3\n' + textwrap.dedent('''
            import json, os, subprocess, sys
            from pathlib import Path
            args = sys.argv[1:]
            if '--slurp' in args and ('--jq' in args or '--template' in args):
                sys.exit('unsupported slurp filter')
            path = Path(os.environ['API_DATABASE'])
            endpoint = next(a for a in args if a.startswith('repos/'))
            method = args[args.index('--method') + 1] if '--method' in args else 'GET'
            if method == 'GET' and os.environ.get('LABEL_DURING_PUSH'):
                env = dict(os.environ, HEAD_SHA=os.environ['OLD_HEAD'])
                env.pop('LABEL_DURING_PUSH')
                subprocess.run(['bash', 'approval.sh'], env=env, check=True, stdout=subprocess.DEVNULL)
            data = json.loads(path.read_text())
            data['calls'].append({'method': method, 'endpoint': endpoint})
            if method == 'POST':
                fields = dict(args[i + 1].split('=', 1) for i, a in enumerate(args) if a == '-f')
                sha = endpoint.rsplit('/', 1)[-1]
                data['statuses'].setdefault(sha, {})[fields['context']] = fields['state']
                result = fields
            elif method == 'DELETE':
                result = {}
            else:
                sha = endpoint.split('/commits/')[1].split('/')[0]
                result = [[{'context': k, 'state': v, 'creator': {'login': 'github-actions[bot]'}} for k, v in data['statuses'].get(sha, {}).items()]]
            path.write_text(json.dumps(data))
            output = json.dumps(result)
            if '--jq' in args:
                output = subprocess.check_output(['jq', '-r', args[args.index('--jq') + 1]], input=output, text=True)
            print(output)
        '''))
        for path in Path('bin').iterdir():
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(Path('bin').resolve()) + os.pathsep + os.environ['PATH'], API_DATABASE=str(self.database), GH_TOKEN='fixture', GITHUB_REPOSITORY='example/repo', PR_NUMBER='1', HEAD_SHA=self.head, BASE_SHA=self.old, AUTHOR='author', CONTRACTS='success', UNIT='success')
        self.commit()

    def execute(self, script, **env):
        return subprocess.run(['bash', '-e', script], env=dict(self.env, **env), capture_output=True, text=True)

    def status(self, sha, context):
        return json.loads(self.database.read_text())['statuses'].get(sha, {}).get(context)

    def test_old_sha_approval_does_not_approve_a_new_push(self):
        approval = self.execute('approval.sh', HEAD_SHA=self.old)
        self.assertEqual(approval.returncode, 0, approval.stderr)
        self.assertEqual(self.status(self.old, 'gate/approved'), 'success')
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')
        self.assertIsNone(self.status(self.head, 'gate/approved'))
        calls = json.loads(self.database.read_text())['calls']
        self.assertEqual(calls[0]['method'], 'POST')
        self.assertEqual(calls[1]['method'], 'DELETE')

    def test_labeled_event_during_push_only_approves_its_captured_sha(self):
        risk = self.execute('risk.sh', LABEL_DURING_PUSH='1', OLD_HEAD=self.old)
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.old, 'gate/approved'), 'success')
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')
        self.assertIsNone(self.status(self.head, 'gate/approved'))
        reads = [c['endpoint'] for c in json.loads(self.database.read_text())['calls'] if c['method'] == 'GET']
        self.assertEqual(reads, [f'repos/example/repo/commits/{self.head}/statuses?per_page=100'])

    def test_approval_on_current_sha_passes_only_after_checks_pass(self):
        self.assertEqual(self.execute('approval.sh').returncode, 0)
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 0, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'success')
        self.assertEqual(self.execute('risk.sh', CONTRACTS='failure').returncode, 1)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')

    def test_missing_base_helper_refuses_approval_and_posts_risk(self):
        Path('scripts/gate/approval.py').unlink()
        self.assertEqual(self.execute('approval.sh').returncode, 0)
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')

    def test_approval_workflow_is_independent_and_runs_no_pr_code(self):
        approval = (WORKFLOWS / 'gate-approve.yml').read_text()
        self.assertIn('pull_request_target:', approval)
        self.assertIn("github.event.label.name == 'gate:approved'", approval)
        self.assertIn('HEAD_SHA: ${{ github.event.pull_request.head.sha }}', approval)
        self.assertNotIn('checkout', approval)
        self.assertNotIn('concurrency:', approval)
        self.assertNotIn('needs:', approval)


def status(creator, state):
    return {'context': 'gate/approved', 'creator': {'login': creator}, 'state': state}


class ApprovalStatusTests(unittest.TestCase):
    def test_bot_success(self):
        self.assertTrue(is_approved([status('github-actions[bot]', 'success')]))

    def test_user_success_only(self):
        self.assertFalse(is_approved([status('user', 'success')]))

    def test_newer_bot_failure(self):
        self.assertFalse(is_approved([
            status('github-actions[bot]', 'failure'),
            status('github-actions[bot]', 'success'),
        ]))

    def test_newer_user_failure(self):
        self.assertTrue(is_approved([
            status('user', 'failure'),
            status('github-actions[bot]', 'success'),
        ]))

    def test_no_entries(self):
        self.assertFalse(is_approved([]))

    def test_other_context_and_missing_creator(self):
        self.assertFalse(is_approved([
            {'context': 'gate/approved', 'state': 'success'},
            {'context': 'gate/review', 'creator': {'login': 'github-actions[bot]'}, 'state': 'success'},
        ]))
