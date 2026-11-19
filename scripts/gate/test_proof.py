import argparse
import json
from pathlib import Path
import unittest
from unittest.mock import patch

from git_fixture import GitRepoFixture
import proof


class ProofTests(GitRepoFixture, unittest.TestCase):
    def test_review_fails_unresolved_high_and_refuses_same_model(self):
        path = Path('findings.json')
        sha = self.git('rev-parse', 'HEAD')
        path.write_text(json.dumps({'head': sha, 'findings': [{'severity': 'high', 'resolved': False}]}))
        args = argparse.Namespace(findings=str(path), verdict='pass', reviewer='review-model')
        proof.state_path().write_text(json.dumps({'author_model': 'author-model'}))
        with patch.object(proof, 'gh') as api:
            proof.review(args, {'repository': 'example/repo'})
            self.assertEqual(api.call_args.args[2]['state'], 'failure')
            self.assertEqual(api.call_args.args[2]['description'], 'fail: review-model, 1 open')
        args.reviewer = 'AUTHOR-MODEL'
        with self.assertRaisesRegex(ValueError, 'differ'):
            proof.review(args, {'repository': 'example/repo'})
        args.reviewer = 'other'
        path.write_text(json.dumps({'head': 'stale', 'findings': []}))
        with self.assertRaisesRegex(ValueError, 'head SHA'):
            proof.review(args, {'repository': 'example/repo'})
