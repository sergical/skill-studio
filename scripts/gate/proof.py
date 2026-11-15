"""Intent, exact-commit review proof, and PR cards."""
import json
from pathlib import Path
import re
import tomllib
from gate import classify, gh, git, pages, run, state_path, read_at, TIERS

START, END = '<!-- gate:card -->', '<!-- /gate:card -->'
BYPASS_MARK = '<!-- gate:bypass -->'
PASSING = {'success', 'neutral', 'skipped'}


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
    open_findings = sum(not f['resolved'] for f in findings)
    record = {'head': sha, 'reviewer': args.reviewer, 'verdict': verdict, 'findings': findings}
    gh(f"repos/{cfg['repository']}/statuses/{sha}", 'POST', {'state': 'success' if verdict == 'pass' else 'failure', 'context': 'gate/review', 'description': f'{verdict}: {args.reviewer}, {open_findings} open'[:140]})
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
    pr = json.loads(run('gh', 'pr', 'view', '--json', 'number,body,headRefOid,baseRefOid,author,title,statusCheckRollup'))
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
    for check in pr['statusCheckRollup']:
        name = check.get('name') or check.get('context')
        states[name] = (check.get('conclusion') or check.get('status') or check.get('state') or 'pending').lower()
    verify_path = Path('.gate/out/verify.json')
    verify = json.loads(verify_path.read_text()) if verify_path.exists() else None
    review_path = state_path().with_suffix('.review.json')
    record = json.loads(review_path.read_text()) if review_path.exists() else {}
    if record.get('head') != sha:
        record = {}
    lines = [START, f"| Intent | {cell(local_state().get('intent', 'not recorded'))} |", '| :-- | :-- |', f"| Result | {cell(args.result or pr['title'])} |", f'| Head | `{sha}` |', f"| Tier | **{risk['tier']}** |", f"| Reasons | {cell('; '.join(r['rule'] + ': ' + r.get('reason', ', '.join(r.get('files', []))) for r in risk['reasons']) or 'no rule matched')} |", f"| Review | {cell(record.get('reviewer', 'unknown'))}: {cell(record.get('verdict', states.get('gate/review', 'not run')))} |", '', '| Check | Status |', '| :-- | :-- |']
    names = [cfg['check'], *cfg.get('ci_jobs', []), *states, 'gate/risk', 'gate/review', 'gate/approved']
    for name in dict.fromkeys(names):
        state = states.get(name, 'not reported')
        mark = '✅' if state == 'success' else '❌' if state in ['failure', 'error', 'cancelled', 'timed_out'] else '⏳'
        lines.append(f'| {cell(name)} | {mark} {cell(state)} |')
    scenarios = verify.get('scenarios', []) if verify else []
    stale = bool(verify and verify.get('head') and verify['head'] != sha)
    if stale:
        scenarios = []
    status = 'not run' if not scenarios else 'pass' if all(s['status'] == 'pass' for s in scenarios) else 'fail'
    if stale:
        status = 'not run (stale head)'
    elif scenarios and not verify.get('head'):
        status = 'unbound evidence; reported ' + status
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
                if path.startswith('https://') and not re.fullmatch(r'https://[^\s()<>|"\\]+', path):
                    raise ValueError('Invalid HTTPS evidence URL')
                if path.startswith('https://'):
                    evidence.append(f"![{cell(entry['kind'])}]({path})" if entry['kind'] == 'screenshot' else f"[{cell(entry['kind'])}]({path})")
                else:
                    evidence.append(f"{cell(entry['kind'])}: `{cell(path)}` (local; upload for PR viewing)")
            lines.append('| ' + ' | '.join([cell(scenario['name']), cell(scenario['intent']), cell(scenario['status']), '<br>'.join(evidence), cell(scenario.get('notes', ''))]) + ' |')
    lines.append(END)
    body = replace_card(pr['body'], '\n'.join(lines))
    gh(f"repos/{cfg['repository']}/pulls/{pr['number']}", 'PATCH', {'body': body})
    print('Proof card updated')


def states_at(repo, sha, merged_at):
    states = {}

    def keep_worst(name, state):
        if states.get(name, 'success') in PASSING:
            states[name] = state

    statuses = sorted((s for page in pages(f'repos/{repo}/commits/{sha}/statuses?per_page=100') for s in page), key=lambda s: s['created_at'])
    latest = {s['context']: s['state'] for s in statuses if s['created_at'] <= merged_at}
    runs = sorted((c for page in pages(f'repos/{repo}/commits/{sha}/check-runs?per_page=100') for c in page['check_runs']), key=lambda c: c['started_at'] or '')
    checks = {}
    for check in runs:
        if (check['started_at'] or '') <= merged_at:
            finished = check['completed_at'] and check['completed_at'] <= merged_at
            checks[check['name']] = check['conclusion'] if finished else 'not finished at merge'
    for name, state in [*latest.items(), *checks.items()]:
        keep_worst(name, state)
    return states


def bypass(args, cfg):
    repo = cfg['repository']
    merged = [p for p in gh(f'repos/{repo}/commits/{args.sha}/pulls') if p.get('merge_commit_sha') == args.sha]
    if not merged:
        print('No pull request merged as this commit')
        return
    pr = gh(f"repos/{repo}/pulls/{merged[0]['number']}")
    rules = gh(f"repos/{repo}/rules/branches/{pr['base']['ref']}")
    required = [c['context'] for r in rules if r['type'] == 'required_status_checks' for c in r['parameters']['required_status_checks']]
    states = states_at(repo, pr['head']['sha'], pr['merged_at'])
    red = {name: states.get(name, 'missing') for name in required if states.get(name) not in PASSING}
    if not red:
        print('Required checks were green at merge')
        return
    lines = [BYPASS_MARK, f"Merged past required checks by **{pr['merged_by']['login']}** at {pr['merged_at']}, on head `{pr['head']['sha']}`.", '', '| Check | State |', '| --- | --- |']
    lines += [f'| {cell(name)} | {cell(state)} |' for name, state in red.items()]
    lines += ['', f"gate/approved on this head: {states.get('gate/approved', 'none')}."]
    gh(f"repos/{repo}/issues/{pr['number']}/comments", 'POST', {'body': '\n'.join(lines)})
    print(f"Bypass recorded on #{pr['number']}: {', '.join(red)}")


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
    parser = sub.add_parser('bypass')
    parser.add_argument('--sha', required=True)
