"""Historical PR outcomes. Cache API data under the ignored output directory."""
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
import subprocess
from gate import classify, gh, git, pages, run


def collect(cfg, since, until):
    repo = cfg['repository']
    cache = Path('.gate/out/history')
    cache.mkdir(parents=True, exist_ok=True)
    def cached(key, loader):
        path = cache / (key + '.json')
        if not path.exists():
            path.write_text(json.dumps(loader()))
        return json.loads(path.read_text())
    prs = json.loads(run('gh', 'pr', 'list', '--repo', repo, '--state', 'merged', '--limit', '1000', '--json', 'number,title,mergedAt,mergeCommit,headRefOid,author,body', '--search', f'merged:>={since}'))
    if len(prs) == 1000:
        raise ValueError('History exceeds 1000 PRs; narrow --since to avoid a truncated report')
    end = datetime.fromisoformat(until).replace(tzinfo=timezone.utc) + timedelta(days=1)
    def detail(pr):
        n = pr['number']
        pr['files'] = [f['filename'] for page in cached(f'{n}-files', lambda: pages(f'repos/{repo}/pulls/{n}/files?per_page=100')) for f in page]
        commit = cached(f'{n}-commit', lambda: gh(f"repos/{repo}/commits/{pr['mergeCommit']['oid']}"))
        parent = commit['parents'][0]['sha']
        head = pr['headRefOid']
        pr['behind'] = cached(f'{n}-compare', lambda: gh(f'repos/{repo}/compare/{parent}...{head}'))['behind_by'] > 0
        statuses = [s for page in cached(f'{n}-statuses', lambda: pages(f'repos/{repo}/commits/{head}/statuses?per_page=100')) for s in page]
        at_merge = [s for s in statuses if s['context'] == 'gate/review' and s['created_at'] <= pr['mergedAt']]
        pr['review_pass'] = bool(at_merge and at_merge[0]['state'] == 'success')
        # Use current policy against historical commits. Snapshot coverage did not exist then.
        pr['risk'] = classify(cfg, parent, pr['mergeCommit']['oid'], author=pr['author']['login'])
        return pr
    # Fetch merge commits before classifying. GitHub's public PR refs keep closed heads reachable.
    run('git', 'fetch', 'origin', cfg['base'].split('/')[-1], '--quiet')
    missing = [p['mergeCommit']['oid'] for p in prs if subprocess.run(['git', 'cat-file', '-e', p['mergeCommit']['oid']], capture_output=True).returncode]
    if missing:
        run('git', 'fetch', 'origin', *missing, '--quiet')
    with ThreadPoolExecutor(max_workers=6) as pool:
        details = list(pool.map(detail, prs))
    selected = [p for p in details if datetime.fromisoformat(p['mergedAt'].replace('Z', '+00:00')) < end]
    commits = []
    for line in git('log', cfg['base'], '--first-parent', f'--since={since}', f'--until={(end + timedelta(days=14)).isoformat()}', '--format=%H%x09%cI%x09%s').splitlines():
        sha, date, title = line.split('\t', 2)
        if re.search(r'\b(fix|regression|hotfix|revert)\b', title, re.I):
            commits.append({'sha': sha, 'date': date, 'title': title, 'files': git('diff-tree', '--no-commit-id', '--name-only', '-r', sha).splitlines()})
    followups = {}
    fix_pattern = re.compile(r'\b(fix|regression|hotfix|revert)\b', re.I)
    for pr in selected:
        date = datetime.fromisoformat(pr['mergedAt'].replace('Z', '+00:00'))
        def related(title, body, files, time):
            later = datetime.fromisoformat(time.replace('Z', '+00:00'))
            return date < later <= date + timedelta(days=14) and (bool(set(files) & set(pr['files'])) or re.search(rf'(?<!\d)#{pr["number"]}(?!\d)', title + ' ' + body))
        fixes = [f"PR #{p['number']}" for p in details if fix_pattern.search(p['title']) and related(p['title'], p['body'], p['files'], p['mergedAt'])]
        fixes += [c['sha'] for c in commits if related(c['title'], '', c['files'], c['date']) and c['sha'] not in {p['mergeCommit']['oid'] for p in details}]
        if fixes:
            followups[str(pr['number'])] = fixes
    runs = [r for page in pages(f'repos/{repo}/actions/runs?branch={cfg["base"].split("/")[-1]}&event=push&status=failure&per_page=100&created={since}..{until}') for r in page['workflow_runs'] if r['event'] == 'push' and r['head_repository']['full_name'] == repo and (not cfg.get('ci_workflows') or r['path'].split('@')[0] in cfg['ci_workflows'])]
    n = len(selected)
    stop = [p for p in selected if p['risk']['tier'] == 'stop']
    per_rule = {}
    for pr in stop:
        for rule in {r['rule'] for r in pr['risk']['reasons'] if r['tier'] == 'stop'}:
            per_rule[rule] = per_rule.get(rule, 0) + 1
    return {'since': since, 'until': until, 'merged_prs': n, 'followup_fixes': {'count': len(followups), 'rate': len(followups) / n if n else 0, 'prs': followups}, 'failed_main_ci_runs': len(runs), 'behind_main_at_merge': sum(p['behind'] for p in selected), 'without_passing_review': [p['number'] for p in selected if not p['review_pass']], 'stop_rate': len(stop) / n if n else 0, 'stop_per_rule': {k: {'count': v, 'rate': v / n if n else 0} for k, v in sorted(per_rule.items())}, 'prs': [{'number': p['number'], 'merged_at': p['mergedAt'], 'risk': p['risk']} for p in selected], 'limitations': ['Follow-up fixes are a title/file heuristic, not a causal claim.', 'Recent PRs have an incomplete 14-day observation window.', 'Historical CLI and MCP snapshots do not exist; historical contract stops are a lower bound.', 'A maximum of 1000 merged PRs is supported; collection fails at that limit.']}


def metrics(args, cfg):
    report = collect(cfg, args.since, args.until)
    if args.last:
        if len(report['prs']) < args.last:
            raise ValueError('Not enough PRs for --last; widen --since')
        # Backtest uses the same classifier; select the most recent merge dates, not PR numbers.
        ordered = sorted(report['prs'], key=lambda p: p['merged_at'], reverse=True)[:args.last]
        counts = {}
        for pr in ordered:
            for rule in {r['rule'] for r in pr['risk']['reasons'] if r['tier'] == 'stop'}:
                counts[rule] = counts.get(rule, 0) + 1
        stop_ids = {str(p['number']) for p in ordered if p['risk']['tier'] == 'stop'}
        followups = set(report['followup_fixes']['prs'])
        report['backtest'] = {'stop_with_followups': len(stop_ids & followups), 'non_stop_with_followups': sum(str(p['number']) in followups for p in ordered if p['risk']['tier'] != 'stop'), 'count': len(ordered), 'stop_rate': sum(p['risk']['tier'] == 'stop' for p in ordered) / len(ordered) if ordered else 0, 'per_rule': {k: {'count': v, 'rate': v / len(ordered)} for k, v in counts.items()}}
    print(json.dumps(report, indent=2))
