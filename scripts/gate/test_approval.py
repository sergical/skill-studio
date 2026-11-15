import json
import os
from pathlib import Path
import subprocess
import textwrap
import unittest
from unittest.mock import Mock

from approval import is_approved
from test_gate import GateTests

WORKFLOWS = Path(__file__).resolve().parents[2] / '.github/workflows'
TRUSTED_RUN = {'path': '.github/workflows/gate-approve.yml', 'event': 'pull_request_target'}


def workflow_script(filename, step):
    text = (WORKFLOWS / filename).read_text()
    block = text.split('- name: ' + step + '\n', 1)[1].split('        run: |\n', 1)[1]
    lines = []
    for line in block.splitlines():
        if line.strip() and not line.startswith('          '):
            break
        lines.append(line[10:])
    return '\n'.join(lines) + '\n'


def status(creator='github-actions[bot]', state='success', run_id='10'):
    return {'context': 'gate/approved', 'creator': {'login': creator}, 'state': state,
            'target_url': f'https://github.com/example/repo/actions/runs/{run_id}'}


class ApprovalTests(GateTests):
    def setUp(self):
        super().setUp()
        self.old = self.git('rev-parse', 'HEAD')
        self.write('code.txt', 'new push')
        self.commit()
        self.head = self.git('rev-parse', 'HEAD')
        self.database = Path('api.json').resolve()
        self.database.write_text(json.dumps({'statuses': {}, 'calls': [], 'runs': {
            '10': TRUSTED_RUN,
            '20': dict(TRUSTED_RUN, event='push'),
            '30': dict(TRUSTED_RUN, path='.github/workflows/other.yml'),
        }}))
        self.write('approval.sh', workflow_script('gate-approve.yml', 'Record approval and remove trigger'))
        self.write('risk.sh', workflow_script('gate-risk.yml', 'Test and classify'))
        for name in ['approval.py', 'gate.py']:
            self.write(f'scripts/gate/{name}', (WORKFLOWS.parents[1] / f'scripts/gate/{name}').read_text())
        self.write('scripts/gate/gate', '#!/bin/sh\nprintf \'{"tier":"stop"}\\n\'\n')
        Path('scripts/gate/gate').chmod(0o755)
        self.write('bin/git', '#!/bin/sh\nexit 0\n')
        self.write('bin/base64', '#!/bin/sh\nprintf stub-auth\n')
        self.write('bin/gh', '#!/usr/bin/env python3\n' + textwrap.dedent('''
            import json, os, subprocess, sys
            from pathlib import Path
            from urllib.parse import parse_qs, urlsplit
            args = sys.argv[1:]
            if '--slurp' in args and ('--jq' in args or '--template' in args):
                sys.exit('unsupported slurp filter')
            if '--slurp' in args and '--paginate' not in args:
                sys.exit('slurp requires pagination')
            path = Path(os.environ['API_DATABASE'])
            endpoint = next(a for a in args if a.startswith('repos/'))
            method = args[args.index('--method') + 1] if '--method' in args else 'GET'
            if method == 'GET' and os.environ.get('LABEL_DURING_PUSH'):
                env = dict(os.environ, HEAD_SHA=os.environ['OLD_HEAD'])
                env.pop('LABEL_DURING_PUSH')
                subprocess.run(['bash', 'approval.sh'], env=env, check=True, stdout=subprocess.DEVNULL)
            data = json.loads(path.read_text())
            data['calls'].append({'method': method, 'endpoint': endpoint, 'args': args})
            if method == 'POST':
                fields = dict(args[i + 1].split('=', 1) for i, a in enumerate(args) if a == '-f')
                sha = endpoint.rsplit('/', 1)[-1]
                entry = dict(fields, creator={'login': os.environ.get('STATUS_CREATOR', 'github-actions[bot]')})
                data['statuses'].setdefault(sha, []).insert(0, entry)
                result = fields
            elif method == 'DELETE':
                result = {}
            elif '/actions/runs/' in endpoint:
                result = data['runs'][endpoint.rsplit('/', 1)[-1]]
            else:
                sha = endpoint.split('/commits/')[1].split('/')[0]
                entries = data['statuses'].get(sha, [])
                size = int(parse_qs(urlsplit(endpoint).query).get('per_page', ['30'])[0])
                pages = [entries[i:i + size] for i in range(0, len(entries), size)] or [[]]
                if '--paginate' not in args:
                    pages = pages[:1]
                result = pages if '--slurp' in args else pages[0]
            path.write_text(json.dumps(data))
            print(json.dumps(result))
        '''))
        for path in Path('bin').iterdir():
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(Path('bin').resolve()) + os.pathsep + os.environ['PATH'],
                        API_DATABASE=str(self.database), GH_TOKEN='fixture', GITHUB_REPOSITORY='example/repo',
                        GITHUB_SERVER_URL='https://github.com', GITHUB_RUN_ID='10', PR_NUMBER='1',
                        HEAD_SHA=self.head, BASE_SHA=self.old, AUTHOR='author')
        self.commit()

    def execute(self, script, **env):
        return subprocess.run(['bash', '-e', '-o', 'pipefail', script], env=dict(self.env, **env), capture_output=True, text=True)

    def status(self, sha, context):
        return next((s['state'] for s in json.loads(self.database.read_text())['statuses'].get(sha, [])
                     if s['context'] == context), None)

    def seed_statuses(self, entries):
        data = json.loads(self.database.read_text())
        data['statuses'][self.head] = entries
        self.database.write_text(json.dumps(data))

    def test_old_sha_approval_does_not_approve_a_new_push(self):
        approval = self.execute('approval.sh', HEAD_SHA=self.old)
        self.assertEqual(approval.returncode, 0, approval.stderr)
        self.assertEqual(self.status(self.old, 'gate/approved'), 'success')
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')
        self.assertIsNone(self.status(self.head, 'gate/approved'))
        calls = json.loads(self.database.read_text())['calls']
        self.assertEqual([c['method'] for c in calls[:2]], ['POST', 'DELETE'])

    def test_labeled_event_during_push_only_approves_its_captured_sha(self):
        risk = self.execute('risk.sh', LABEL_DURING_PUSH='1', OLD_HEAD=self.old)
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.old, 'gate/approved'), 'success')
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')
        self.assertIsNone(self.status(self.head, 'gate/approved'))
        reads = [c for c in json.loads(self.database.read_text())['calls'] if c['method'] == 'GET']
        self.assertEqual([c['endpoint'] for c in reads], [f'repos/example/repo/commits/{self.head}/statuses?per_page=100'])
        self.assertIn('--paginate', reads[0]['args'])

    def test_approval_on_current_sha_passes(self):
        self.assertEqual(self.execute('approval.sh').returncode, 0)
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 0, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'success')
        entry = next(s for s in json.loads(self.database.read_text())['statuses'][self.head] if s['context'] == 'gate/approved')
        self.assertEqual(entry['target_url'], 'https://github.com/example/repo/actions/runs/10')

    def test_user_success_is_ignored(self):
        self.seed_statuses([status(creator='user')])
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')

    def test_newest_trusted_status_decides(self):
        self.seed_statuses([status(state='failure'), status()])
        self.assertEqual(self.execute('risk.sh').returncode, 1)
        self.seed_statuses([status(creator='user', state='failure'), status(run_id='20'), status()])
        self.assertEqual(self.execute('risk.sh').returncode, 0)

    def test_approval_on_later_page(self):
        self.seed_statuses([status(creator='user')] * 100 + [status()])
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 0, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'success')

    def test_missing_base_helper_refuses_approval_and_posts_risk(self):
        Path('scripts/gate/approval.py').unlink()
        self.assertEqual(self.execute('approval.sh').returncode, 0)
        risk = self.execute('risk.sh')
        self.assertEqual(risk.returncode, 1, risk.stderr)
        self.assertEqual(self.status(self.head, 'gate/risk'), 'failure')

    def test_workflows_use_trusted_yaml_and_separate_checks(self):
        risk = (WORKFLOWS / 'gate-risk.yml').read_text()
        self.assertIn('pull_request_target:', risk)
        self.assertIn('ref: ${{ github.event.pull_request.base.sha }}', risk)
        self.assertIn('persist-credentials: false', risk)
        self.assertIn('actions: read', risk)
        for text in ['needs:', 'UNIT:', 'CONTRACTS:', 'pull_request.head.ref']:
            self.assertNotIn(text, risk)
        checks = (WORKFLOWS / 'gate.yml').read_text()
        self.assertIn('  unit:', checks)
        self.assertIn('  contracts:', checks)
        self.assertNotIn('  risk:', checks)
        self.assertNotIn('statuses: write', checks)

    def test_approval_workflow_is_independent_and_runs_no_pr_code(self):
        approval = (WORKFLOWS / 'gate-approve.yml').read_text()
        self.assertIn('pull_request_target:', approval)
        self.assertIn("github.event.label.name == 'gate:approved'", approval)
        self.assertIn('HEAD_SHA: ${{ github.event.pull_request.head.sha }}', approval)
        self.assertNotIn('checkout', approval)
        self.assertNotIn('concurrency:', approval)
        self.assertNotIn('needs:', approval)


