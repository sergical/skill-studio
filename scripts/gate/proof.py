"""Intent and exact-commit review proof."""
import json
from pathlib import Path
import re
from gate import gh, git, state_path


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


def arguments(sub):
    parser = sub.add_parser('start')
    parser.add_argument('intent', nargs='?')
    parser.add_argument('--issue')
    parser.add_argument('--author-model')
    parser = sub.add_parser('review-status')
    parser.add_argument('--verdict', choices=['pass', 'fail'], required=True)
    parser.add_argument('--reviewer', required=True)
    parser.add_argument('--findings', required=True)
