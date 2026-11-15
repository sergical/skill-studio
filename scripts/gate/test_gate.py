import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import gate


class GateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'user.name', 'Test')
        self.write('code.txt', 'before')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.cfg = {'rules': [{'id': 'contract', 'paths': ['contracts/**'], 'tier': 'stop'}], 'dependency_files': ['package.json'], 'version_files': ['package.json'], 'patch_bots': ['bot']}

    def tearDown(self):
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], text=True).strip()

    def write(self, path, text):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(text)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')

    def risk(self, **kwargs):
        return gate.classify(self.cfg, self.base, **kwargs)

    def test_contract_and_deleted_contract_stop(self):
        self.write('contracts/api.json', '{}')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'stop')
        self.base = self.git('rev-parse', 'HEAD')
        Path('contracts/api.json').unlink()
        self.commit()
        self.assertEqual(self.risk()['tier'], 'stop')

    def test_code_is_ship_and_raise_cannot_lower(self):
        self.write('code.txt', 'after')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'ship')
        self.assertEqual(self.risk(raise_to='show', reason='file writes')['tier'], 'show')
        self.write('contracts/api.json', '{}')
        self.commit()
        with self.assertRaises(ValueError):
            self.risk(raise_to='ship', reason='skip')
        with self.assertRaises(ValueError):
            self.risk(raise_to='stop')

    def test_dependencies_versions_and_bot_patch(self):
        self.write('package.json', json.dumps({'version': '1.0.0', 'dependencies': {'a': '^1.0.0'}}))
        self.commit()
        self.assertEqual(self.risk()['tier'], 'stop')
        self.base = self.git('rev-parse', 'HEAD')
        self.write('package.json', json.dumps({'version': '1.0.0', 'dependencies': {'a': '^1.0.1'}}))
        self.commit()
        self.assertEqual(self.risk(author='bot')['tier'], 'ship')
        self.write('package.json', json.dumps({'version': '1.1.0', 'dependencies': {'a': '^1.0.1', 'b': '2'}}))
        self.commit()
        self.assertEqual(self.risk(author='bot')['tier'], 'stop')

    def test_snapshot_staleness(self):
        self.write('contracts/api.json', '{}')
        self.commit()
        cfg = {'contracts': [{'kind': 'files', 'glob': 'contracts/*.json', 'snapshot': 'snapshot.json'}]}
        gate.snapshots(cfg, True)
        gate.snapshots(cfg)
        self.write('contracts/api.json', '{"new":true}')
        with self.assertRaisesRegex(ValueError, 'Stale contract'):
            gate.snapshots(cfg)

    def test_intent_is_untracked_and_branch_specific(self):
        first = gate.state_path()
        first.write_text('{}')
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.git('checkout', '-qb', 'other')
        self.assertNotEqual(first, gate.state_path())


if __name__ == '__main__':
    unittest.main()
