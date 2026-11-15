"""Read approval from trusted workflow runs, newest status first."""
import json
import os
import re
import sys


def is_approved(statuses, repository, head_sha, lookup_run, server_url='https://github.com'):
    run_url = re.compile(re.escape(f'{server_url}/{repository}/actions/runs/') + r'([0-9]+)')
    for status in statuses:
        if (status.get('context') != 'gate/approved'
                or (status.get('creator') or {}).get('login') != 'github-actions[bot]'):
            continue
        match = run_url.fullmatch(status.get('target_url') or '')
        if not match:
            continue
        run = lookup_run(match[1])
        if (run.get('path', '').split('@', 1)[0] == '.github/workflows/gate-approve.yml'
                and run.get('event') == 'pull_request_target'
                and run.get('display_title') == f'gate:approved {head_sha}'):
            return status.get('state') == 'success'
    return False


if __name__ == '__main__':
    from gate import gh
    repository = os.environ['GITHUB_REPOSITORY']
    approved = is_approved(
        json.load(sys.stdin), repository, os.environ['HEAD_SHA'],
        lambda run_id: gh(f'repos/{repository}/actions/runs/{run_id}'),
        os.environ.get('GITHUB_SERVER_URL', 'https://github.com'),
    )
    print('true' if approved else 'false')
