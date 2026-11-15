"""Intent, exact-commit review proof, and PR cards."""
import json
from pathlib import Path
import re
import tomllib
from urllib.parse import unquote
from gate import classify, gh, git, run, state_path, read_at, TIERS

START, END = '<!-- gate:card -->', '<!-- /gate:card -->'
FILE_TOOLS = {
    'Read', 'Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'apply_patch',
    'functions.apply_patch', 'read_file', 'write_file', 'edit_file',
    'mcp__filesystem__read_file', 'mcp__filesystem__write_file',
    'mcp__filesystem__edit_file', 'mcp__files__write',
}


def local_state():
    path = state_path()
    return json.loads(path.read_text()) if path.exists() else {}


def start(args, cfg=None):
    intent = args.intent or args.issue
    if not intent:
        raise ValueError('Supply an intent or --issue URL')
    if args.issue and (not re.fullmatch(r'https://[^\s]+', args.issue) or (cfg and cfg.get('issue_patterns') and not any(re.fullmatch(p, args.issue) for p in cfg['issue_patterns']))):
        raise ValueError('--issue requires an HTTPS URL')
    state = local_state()
    state['intent'] = intent
    if args.issue:
        state['issue'] = args.issue
    if args.author_model:
        state['author_model'] = args.author_model
    state_path().write_text(json.dumps(state, indent=2) + '\n')
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
    pr = json.loads(run('gh', 'pr', 'view', '--json', 'number,body,headRefOid,baseRefOid,author'))
    sha = git('rev-parse', 'HEAD')
    if pr['headRefOid'] != sha:
        raise ValueError('Push the local head before writing its card')
    base = git('merge-base', pr['baseRefOid'], sha)
    policy = read_at(base, '.gate/config.toml')
    risk = classify(tomllib.loads(policy) if policy else cfg, base, sha, author=pr['author']['login'])
    raised_path = state_path().with_suffix('.risk.json')
    raised = json.loads(raised_path.read_text()) if raised_path.exists() else {}
    if raised.get('head') == sha:
        for reason in raised['reasons']:
            if reason['rule'] == 'manual':
                risk['reasons'].append(reason)
                risk['tier'] = max(risk['tier'], reason['tier'], key=TIERS.index)
    statuses = gh(f"repos/{cfg['repository']}/commits/{sha}/status")['statuses']
    states = {s['context']: s['state'] for s in reversed(statuses)}
    verify_path = Path('.gate/out/verify.json')
    verify = json.loads(verify_path.read_text()) if verify_path.exists() else None
    review_path = state_path().with_suffix('.review.json')
    record = json.loads(review_path.read_text()) if review_path.exists() else {}
    if record.get('head') != sha:
        record = {}
    scenarios = verify.get('scenarios', []) if verify else []
    stale = bool(verify and verify.get('head') and verify['head'] != sha)
    if stale:
        scenarios = []
    status = 'not run' if not scenarios else 'pass' if all(s['status'] == 'pass' for s in scenarios) else 'fail'
    if stale:
        status = 'not run (stale head)'
    elif scenarios and not verify.get('head'):
        status = 'unbound evidence; reported ' + status
    state = local_state()
    verdict = record.get('verdict', states.get('gate/review', 'not run'))
    reasons = '; '.join(r['rule'] + ': ' + r.get('reason', ', '.join(r.get('files', []))) for r in risk['reasons']) or 'no rule matched'
    lines = [
        START,
        f"<details><summary>Gate record: tier <b>{cell(risk['tier'])}</b> · review {cell(verdict)} · verify {cell(status)}</summary>",
        '',
        '| | |',
        '| :-- | :-- |',
        f"| Intent | {cell(state.get('intent', 'not recorded'))} |",
        f'| Head | `{sha}` |',
        f'| Reasons | {cell(reasons)} |',
        f"| Review | {cell(record.get('reviewer', 'unknown'))}: {cell(verdict)} |",
        f"| Writer | {cell(state.get('author_model', 'not recorded'))} |",
    ]
    if scenarios:
        lines += ['', '| Scenario | Intent | Status | Evidence | Notes |', '| :-- | :-- | :-- | :-- | :-- |']
        for scenario in scenarios:
            evidence = []
            for entry in scenario['evidence']:
                path = entry['path']
                if Path(path).is_absolute() or re.match(r'^[A-Za-z]:', path):
                    raise ValueError('Evidence paths must be repo-relative or HTTPS URLs')
                # Local paths cannot be viewed by a reviewer on GitHub.
                if path.startswith('https://') and not re.fullmatch(r'https://[^\s()<>|"\\]+', path):
                    raise ValueError('Invalid HTTPS evidence URL')
                if path.startswith('https://'):
                    evidence.append(f"![{cell(entry['kind'])}]({path})" if entry['kind'] == 'screenshot' else f"[{cell(entry['kind'])}]({path})")
                else:
                    evidence.append(f"{cell(entry['kind'])}: `{cell(path)}` (local; upload for PR viewing)")
            lines.append('| ' + ' | '.join([cell(scenario['name']), cell(scenario['intent']), cell(scenario['status']), '<br>'.join(evidence), cell(scenario.get('notes', ''))]) + ' |')
    lines += ['', '</details>', END]
    body = replace_card(pr['body'], '\n'.join(lines))
    gh(f"repos/{cfg['repository']}/pulls/{pr['number']}", 'PATCH', {'body': body})
    print('Proof card updated')


def guard():
    import sys
    try:
        event = json.load(sys.stdin)
        tool = event.get('tool_name')
        if tool in FILE_TOOLS:
            return
        command = unquote(json.dumps(event.get('tool_input', event)))
        command = re.sub(r"['\"\\]", '', command)
        if re.search(r'gate\s*[:/]\s*approved', command, re.I):
            raise ValueError('Only a human may grant gate approval')
    except Exception as error:
        print(str(error), file=sys.stderr)
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
    sub.add_parser('card')
    sub.add_parser('guard')
