"""Check fixture repeatability and output-only answer extraction."""

import contextlib
import io
import json
import os
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

import answer_contracts as contracts
import fixture
from bench import score


class AnswerContractsTest(unittest.TestCase):
    def test_published_truth_and_scaled_traps(self):
        now = datetime(2026, 11, 14, 12, tzinfo=timezone.utc)
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "fixture"
            old = fixture.build(out, now)
            self.assertEqual([old[k] for k in ("q1", "q2", "q4")], [36, 22, 6])
            self.assertEqual([r["uses"] for r in old["q3"]], [20, 15, 12, 10, 8])
            for task in fixture.TASKS:
                first = fixture.build(out, now, task, 40, 7)
                again = fixture.build(Path(temp) / "other", now, task, 40, 7)
                self.assertEqual(first, again)
                self.assertTrue(set(old["traps"]) <= set(first["traps"]))
                self.assertEqual(first["q1"], 40)
                self.assertFalse(any("truth" in p.name for p in out.rglob("*")))

    def test_inherited_paths_cannot_override_fixture(self):
        with patch.dict(os.environ, {"SKILL_STUDIO_FIXTURE": "/outside", "SKILL_STUDIO_PROJECT": "/outside",
                                     "CODEX_HOME": "/outside", "OPENCODE_DB": "/outside"}):
            env = contracts.environment(Path("/fixture/home"))
        for key in ("SKILL_STUDIO_FIXTURE", "SKILL_STUDIO_PROJECT", "OPENCODE_DB"):
            self.assertNotIn(key, env)
        self.assertEqual(env["CODEX_HOME"], "/fixture/home/.codex")

    def test_baseline_regression_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for binary in ("skill-studio", "skill-studio-mcp"):
                path = root / "target/debug" / binary
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            baseline = root / "baseline.json"
            baseline.write_text(json.dumps(["usage/q1/40/7/cli"]))
            args = SimpleNamespace(size=[40], seed=7, json_out=root / "result.json", check=True, update_baseline=False)
            truth = {"q1": 40, "q2": 1, "q3": [{"skill": "invented", "uses": 1}], "q4": 0}
            with patch.object(contracts, "BASELINE", baseline), patch.object(fixture, "build", return_value=truth), \
                 patch.object(contracts.subprocess, "run", return_value=SimpleNamespace(stdout=b'{"skills":[]}', returncode=0)), \
                 patch.object(contracts, "mcp_call", return_value=b'{"content":[{"type":"text","text":"{\\\"skills\\\":[]}"}]}'), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(contracts.run(args, score, {"usage": ("q1",)}, root), 1)

    def test_extractors_use_returned_fields(self):
        home = Path("/fixture/home")
        data = {"rows": [{"skill": "invented", "recent_uses": 8},
                         {"skill": "other", "recent_uses": 0}]}
        self.assertEqual(contracts.extract("q2", data, home), 1)
        self.assertEqual(contracts.extract("q3", data, home)[0], {"skill": "invented", "uses": 8})
        scan = {"skills": [{"name": "invented", "deployments": [{"path": str(home / ".agents/skills/invented")}]}]}
        self.assertEqual(contracts.extract("q1", scan, home), 1)
        for question in ("u1", "u2", "s1", "s2"):
            self.assertIsNone(contracts.extract(question, scan, home))
        truth = {"q1": 1, "q2": 1, "q3": [{"skill": "invented", "uses": 8}], "q4": 0}
        self.assertTrue(score("usage", {"q3": contracts.extract("q3", data, home)[:1]}, truth)["q3"])


if __name__ == "__main__":
    unittest.main()
