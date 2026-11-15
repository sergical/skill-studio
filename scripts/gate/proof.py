"""Intent, exact-commit review proof, and PR cards."""
import json
from pathlib import Path
import re
import subprocess
from gate import classify, gh, git, run, state_path

START, END = '<!-- gate:card -->', '<!-- /gate:card -->'


def local_state():
    path = state_path()
    return json.loads(path.read_text()) if path.exists() else {}


def start(args):
    intent = args.intent or args.issue
    if not intent:
        raise ValueError('Supply an intent or --issue URL')
    if args.issue and not re.fullmatch(r'https://[^\s]+', args.issue):
        raise ValueError('--issue requires an HTTPS URL')
    state_path().write_text(json.dumps({'intent': intent, 'author_model': args.author_model}, indent=2) + '\n')
    print('Intent saved for this branch')


def review(args, cfg):
    data = json.loads(Path(args.findings).read_text())
    sha = git('rev-parse', 'HEAD')
    if data.get('head') != sha:
        raise ValueError('Findings must name the current head SHA')
    findings = data.get('findings')
    if not isinstance(findings, list):
        raise ValueError('findings must be an array')
    for finding in findings:
        if finding.get('severity') not in ['low', 'medium', 'high', 'critical'] or not isinstance(finding.get('resolved'), bool):
            raise ValueError('Each finding needs severity and boolean resolved')
    author = local_state().get('author_model') or data.get('author_model')
    if author and author.strip().casefold() == args.reviewer.strip().casefold():
        raise ValueError('Reviewer model must differ from author model')
    verdict = 'fail' if any(f['severity'] in ['high', 'critical'] and not f['resolved'] for f in findings) else args.verdict
    record = {'head': sha, 'reviewer': args.reviewer, 'verdict': verdict, 'findings': findings}
    gh(f"repos/{cfg['repository']}/statuses/{sha}", 'POST', {'state': 'success' if verdict == 'pass' else 'failure', 'context': 'gate/review', 'description': f'{verdict}: {args.reviewer}'[:140]})
    state_path().with_suffix('.review.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2))


def cell(value):
    return str(value).replace('|', '\\|').replace('\n', ' ').replace('<', '&lt;').replace('>', '&gt;')


def replace_card(body, card):
    if START in body or END in body:
        if body.count(START) != 1 or body.count(END) != 1 or body.index(START) > body.index(END):
            raise ValueError('Malformed gate card markers')
        return body[:body.index(START)] + card + body[body.index(END) + len(END):]
    return body.rstrip() + '\n\n' + card + '\n'


def card(args, cfg):
    pr = json.loads(run('gh', 'pr', 'view', '--json', 'number,body,headRefOid,title,statusCheckRollup'))
    sha = git('rev-parse', 'HEAD')
    if pr['headRefOid'] != sha:
        raise ValueError('Push the local head before writing its card')
    risk = classify(cfg, git('merge-base', cfg['base'], sha), sha)
    statuses = gh(f"repos/{cfg['repository']}/commits/{sha}/status")['statuses']
    states = {s['context']: s['state'] for s in reversed(statuses)}
    for check in pr['statusCheckRollup']:
        name = check.get('name') or check.get('context')
        states[name] = (check.get('conclusion') or check.get('status') or check.get('state') or 'pending').lower()
    verify_path = Path('.gate/out/verify.json')
    verify = json.loads(verify_path.read_text()) if verify_path.exists() else None
    review_path = state_path().with_suffix('.review.json')
    record = json.loads(review_path.read_text()) if review_path.exists() else {}
    if record.get('head') != sha:
        record = {}
    lines = [START, f"| Intent | {cell(local_state().get('intent', 'not recorded'))} |", '| :-- | :-- |', f"| Result | {cell(args.result or pr['title'])} |", f'| Head | `{sha}` |', f"| Tier | **{risk['tier']}** |", f"| Reasons | {cell(', '.join(r['rule'] for r in risk['reasons']) or 'no rule matched')} |", f"| Review | {cell(record.get('reviewer', 'unknown'))}: {cell(record.get('verdict', states.get('gate/review', 'not run')))} |", '', '| Check | Status |', '| :-- | :-- |']
    names = [cfg['check'], *cfg.get('ci_jobs', []), *states, 'gate/risk', 'gate/review']
    for name in dict.fromkeys(names):
        state = states.get(name, 'not reported')
        mark = '✅' if state == 'success' else '❌' if state in ['failure', 'error', 'cancelled', 'timed_out'] else '⏳'
        lines.append(f'| {cell(name)} | {mark} {cell(state)} |')
    scenarios = verify.get('scenarios', []) if verify else []
    status = 'not run' if not scenarios else 'pass' if all(s['status'] == 'pass' for s in scenarios) else 'fail'
    lines += [f'| verify | {status} |']
    if scenarios:
        lines += ['', '| Scenario | Intent | Status | Evidence | Notes |', '| :-- | :-- | :-- | :-- | :-- |']
        for scenario in scenarios:
            evidence = []
            for entry in scenario['evidence']:
                path = entry['path']
                if Path(path).is_absolute() or re.match(r'^[A-Za-z]:', path):
                    raise ValueError('Evidence paths must be repo-relative or HTTPS URLs')
                # Local paths cannot be viewed by a reviewer on GitHub.
                if path.startswith('https://'):
                    evidence.append(f"![{cell(entry['kind'])}]({path})" if entry['kind'] == 'screenshot' else f"[{cell(entry['kind'])}]({path})")
                else:
                    evidence.append(f"{cell(entry['kind'])}: `{cell(path)}` (local; upload for PR viewing)")
            lines.append('| ' + ' | '.join([cell(scenario['name']), cell(scenario['intent']), cell(scenario['status']), '<br>'.join(evidence), cell(scenario.get('notes', ''))]) + ' |')
    lines.append(END)
    body = replace_card(pr['body'], '\n'.join(lines))
    gh(f"repos/{cfg['repository']}/pulls/{pr['number']}", 'PATCH', {'body': body})
    print('Proof card updated')


def guard():
    import sys
    event = json.load(sys.stdin)
    command = json.dumps(event.get('tool_input', event))
    if re.search(r'gate(?::|%3[aA])approved', command, re.I):
        print('Only a human may add gate:approved', file=sys.stderr)
        raise SystemExit(2)


def arguments(sub):
    parser = sub.add_parser('start')
    parser.add_argument('intent', nargs='?')
    parser.add_argument('--issue')
    parser.add_argument('--author-model')
    parser = sub.add_parser('review-status')
    parser.add_argument('--verdict', choices=['pass', 'fail'], required=True)
    parser.add_argument('--reviewer', required=True)
    parser.add_argument('--findings', required=True)
    parser = sub.add_parser('card')
    parser.add_argument('--result')
    sub.add_parser('guard')