class ApprovalStatusTests(unittest.TestCase):
    def setUp(self):
        self.lookup = Mock(return_value=TRUSTED_RUN)

    def approved(self, entries):
        return is_approved(entries, 'example/repo', self.lookup)

    def test_valid_run(self):
        self.assertTrue(self.approved([status()]))
        self.lookup.assert_called_once_with('10')

    def test_path_with_ref(self):
        self.lookup.return_value = dict(TRUSTED_RUN, path='.github/workflows/gate-approve.yml@refs/heads/main')
        self.assertTrue(self.approved([status()]))

    def test_user_success_only(self):
        self.assertFalse(self.approved([status(creator='user')]))
        self.lookup.assert_not_called()

    def test_newer_bot_failure(self):
        self.assertFalse(self.approved([status(state='failure'), status()]))

    def test_newer_user_failure(self):
        self.assertTrue(self.approved([status(creator='user', state='failure'), status()]))

    def test_push_run(self):
        self.lookup.return_value = dict(TRUSTED_RUN, event='push')
        self.assertFalse(self.approved([status()]))

    def test_wrong_path(self):
        self.lookup.return_value = dict(TRUSTED_RUN, path='.github/workflows/other.yml')
        self.assertFalse(self.approved([status()]))

    def test_missing_or_foreign_url(self):
        missing = status()
        missing.pop('target_url')
        self.assertFalse(self.approved([missing]))
        for url in [None, '', 'https://github.com/other/repo/actions/runs/10',
                    'https://example.invalid/example/repo/actions/runs/10']:
            with self.subTest(url=url):
                self.assertFalse(self.approved([dict(status(), target_url=url)]))
        self.lookup.assert_not_called()

    def test_no_entries(self):
        self.assertFalse(self.approved([]))

    def test_other_context_and_missing_creator(self):
        self.assertFalse(self.approved([
            {'context': 'gate/approved', 'state': 'success'},
            dict(status(), context='gate/review'),
        ]))
        self.lookup.assert_not_called()

    def test_lookup_failure_does_not_grant_approval(self):
        self.lookup.side_effect = RuntimeError('lookup failed')
        with self.assertRaises(RuntimeError):
            self.approved([status()])
