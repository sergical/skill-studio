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

    def test_overrides_that_swap_a_package_stop(self):
        self.write('package.json', '{"pnpm":{"overrides":{"a":"1.0.0"}},"overrides":{"b":{"c":"1.0.0"}}}')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('package.json', '{"pnpm":{"overrides":{"a":"1.0.1"}},"overrides":{"b":{"c":"1.0.1"}}}')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'ship')
        for doc in [{'pnpm': {'overrides': {'a': 'npm:evil@1.0.0'}}, 'overrides': {'b': {'c': '1.0.1'}}},
                    {'pnpm': {'overrides': {'a': '1.0.1'}}, 'overrides': {'b': {'c': 'github:someone/c'}}},
                    {'pnpm': {'overrides': {'a': '1.0.1', 'd': '2.0.0'}}, 'overrides': {'b': {'c': '1.0.1'}}},
                    {'pnpm': {'overrides': {'a': '1.0.1'}}, 'overrides': {'b': {'c': '1.0.1'}}, 'resolutions': {'e': '1.0.0'}}]:
            self.write('package.json', json.dumps(doc))
            self.commit()
            self.assertEqual(self.risk()['tier'], 'stop', doc)

    def test_override_edge_cases(self):
        def tier(before, after, author=''):
            self.write('package.json', json.dumps(before))
            self.commit()
            self.base = self.git('rev-parse', 'HEAD')
            self.write('package.json', json.dumps(after))
            self.commit()
            return self.risk(author=author)['tier']
        deps = {'a': 'npm:x@1.0.0', 'b': 'github:someone/b'}
        self.assertEqual(tier({'dependencies': deps, 'overrides': {'f': '$a'}}, {'dependencies': deps, 'overrides': {'f': '$b'}}), 'stop')
        self.assertEqual(tier({'overrides': {'f': '1.0.0'}}, {'overrides': {'f': {'.': '1.0.1'}}}), 'ship')
        self.assertEqual(tier({'pnpm': None}, {'pnpm': None, 'name': 'x'}), 'ship')
        self.assertEqual(tier({'pnpm': 'meta'}, {'pnpm': ['meta']}), 'ship')
        self.assertEqual(tier({'overrides': {'f': 'npm:other@1.0.0'}}, {}), 'stop')
        self.assertEqual(tier({'dependencies': {'a': 'npm:other@1.0.0'}}, {'dependencies': {'a': 'npm:other@1.0.1'}}), 'ship')
        self.assertEqual(tier({'dependencies': {'a': 'npm:@s/other@1.0.0'}}, {'dependencies': {'a': 'npm:@s/evil@1.0.0'}}), 'stop')
        self.assertEqual(tier({'dependencies': {'a': 'npm:x@1.0.0', 'b': 'github:someone/b', 'c': '1'}}, {'dependencies': {'c': '1'}}), 'ship')
        self.cfg['dependency_files'] = ['Cargo.toml']
        self.write('Cargo.toml', '[dependencies]\nlocal = { path = "../local" }\nremote = { git = "https://example.invalid/r" }\n')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('Cargo.toml', '[dependencies]\n')
        self.commit()
        self.assertEqual(self.risk()['tier'], 'ship')

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

    def test_comment_says_the_decision_why_and_the_rule_to_change(self):
        self.cfg['owner'] = {'contract': {'ask': 'Change the API?', 'why': 'Clients break.'}}
        for i in range(12):
            self.write(f'contracts/{i:02}.json', '{}')
        self.write('contracts/(x)[y].json', '{}')
        self.commit()
        risk = self.risk(raise_to='stop', reason='Spends money')
        jq = Path(gate.__file__).resolve().with_name('comment.jq')
        body = subprocess.run(['jq', '-r', '--arg', 'marker', '<!-- m -->', '-f', str(jq)], input=json.dumps(risk),
                              text=True, capture_output=True, check=True).stdout
        self.assertTrue(body.startswith('<!-- m -->\n## Needed from you and why\n\n**Decision:** Change the API?\n'
                                        '**Why you:** Clients break.\n**Change:** contracts\\/\\(x\\)\\[y\\]\\.json, contracts\\/00\\.json, '), body)
        self.assertIn(' and 3 more, matched by rule `contract` in `.gate/config.toml`.\n\n**Decision:** Spends money\n'
                      '**Why you:** An agent raised this PR to stop by hand.\n\nTo say yes, merge this PR.', body)
        self.assertNotIn('(x)[y]', body)

    def migrations(self):
        self.cfg['rules'].append({'id': 'database', 'paths': ['drizzle/**', 'drizzle.config.ts', 'schema.ts'], 'tier': 'stop'})
        self.cfg['additive_migrations'] = {'rule': 'database', 'dir': 'drizzle', 'journal': 'drizzle/meta/_journal.json', 'schema': ['schema.ts']}
        self.write('drizzle/0000_base.sql', 'CREATE TABLE "old" ("id" uuid PRIMARY KEY NOT NULL);')
        self.write('drizzle/meta/_journal.json', json.dumps({'entries': [{'idx': 0, 'tag': '0000_base', 'when': 100}]}))
        self.write('drizzle.config.ts', 'config')
        self.write('schema.ts', 'old')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')

    def migrate(self, sql, when=200, tag='0001_next'):
        self.git('reset', '-q', '--hard', self.base)
        self.write(f'drizzle/{tag}.sql', sql)
        self.write(f'drizzle/meta/{tag[:4]}_snapshot.json', '{}')
        entries = json.loads(Path('drizzle/meta/_journal.json').read_text())['entries']
        self.write('drizzle/meta/_journal.json', json.dumps({'entries': entries + [{'idx': len(entries), 'tag': tag, 'when': when}]}))
        self.write('schema.ts', 'new')

    def test_a_migration_that_only_adds_ships(self):
        self.migrations()
        for sql in [
            'CREATE TABLE "grids" (\n\t"id" uuid PRIMARY KEY NOT NULL,\n\t"old_id" uuid\n);\n--> statement-breakpoint\n'
            'ALTER TABLE "grids" ADD CONSTRAINT "grids_old_fk" FOREIGN KEY ("old_id") REFERENCES "public"."old"("id") ON DELETE cascade ON UPDATE no action;'
            '--> statement-breakpoint\nCREATE UNIQUE INDEX "grids_one" ON "grids" USING btree ("old_id") WHERE "grids"."old_id" is not null;',
            'ALTER TABLE "old" ADD COLUMN "note" text;',
            'ALTER TABLE "old" ADD COLUMN "count" integer DEFAULT 0 NOT NULL;',
            "ALTER TABLE \"old\" ADD COLUMN \"tags\" jsonb DEFAULT '[]'::jsonb NOT NULL;",
            "CREATE TABLE \"grids\" (\"sep\" text DEFAULT ';', \"dash\" text DEFAULT '--');",
        ]:
            with self.subTest(sql=sql):
                self.migrate(sql)
                self.commit()
                self.assertEqual((self.risk()['tier'], self.risk()['needs_you']), ('ship', []))

    def test_a_migration_that_can_change_or_lock_rows_stops(self):
        self.migrations()
        for sql in [
            'DROP TABLE "old";',
            'ALTER TABLE "old" RENAME COLUMN "id" TO "key";',
            'ALTER TABLE "old" ALTER COLUMN "id" TYPE text;',
            'ALTER TABLE "old" ADD COLUMN "name" text NOT NULL;',
            'ALTER TABLE "old" ADD COLUMN "at" timestamp DEFAULT now();',
            'ALTER TABLE "old" ADD COLUMN "name" text, DROP COLUMN "id";',
            'ALTER TABLE "old" ADD COLUMN "new_id" uuid REFERENCES "public"."old"("id");',
            'CREATE INDEX "old_id" ON "old" USING btree ("id");',
            'CREATE TABLE "grids" ("id" uuid);--> statement-breakpoint\n'
            'ALTER TABLE "old" ADD CONSTRAINT "old_grid_fk" FOREIGN KEY ("id") REFERENCES "public"."grids"("id");',
            'CREATE TABLE "grids" ("id" uuid);--> statement-breakpoint\nUPDATE "old" SET "id" = null;',
            '-- nothing',
            # PostgreSQL reads these differently from a pattern match.
            "CREATE TABLE \"grids\" (\"a\" text DEFAULT ')--'); DROP TABLE \"old\";",
            'CREATE TABLE "grids" ("a" int /* ); DROP TABLE "old"; */);',
            'CREATE TABLE "grids" ("a" text DEFAULT $$x$$);',
            'ALTER TABLE "old" ADD COLUMN "n" integer NOT NULL -- DEFAULT 0',
            'ALTER TABLE "old" ADD COLUMN "n" serial;',
            'ALTER TABLE "other"."old" ADD COLUMN "note" text;',
            'CREATE TABLE IF NOT EXISTS "old" ("id" uuid);--> statement-breakpoint\nCREATE UNIQUE INDEX "u" ON "old" ("id");',
            'CREATE TABLE "grids" ("id" uuid) INHERITS ("old");',
            "ALTER TABLE \"old\" ADD COLUMN \"n\" text DEFAULT 'x'::text UNIQUE;",
            "ALTER TABLE \"old\" ADD COLUMN \"n\" text DEFAULT 'x'::counter;",
            'CREATE TABLE "grids" ("id" integer);-- --> statement-breakpoint DROP TABLE "old";',
            'CREATE TABLE "grids" ("id" integer);-- comment\rDROP TABLE "old";',
        ]:
            with self.subTest(sql=sql):
                self.migrate(sql)
                self.commit()
                self.assertEqual(self.risk()['tier'], 'stop')

    def test_a_migration_stops_when_the_files_around_it_are_not_new_or_in_order(self):
        self.migrations()
        adds = 'ALTER TABLE "old" ADD COLUMN "note" text;'

        def journal(edit):
            data = json.loads(Path('drizzle/meta/_journal.json').read_text())
            edit(data['entries'])
            self.write('drizzle/meta/_journal.json', json.dumps(data))

        def second(when):
            self.write('drizzle/0002_more.sql', adds)
            journal(lambda e: e.append({'idx': 2, 'tag': '0002_more', 'when': when}))
        cases = {
            'two new entries run backwards': lambda: (self.migrate(adds, when=300), second(200)),
            'two new entries share a time': lambda: (self.migrate(adds, when=200), second(200)),
            'an old journal entry changed': lambda: (self.migrate(adds), journal(lambda e: e[0].update(when=1))),
            'runs before an older migration': lambda: self.migrate(adds, when=50),
            'journal tag names another file': lambda: (self.migrate(adds), self.write('drizzle/0001_other.sql', adds)),
            'old migration edited': lambda: (self.migrate(adds), self.write('drizzle/0000_base.sql', 'DROP TABLE "old";')),
            'drizzle config changed': lambda: (self.migrate(adds), self.write('drizzle.config.ts', 'other')),
            'schema without a migration': lambda: (self.git('reset', '-q', '--hard', self.base), self.write('schema.ts', 'new')),
        }
        for name, change in cases.items():
            with self.subTest(name):
                change()
                self.commit()
                self.assertEqual(self.risk()['tier'], 'stop')

    def test_a_patched_dependency_stops(self):
        self.write('package.json', json.dumps({'dependencies': {'left-pad': '1.0.0'}}))
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.write('package.json', json.dumps({'dependencies': {'left-pad': '1.0.0'}, 'pnpm': {'patchedDependencies': {'left-pad@1.0.0': 'patches/left-pad.patch'}}}))
        self.commit()
        self.assertEqual(self.risk()['tier'], 'stop')

    def test_the_repo_config_stops_pnpm_hooks_and_patches(self):
        import tomllib
        cfg = tomllib.loads((Path(__file__).resolve().parents[2] / '.gate/config.toml').read_text())
        stop = [p for r in cfg['rules'] if r['tier'] == 'stop' for p in r['paths']]
        for path in ['.pnpmfile.cjs', 'apps/web/.pnpmfile.cjs', 'patches/left-pad.patch', 'pnpm-workspace.yaml', '.npmrc']:
            with self.subTest(path):
                self.assertTrue(gate.matches(path, stop))

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
