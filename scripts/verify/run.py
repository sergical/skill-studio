#!/usr/bin/env python3
"""Exercise built Skill Studio processes in disposable homes; retain evidence."""
import argparse
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/skill-studio')
    parser.add_argument('--out', type=Path, default=ROOT / '.gate/out')
    parser.add_argument('--cli-only', action='store_true', help='Only the CLI/npm smoke subset, not a full verification')
    parser.add_argument('--path', default='/usr/bin:/bin', help='Runtime PATH, for example the Node directory for an npm launcher')
    parser.add_argument('--ui-url', help='An existing Vite dev harness URL')
    parser.add_argument('--rust-artifacts', type=Path, help='cargo test --no-run --message-format=json output')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    scenarios = []

    def scenario(name, intent, action):
        transcript = []
        status, notes = 'pass', 'Observed expected state.'
        with tempfile.TemporaryDirectory(prefix='skill-studio-verify-') as directory:
            home = Path(directory).resolve()
            env = {'HOME': str(home), 'PATH': args.path, 'SKILL_STUDIO_HOME': str(home),
                   'CODEX_HOME': str(home / '.codex'), 'XDG_CONFIG_HOME': str(home / '.config'),
                   'XDG_DATA_HOME': str(home / '.local/share'), 'DOTAGENTS_HOME': str(home / '.agents'),
                   'SKILL_STUDIO_TELEMETRY': '0', 'SKILL_STUDIO_MCP_DEV_TOOLS': '1',
                   'TMPDIR': str(home), 'DO_NOT_TRACK': '1'}
            assert home != Path.home().resolve(), 'Refusing real HOME'
            try:
                action(home, env, transcript)
            except Exception as error:
                status, notes = 'fail', str(error)
            # Evidence is portable and contains only fixture state.
            log = '\n'.join(transcript + [notes]).replace(str(home), '$HOME').replace(str(ROOT), '$REPO')
            (out / f'{name}.txt').write_text(log)
        scenarios.append({'name': name, 'intent': intent, 'status': status,
                          'evidence': [{'kind': 'transcript', 'path': f'{name}.txt'}], 'notes': notes.replace(str(home), '$HOME')})
        (out / 'verify.json').write_text(json.dumps({'scenarios': scenarios}, indent=2) + '\n')
        print(f'{status}: {name}', flush=True)

    def cli(home, env, log, *argv):
        command = [str(binary), *argv, '--json']
        result = subprocess.run(command, env=env, cwd=home, capture_output=True, text=True, timeout=30)
        log.extend(['$ skill-studio ' + ' '.join(argv), result.stdout, result.stderr, f'exit={result.returncode}'])
        value = json.loads(result.stdout)
        assert result.returncode == 0 and value['status'] == 'ok', result.stdout
        return value['data']

    def skill(home, root, name):
        folder = home / root / name
        folder.mkdir(parents=True)
        (folder / 'SKILL.md').write_text(f'---\nname: {name}\ndescription: Verification fixture\n---\nKeep these bytes.\n')
        return folder

    def libraries(home, env, log):
        roots = ['.agents/skills', '.claude/skills', '.codex/skills', '.config/opencode/skills',
                 '.pi/agent/skills', '.cursor/skills', '.grok/skills']
        for index, root in enumerate(roots):
            skill(home, root, f'fixture-{index}')
        scan = cli(home, env, log, 'scan')
        assert {f'fixture-{i}' for i in range(len(roots))} <= {s['name'] for s in scan['skills']}
        # Keep a real stdio server alive while a second process scans the same home.
        with subprocess.Popen([str(binary), 'mcp'], env=env, cwd=home, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as server:
            try:
                buffer = bytearray()
                def rpc(request):
                    log.append('> ' + json.dumps(request))
                    server.stdin.write((json.dumps(request) + '\n').encode())
                    server.stdin.flush()
                    if 'id' not in request:
                        return None
                    with selectors.DefaultSelector() as selector:
                        selector.register(server.stdout, selectors.EVENT_READ)
                        deadline = time.monotonic() + 30
                        while time.monotonic() < deadline:
                            if b'\n' not in buffer:
                                assert selector.select(max(0, deadline - time.monotonic())), 'MCP response timeout'
                                chunk = os.read(server.stdout.fileno(), 65536)
                                assert chunk, 'MCP closed stdout'
                                buffer.extend(chunk)
                                continue
                            line, _, rest = buffer.partition(b'\n')
                            buffer[:] = rest
                            log.append('< ' + line.decode())
                            response = json.loads(line)
                            if response.get('id') == request['id']:
                                assert 'error' not in response, response
                                return response['result']
                    raise AssertionError('MCP response timeout')
                rpc({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
                    'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'verify', 'version': '1'}}})
                rpc({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
                result = rpc({'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call', 'params': {'name': 'scan', 'arguments': {}}})
                assert not result.get('isError'), result
                mcp = result['structuredContent']
                assert mcp['status'] == 'ok', mcp
                assert {s['name'] for s in mcp['data']['skills']} == {s['name'] for s in scan['skills']}
                cli(home, env, log, 'scan')
            finally:
                server.kill()
                server.wait(timeout=5)

    def roundtrip(home, env, log):
        source = skill(home, '.agents/skills', 'roundtrip')
        (source / 'run.sh').write_text('#!/bin/sh\nexit 0\n')
        (source / 'run.sh').chmod(0o755)
        link = home / '.claude/skills/roundtrip'
        link.parent.mkdir(parents=True)
        link.symlink_to('../../.agents/skills/roundtrip')
        scan = cli(home, env, log, 'scan')
        deployment = next(d for s in scan['skills'] if s['name'] == 'roundtrip' for d in s['deployments'] if d['root']['kind']['kind'] == 'universal')
        cli(home, env, log, 'split', '--deployment-id', deployment['id'], '--harness', 'claude-code', '--harness', 'codex')
        first = cli(home, env, log, 'undo')
        assert source.is_dir() and (source / 'SKILL.md').read_bytes().endswith(b'Keep these bytes.\n')
        assert link.is_symlink() and os.readlink(link) == '../../.agents/skills/roundtrip'
        assert not (home / '.codex/skills/roundtrip').exists()
        restored = cli(home, env, log, 'scan')
        assert any(d['root']['kind']['kind'] == 'universal' for s in restored['skills'] if s['name'] == 'roundtrip' for d in s['deployments'])
        cli(home, env, log, 'restore', '--event-id', first['restore_event_id'])
        for root in ['.claude/skills', '.codex/skills']:
            copy = home / root / 'roundtrip'
            assert (copy / 'SKILL.md').read_bytes().endswith(b'Keep these bytes.\n')
            assert (copy / 'run.sh').stat().st_mode & 0o111
        log.append('Both split copies and executable modes survived undo of undo.')

    def park(home, env, log):
        skill(home, '.agents/skills', 'foo')
        agents = home / '.agents'
        originals = {'agents.toml': 'version = 1\n\n[[skills]]\nname = "foo"\nsource = "owner/foo"\n\n[[skills]]\nname = "bar"\nsource = "owner/bar"\n',
                     'agents.lock': 'version = 1\n\n[skills.foo]\nsource = "owner/foo"\n', '.gitignore': '/skills/foo\n!/skills/foo/\n'}
        for name, contents in originals.items():
            (agents / name).write_text(contents)
        bindir = home / 'bin'
        bindir.mkdir()
        shim = bindir / 'shim'
        shim.write_text("""#!/bin/sh
[ "${0##*/}" = dotagents ] || exit 71
[ "$1" = remove ] || exit 0
/bin/cat > "$DOTAGENTS_HOME/agents.toml" <<'TOML'
version = 1

[[skills]]
name = "bar"
source = "owner/bar"
TOML
printf 'version = 1\\n' > "$DOTAGENTS_HOME/agents.lock"
/bin/cat > "$DOTAGENTS_HOME/.gitignore" <<'IGNORE'
!/skills/foo/
IGNORE
""")
        shim.chmod(0o755)
        (bindir / 'dotagents').symlink_to('shim')
        env['PATH'] = str(bindir) + ':' + args.path
        cli(home, env, log, 'park', 'foo')
        assert not (agents / 'skills/foo').exists()
        cli(home, env, log, 'unpark', 'foo')
        assert (agents / 'skills/foo/SKILL.md').exists()
        for name, contents in originals.items():
            actual = (agents / name).read_text()
            log.append(f'{name}: expected={contents!r}, observed={actual!r}')
            if name == 'agents.toml':
                expected = tomllib.loads(contents)
                observed = tomllib.loads(actual)
                expected['skills'].sort(key=lambda row: row['name'])
                observed['skills'].sort(key=lambda row: row['name'])
                assert observed == expected, 'Manifest ownership changed'
            else:
                assert actual == contents, f'{name} was not restored exactly'
        log.append('Symlinked shim retained argv[0]; skill ownership restored; lock and ignore files restored exactly.')

    scenario('libraries-cli-mcp', 'Find fixture skills for all six agents and the shared library through real CLI and MCP processes.', libraries)
    scenario('split-undo-undo', '#345 → #365: keep both copies and script modes when undoing a split restore.', roundtrip)
    scenario('park-shim-restore', '#401 → #414: run shims by name and restore skill ownership and installer state on Turn on.', park)

    if args.rust_artifacts:
        artifacts = [json.loads(line) for line in args.rust_artifacts.read_text().splitlines() if line.startswith('{')]
        executables = {row['executable'] for row in artifacts if row.get('reason') == 'compiler-artifact' and row.get('executable') and row.get('profile', {}).get('test') and row['target']['name'] in {'skill_studio_lib', 'skill_studio_host', 'install', 'lease'}}
        regressions = [
            ('packaged-shebang', '#301: resolve npx and the node required by its shebang under a minimal PATH.', 'a_packaged_apps_minimal_process_path_still_resolves_a_bare_npx_and_the_node_its_shebang_needs_or_names_which_lookup_starved'),
            ('packaged-path', '#301 → #305: resolve a bare desktop command with a minimal PATH.', 'a_bare_program_outside_the_process_path_runs_through_the_command_runner_or_names_the_spawn_error'),
            ('install-roots', '#345 → #364: write installs under configured roots.', 'per_harness_install_with_a_configured_'),
            ('lease-two-processes', '#227 → #304: another process must see a held write lease as busy.', 'a_second_process_holding_the_lease_gets_busy'),
            ('lease-undo', '#227 → #304: undo must complete with the command lease held.', 'undo_of_a_done_make_independent_copy_row_restores_the_link_and_removes_copy_ownership'),
            ('lease-copy-undo', '#227 → #304: copy and undo must complete with the command lease held.', 'make_independent_copy_run_the_way_the_command_runs_it_does_not_self_deadlock_on_the_write_lease'),
        ]
        for name, intent, test_filter in regressions:
            def regression(home, env, log, test_filter=test_filter):
                matches = []
                for executable in sorted(executables):
                    listing = subprocess.run([executable, '--list', test_filter], env=env, cwd=home, capture_output=True, text=True, timeout=30)
                    if ': test' in listing.stdout:
                        matches.append(executable)
                assert matches, f'No compiled test matches {test_filter}'
                for executable in matches:
                    result = subprocess.run([executable, test_filter, '--nocapture', '--test-threads=1'], env=env, cwd=home, capture_output=True, text=True, timeout=60)
                    log.extend([Path(executable).name, result.stdout, result.stderr])
                    assert result.returncode == 0, result.stdout + result.stderr
            scenario(name, intent, regression)
    if args.ui_url:
        def ui(home, env, log):
            result = subprocess.run([subprocess.check_output([shutil.which('node'), '-p', 'process.execPath'], text=True).strip(), str(ROOT / 'scripts/verify/ui.mjs'), args.ui_url, str(out)],
                                    cwd=ROOT, env={**env, 'PLAYWRIGHT_BROWSERS_PATH': os.environ.get('PLAYWRIGHT_BROWSERS_PATH', str(Path.home() / 'Library/Caches/ms-playwright'))}, capture_output=True, text=True, timeout=90)
            log.extend([result.stdout, result.stderr])
            assert result.returncode == 0, result.stdout + result.stderr
        scenario('desktop-affordances', '#200/#211 → #293 and #281 → #331: omit unavailable pack import and misleading Settings health cards.', ui)
        if scenarios[-1]['status'] == 'pass':
            scenarios[-1]['evidence'].extend([{'kind': 'screenshot', 'path': name} for name in ['add-methods.png', 'settings.png']])
            (out / 'verify.json').write_text(json.dumps({'scenarios': scenarios}, indent=2) + '\n')
    if not args.cli_only:
        for flag, value in [('--rust-artifacts', args.rust_artifacts), ('--ui-url', args.ui_url)]:
            if not value:
                def missing(home, env, log, flag=flag):
                    raise AssertionError(f'Blocked: full verification requires {flag}; --cli-only is only a release smoke subset')
                scenario('missing-' + flag[2:], 'Run all requested regression checks before accepting verification.', missing)
    raise SystemExit(any(s['status'] == 'fail' for s in scenarios))


if __name__ == '__main__':
    main()
