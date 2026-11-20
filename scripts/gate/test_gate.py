import json
from pathlib import Path
import unittest

import gate
from git_fixture import GitRepoFixture


class GateTests(GitRepoFixture, unittest.TestCase):
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

    def test_intent_is_untracked_and_branch_specific(self):
        first = gate.state_path()
        first.write_text('{}')
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.git('checkout', '-qb', 'other')
        self.assertNotEqual(first, gate.state_path())


if __name__ == '__main__':
    unittest.main()
