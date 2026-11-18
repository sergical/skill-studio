#!/usr/bin/env python3
"""Configurable merge proof and risk classification. Requires Python 3.11+."""
import argparse
from datetime import date
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

sys.modules.setdefault('gate', sys.modules[__name__])

TIERS = ['ship', 'show', 'stop']


def run(*args, cwd=None, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def git(*args):
    return run('git', *args)


def config():
    return tomllib.loads(Path('.gate/config.toml').read_text())


def gh(endpoint, method='GET', data=None):
    args = ['gh', 'api', '--method', method, endpoint]
    if data is not None:
        args += ['--input', '-']
    return json.loads(subprocess.check_output(args, input=json.dumps(data) if data is not None else None, text=True))


def pages(endpoint):
    return json.loads(run('gh', 'api', '--paginate', '--slurp', endpoint))


def read_at(ref, path):
    result = subprocess.run(['git', 'show', f'{ref}:{path}'], capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else ''


def matches(path, patterns):
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


def classify(cfg, base, head='HEAD', raise_to=None, reason=None, author=''):
    # -z gives raw names. Without it, git quotes a name with a backslash or a
    # control character, and the quoted name matches no rule.
    files = [f for f in subprocess.check_output(['git', 'diff', '--name-only', '--no-renames', '-z', base, head], text=True).split('\0') if f]
    hits = []
    if read_at(base, '.gate/config.toml') != read_at(head, '.gate/config.toml'):
        hits.append({'rule': 'policy-change', 'tier': 'stop', 'files': ['.gate/config.toml']})
    for rule in cfg.get('rules', []):
        affected = [p for p in files if matches(p, rule['paths'])]
        if affected:
            hits.append({'rule': rule['id'], 'tier': rule['tier'], 'files': affected})
    for path in files:
        if not matches(path, cfg.get('version_files', []) + cfg.get('dependency_files', [])):
            continue
        old, new = read_at(base, path), read_at(head, path)
        if matches(path, cfg.get('version_files', [])):
            if version(path, old) != version(path, new):
                hits.append({'rule': 'version', 'tier': 'stop', 'files': [path]})
        if path.endswith('package.json') and old and new and json.loads(old).get('scripts') != json.loads(new).get('scripts'):
            hits.append({'rule': 'check-scripts', 'tier': 'stop', 'files': [path]})
        if matches(path, cfg.get('dependency_files', [])):
            before, after = dependencies(path, old), dependencies(path, new)
            # A new name, or the same name from another source (an npm alias,
            # a git URL, a local path), is outside code we have not trusted.
            # Removing an entry that came from another source changes what is
            # installed too.
            added = (set(after) - set(before)) | {k for k in set(before) & set(after) if source(before[k]) != source(after[k])} | {k for k in set(before) - set(after) if source(before[k])}
            patch = author in cfg.get('patch_bots', []) and set(before) == set(after) and all(
                before.get(k) == v or patch_bump(before.get(k, ''), v) for k, v in after.items())
            if added or (author in cfg.get('patch_bots', []) and not patch):
                hits.append({'rule': 'dependencies', 'tier': 'stop', 'files': [path]})
    tier = max((h['tier'] for h in hits), key=TIERS.index, default='ship')
    if raise_to:
        if not reason:
            raise ValueError('--raise requires --reason')
        if TIERS.index(raise_to) < TIERS.index(tier):
            raise ValueError('The minimum tier cannot be lowered')
        hits.append({'rule': 'manual', 'tier': raise_to, 'reason': reason})
        tier = raise_to
    return {'tier': tier, 'reasons': hits, 'needs_you': needs_you(cfg, hits), 'base': base, 'head': git('rev-parse', head)}


def needs_you(cfg, hits):
    """One entry per stop rule: what the owner decides, why a person must, and the files."""
    texts, grouped = cfg.get('owner', {}), {}
    for hit in hits:
        if hit['tier'] != 'stop':
            continue
        text = texts.get(hit['rule'], {})
        if not (str(text.get('ask', '')).strip() and str(text.get('why', '')).strip()):
            text = {
                'ask': hit.get('reason') or f"Merge this PR? Rule `{hit['rule']}` matched.",
                'why': 'An agent raised this PR to stop by hand.' if hit['rule'] == 'manual'
                else f"`.gate/config.toml` has no complete [owner.{hit['rule']}] text yet.",
            }
        item = grouped.setdefault(hit['rule'], {'rule': hit['rule'], 'ask': text['ask'], 'why': text['why'], 'files': []})
        item['files'] += [f for f in hit.get('files', []) if f not in item['files']]
    return list(grouped.values())


def dependencies(path, text):
    if not text:
        return {}
    if path.endswith('.json'):
        data = json.loads(text)
        result = {f'{section}/{k}': str(v) for section in ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies'] for k, v in data.get(section, {}).items()}
        # Overrides and resolutions replace a package anywhere in the tree,
        # including with another name, a tarball or a git source.
        # npm writes `"foo": "1"` and `"foo": {".": "1"}` for the same override.
        def flatten(node, prefix):
            for key, value in node.items():
                if isinstance(value, dict):
                    flatten(value, f'{prefix}/{key}')
                else:
                    result[prefix if key == '.' else f'{prefix}/{key}'] = str(value)
        pnpm = data.get('pnpm')
        for prefix, node in [('overrides', data.get('overrides')), ('resolutions', data.get('resolutions')), ('pnpm/overrides', pnpm.get('overrides') if isinstance(pnpm, dict) else None)]:
            if isinstance(node, dict):
                flatten(node, prefix)
        return result
    data = tomllib.loads(text)
    result = {}
    def walk(node, prefix=''):
        for key, value in node.items():
            if key.endswith('dependencies') and isinstance(value, dict):
                for name, spec in value.items():
                    result[f'{prefix}{key}/{name}'] = spec
            elif isinstance(value, dict):
                walk(value, prefix + key + '/')
    walk(data)
    return result


def source(spec):
    """Where a dependency comes from. None is the default registry."""
    if isinstance(spec, dict):
        return {k: spec[k] for k in ['git', 'path', 'registry', 'package', 'branch', 'tag', 'rev'] if k in spec} or None
    spec = str(spec)
    if spec.startswith('npm:'):
        # npm:name@version: the source is the name, not the version.
        name, _, _ = spec[4:].rpartition('@')
        return 'npm:' + (name or spec[4:])
    # `$name` reuses another dependency's spec, so a new reference can be a new source.
    return spec if ':' in spec or '/' in spec or spec.startswith('$') else None


def version(path, text):
    if not text:
        return None
    data = json.loads(text) if path.endswith('.json') else tomllib.loads(text)
    if path.endswith('.json'):
        return data.get('version')
    return [data.get('package', {}).get('version'), data.get('workspace', {}).get('package', {}).get('version')]


def patch_bump(before, after):
    if isinstance(before, dict) or isinstance(after, dict):
        if not isinstance(before, dict) or not isinstance(after, dict):
            return False
        if {k: v for k, v in before.items() if k != 'version'} != {k: v for k, v in after.items() if k != 'version'}:
            return False
        before, after = before.get('version', ''), after.get('version', '')
    if not isinstance(before, str) or not isinstance(after, str):
        return False
    a, b = re.fullmatch(r'([~^]?)(\d+)\.(\d+)\.(\d+)', before), re.fullmatch(r'([~^]?)(\d+)\.(\d+)\.(\d+)', after)
    return bool(a and b and a.groups()[:3] == b.groups()[:3] and int(b[4]) >= int(a[4]))


def capture_contract(spec):
    env = os.environ | spec.get('env', {})
    if spec['kind'] == 'files':
        return {p: json.loads(Path(p).read_text()) for p in sorted(git('ls-files', spec['glob']).splitlines())}
    if spec['kind'] == 'cli':
        result, pending = {}, [[]] + spec.get('extra_commands', [])
        while pending:
            command = pending.pop(0)
            key = ' '.join(command) or '<root>'
            if key in result:
                continue
            text = run(*spec['command'], *command, '--help', env=env)
            result[key] = text
            section = re.search(r'(?m)^Commands:\n((?:  .*\n?)+)', text)
            if section:
                pending.extend(command + [line.split()[0]] for line in section[1].splitlines() if line.split()[0] != 'help')
        return result
    if spec['kind'] == 'mcp':
        requests = [dict(jsonrpc='2.0', id=1, method='initialize', params={'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'gate', 'version': '1'}}), dict(jsonrpc='2.0', method='notifications/initialized'), dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
        proc = subprocess.Popen(spec['command'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        try:
            output, errors = proc.communicate('\n'.join(json.dumps(r) for r in requests) + '\n', timeout=30)
            for line in output.splitlines():
                reply = json.loads(line)
                if reply.get('id') == 2 and 'result' in reply:
                    if reply['result'].get('nextCursor'):
                        raise ValueError('Paginated MCP contracts need a full export command')
                    return sorted([{'name': t['name'], 'inputSchema': t['inputSchema']} for t in reply['result']['tools']], key=lambda t: t['name'])
            raise ValueError('MCP tools/list failed: ' + errors[:300])
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    raise ValueError('Unknown contract kind')


def snapshots(cfg, update=False, prepare=False):
    if prepare:
        import platform
        for command in cfg.get('prepare_' + platform.system().lower(), []):
            subprocess.run(command, check=True)
    for spec in cfg.get('contracts', []):
        actual = json.dumps(capture_contract(spec), indent=2, sort_keys=True) + '\n'
        path = Path(spec['snapshot'])
        if update:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(actual)
        elif not path.exists() or path.read_text() != actual:
            raise ValueError(f'Stale contract: {path}. Run gate snapshots --update and review the diff.')


def state_path():
    branch = git('symbolic-ref', '--short', 'HEAD')
    path = Path(git('rev-parse', '--git-path', 'gate')) / (hashlib.sha256(branch.encode()).hexdigest() + '.json')
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    risk = sub.add_parser('risk')
    risk.add_argument('--base', default=None)
    risk.add_argument('--head', default='HEAD')
    risk.add_argument('--json', action='store_true')
    risk.add_argument('--raise', dest='raise_to', choices=TIERS)
    risk.add_argument('--reason')
    risk.add_argument('--author', default='')
    snap = sub.add_parser('snapshots')
    snap.add_argument('--update', action='store_true')
    snap.add_argument('--prepare', action='store_true')
    from proof import arguments, start, review
    arguments(sub)
    metric = sub.add_parser('metrics')
    metric.add_argument('--since', required=True)
    metric.add_argument('--until', default=date.today().isoformat())
    metric.add_argument('--last', type=int)
    args = parser.parse_args()
    os.chdir(git('rev-parse', '--show-toplevel'))
    cfg = config()
    if args.command == 'risk':
        # The rules come from the trusted base as it is now, so a rule added
        # after the branch started still applies. The merge base only decides
        # which files changed.
        base = args.base or cfg['base']
        policy = read_at(base, '.gate/config.toml')
        merge_base = git('merge-base', base, args.head)
        result = classify(tomllib.loads(policy) if policy else cfg, merge_base, args.head, args.raise_to, args.reason, args.author)
        print(json.dumps(result, indent=2) if args.json else result['tier'] + '\n' + '\n'.join(f"- {r['rule']}: {r.get('reason', ', '.join(r.get('files', [])))}" for r in result['reasons'])
              + ''.join(f"\nNeeds you: {n['ask']}\n  Why: {n['why']}" for n in result['needs_you']))
    elif args.command == 'snapshots':
        snapshots(cfg, args.update, args.prepare)
    elif args.command == 'start':
        start(args, cfg)
    elif args.command == 'review-status':
        review(args, cfg)
    elif args.command == 'metrics':
        from metrics import metrics
        metrics(args, cfg)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'gate: {error}', file=sys.stderr)
        sys.exit(1)
