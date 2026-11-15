"""Read approval from commit statuses in API order, newest first."""
import json
import sys


def is_approved(statuses):
    for status in statuses:
        if (status.get('context') == 'gate/approved'
                and (status.get('creator') or {}).get('login') == 'github-actions[bot]'):
            return status.get('state') == 'success'
    return False


if __name__ == '__main__':
    print('true' if is_approved(json.load(sys.stdin)) else 'false')
