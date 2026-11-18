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

    def test_cargo_table_patch_and_version_only(self):
        self.cfg['dependency_files'] = ['Cargo.toml']
        self.cfg['version_files'] = ['Cargo.toml', 'package.json']
        self.write('Cargo.toml', '[package]\nversion="1.0.0"\n[dependencies.a]\nversion="1.0.0"\nfeatures=["derive"]\n')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('Cargo.toml', '[package]\nversion="1.0.0"\n[dependencies.a]\nversion="1.0.1"\nfeatures=["derive"]\n')
        self.commit()
        self.assertEqual(self.risk(author='bot')['tier'], 'ship')
        self.write('package.json', '{"name":"example","version":"2.0.0"}')
        self.commit()
        self.assertIn('version', [r['rule'] for r in self.risk()['reasons']])

    def test_bot_dependency_removal_stops(self):
        self.write('package.json', '{"dependencies":{"a":"1.0.0","b":"1.0.0"}}')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('package.json', '{"dependencies":{"a":"1.0.1"}}')
        self.commit()
        self.assertEqual(self.risk(author='bot')['tier'], 'stop')

    def test_policy_changes_stop_even_without_rules(self):
        self.write('.gate/config.toml', 'rules = []')
        self.commit()
        self.assertEqual(gate.classify({'rules': []}, self.base)['tier'], 'stop')

    def test_unusual_file_names_still_match_rules(self):
        self.write('contracts/a\\b.json', '{}')
        self.write('contracts/‮x.json', '{}')
        self.commit()
        files = self.risk()['needs_you'][0]['files']
        self.assertEqual(sorted(files), sorted(['contracts/a\\b.json', 'contracts/‮x.json']))

    def test_same_name_from_another_source_stops(self):
        self.write('package.json', '{"dependencies":{"ai":"^7.0.96"}}')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('package.json', '{"dependencies":{"ai":"^7.1.0"}}')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'ship')
        for spec in ['npm:other-package@1.0.0', 'github:someone/ai', 'file:../ai']:
            self.write('package.json', json.dumps({'dependencies': {'ai': spec}}))
            self.commit()
            self.assertEqual(self.risk()['tier'], 'stop', spec)
        self.cfg['dependency_files'] = ['Cargo.toml']
        self.write('Cargo.toml', '[dependencies]\na = "1"\n')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('Cargo.toml', '[dependencies]\na = { version = "1", git = "https://example.invalid/a" }\n')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'stop')

    def test_cli_uses_the_current_base_rules_not_the_branch_start(self):
        self.write('.gate/config.toml', 'rules = []\n')
        self.commit()
        start = self.git('rev-parse', 'HEAD')
        self.git('checkout', '-qb', 'branch')
        self.write('secret.txt', 'changed')
        self.commit()
        self.git('checkout', '-q', start)
        self.git('checkout', '-qb', 'main2')
        self.write('.gate/config.toml', 'rules = [{ id = "secret", tier = "stop", paths = ["secret.txt"] }]\n')
        self.commit()
        self.git('checkout', '-q', 'branch')
        script = Path(gate.__file__).resolve()
        result = json.loads(subprocess.check_output(['python3', str(script), 'risk', '--base', 'main2', '--head', 'branch', '--json'], text=True))
        self.assertEqual(result['tier'], 'stop')

    def test_needs_you_names_the_decision_and_why(self):
        self.cfg['owner'] = {'contract': {'ask': 'Change the API?', 'why': 'Clients break.'}}
        self.write('contracts/a.json', '{}')
        self.write('contracts/b.json', '{}')
        self.write('package.json', '{"dependencies":{"a":"1.0.0"}}')
        self.commit()
        needs = {n['rule']: n for n in self.risk()['needs_you']}
        self.assertEqual(needs['contract'], {'rule': 'contract', 'ask': 'Change the API?', 'why': 'Clients break.', 'files': ['contracts/a.json', 'contracts/b.json']})
        self.assertIn('no complete [owner.dependencies] text', needs['dependencies']['why'])
        self.cfg['owner']['contract'] = {'ask': 'Change the API?', 'why': ' '}
        self.assertIn('no complete [owner.contract] text', self.risk()['needs_you'][0]['why'])
        self.assertEqual(self.risk(raise_to='stop', reason='Spends money')['needs_you'][-1]['ask'], 'Spends money')

    def test_every_stop_rule_in_the_repo_config_has_owner_text(self):
        import tomllib
        cfg = tomllib.loads((Path(__file__).resolve().parents[2] / '.gate/config.toml').read_text())
        rules = {r['id'] for r in cfg['rules'] if r['tier'] == 'stop'} | {'dependencies', 'check-scripts', 'policy-change'}
        if cfg.get('version_files'):
            rules.add('version')
        for rule in rules:
            self.assertTrue(cfg['owner'][rule]['ask'] and cfg['owner'][rule]['why'], rule)

    def test_intent_is_untracked_and_branch_specific(self):
        first = gate.state_path()
        first.write_text('{}')
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.git('checkout', '-qb', 'other')
        self.assertNotEqual(first, gate.state_path())


if __name__ == '__main__':
    unittest.main()
