"""Read approval from trusted workflow runs, newest status first."""
import json
import os
import re
import sys


def is_approved(statuses, repository, head_sha, lookup_run, default_branch='main',
                base_changed_at=None, server_url='https://github.com'):
    run_url = re.compile(re.escape(f'{server_url}/{repository}/actions/runs/') + r'([0-9]+)')
    for status in statuses:
        if (status.get('context') != 'gate/approved'
                or (status.get('creator') or {}).get('login') != 'github-actions[bot]'):
            continue
        match = run_url.fullmatch(status.get('target_url') or '')
        if not match:
            continue
        run = lookup_run(match[1])
        pulls = run.get('pull_requests') or []
        if (run.get('path', '').split('@', 1)[0] == '.github/workflows/gate-approve.yml'
                and run.get('event') == 'pull_request_target'
                and run.get('display_title') == f'gate:approved {head_sha}'
                and pulls
                and all((pr.get('base') or {}).get('ref') == default_branch for pr in pulls)
                and (base_changed_at is None or (run.get('created_at') or '') > base_changed_at)):
            return status.get('state') == 'success'
    return False


def last_base_change(timeline):
    return max((event['created_at'] for event in timeline
                if event.get('event') == 'base_ref_changed'), default=None)


if __name__ == '__main__':
    from gate import gh, pages
    repository = os.environ['GITHUB_REPOSITORY']
    statuses = json.load(sys.stdin)
    timeline = [event for page in pages(
        f'repos/{repository}/issues/{os.environ["PR_NUMBER"]}/timeline?per_page=100') for event in page]
    approved = is_approved(
        statuses, repository, os.environ['HEAD_SHA'],
        lambda run_id: gh(f'repos/{repository}/actions/runs/{run_id}'),
        os.environ['DEFAULT_BRANCH'], last_base_change(timeline),
        os.environ.get('GITHUB_SERVER_URL', 'https://github.com'),
    )
    print('true' if approved else 'false')
