#!/usr/bin/env python3
# ============================================================================
# Skill Studio - agent benchmark fixture
# Writes a synthetic home folder (skill library + session logs of five agent
# harnesses) and the ground truth for the benchmark questions.
# ============================================================================
"""Generate the benchmark fixture.

Every skill name, project and session in the fixture is synthetic. The logs
use each harness's real record format (see docs/agent-skill-conventions.md,
"Skill uses"), so the same files can be read by hand, by the Skill Studio CLI,
and by the Skill Studio MCP server.

Usage: python3 tools/agent-bench/fixture.py --out <dir> [--now 2026-11-14T12:00:00Z]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import sqlite3
import stat
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path

FIXTURE_VERSION = 1
SEED = 20261114
MAC_HOME = "/Users/dev"
LIB = f"{MAC_HOME}/.agents/skills"
WINDOW_DAYS = 45
SKILL_STUDIO_STATE = ".skill-studio"

LIBRARY = [
    "accessibility-pass", "api-contract-check", "bundle-size", "cache-tuning",
    "commit-message", "copy-edit", "cost-report", "cron-audit", "css-tokens",
    "data-export", "dependency-audit", "design-review", "docs-sync", "e2e-smoke",
    "env-check", "error-copy", "feature-flag-cleanup", "flaky-test-hunter",
    "form-validation", "i18n-strings", "image-optim", "incident-timeline",
    "license-scan", "log-scrubber", "oncall-handoff", "onboarding-guide",
    "perf-budget", "pr-summary", "queue-retry", "release-notes", "rollback-plan",
    "schema-migrate", "security-headers", "sql-review", "storybook-stories",
    "test-triage",
]

# disable-model-invocation: true in SKILL.md (manual-only in Claude Code).
MANUAL_IN_CLAUDE = {
    "release-notes", "schema-migrate", "rollback-plan", "oncall-handoff",
    "cost-report", "data-export", "license-scan", "design-review",
    "incident-timeline",
}
# agents/openai.yaml policy.allow_implicit_invocation for Codex.
CODEX_IMPLICIT = {
    "schema-migrate": False, "rollback-plan": False, "data-export": False,
    "cost-report": True, "env-check": False,
}

# Uses inside the last 45 days, per library skill.
IN_WINDOW = {
    "sql-review": 19, "pr-summary": 15, "test-triage": 12, "commit-message": 10,
    "api-contract-check": 8, "docs-sync": 6, "flaky-test-hunter": 5,
    "release-notes": 5, "dependency-audit": 4, "e2e-smoke": 4, "error-copy": 3,
    "perf-budget": 3, "schema-migrate": 2, "i18n-strings": 2, "design-review": 2,
    "accessibility-pass": 2, "bundle-size": 1, "cost-report": 1, "env-check": 1,
    "oncall-handoff": 1, "cron-audit": 1, "copy-edit": 1,
}
# Uses older than the window (day offsets).
BEFORE_WINDOW = {
    "queue-retry": [46], "cache-tuning": [52, 58], "form-validation": [61],
    "image-optim": [75], "sql-review": [47, 63, 88], "docs-sync": [49],
    "pr-summary": [70],
}
# A use placed on the window edge, so the date filter matters.
EDGE_OFFSET = {"cron-audit": 44}

HARNESS_WEIGHTS = [("claude", 40), ("codex", 30), ("opencode", 10), ("pi", 10), ("cursor", 10)]
TRIGGERS = {
    "claude": [("agent", 5), ("user", 3), ("read", 2)],
    "codex": [("user", 5), ("read", 4), ("agent", 1)],
    "opencode": [("agent", 5), ("user", 3), ("read", 2)],
    "pi": [("user", 5), ("read", 5)],
    "cursor": [("read", 1)],
}
PROJECTS = ["acme-web", "acme-api", "billing-service", "infra", "mobile-app"]

FILLER_PROMPTS = [
    "Fix the failing test in the checkout flow.",
    "Why is the build slow on CI?",
    "Add a loading state to the settings page.",
    "Rename the user service to account service.",
    "Look at the last migration and tell me if it is safe.",
    "Write a short summary of this branch.",
    "Update the README for the new env vars.",
    "Find where we parse the config file.",
]
FILLER_FILES = ["src/app.ts", "src/config.ts", "package.json", "README.md", "src/db/schema.sql"]


@dataclass
class Use:
    skill: str
    harness: str
    trigger: str
    offset: int
    counted: bool = True
    note: str = ""


@dataclass
class Session:
    harness: str
    sid: str
    project: str
    offset: int
    uses: list[Use] = field(default_factory=list)
    extras: list[str] = field(default_factory=list)


class Clock:
    def __init__(self, now: datetime, rng: random.Random):
        self.now = now
        self.rng = rng

    def at(self, offset_days: int, minute: int = 0) -> datetime:
        base = self.now - timedelta(days=offset_days)
        hours_back = 2 if offset_days == 0 else 1 + (offset_days * 7) % 9
        return base - timedelta(hours=hours_back) + timedelta(minutes=minute)


def iso(t: datetime) -> str:
    return t.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


def jsonl(records) -> str:
    return "".join(json.dumps(r, separators=(",", ":"), ensure_ascii=False) + "\n" for r in records)


def ms(t: datetime) -> int:
    return int(t.timestamp() * 1000)


def pick(rng: random.Random, weighted):
    total = sum(w for _, w in weighted)
    r = rng.uniform(0, total)
    for value, weight in weighted:
        r -= weight
        if r <= 0:
            return value
    return weighted[-1][0]


def uid(rng: random.Random) -> str:
    return str(uuid.UUID(int=rng.getrandbits(128), version=4))


def skill_md_path(skill: str) -> str:
    return f"{LIB}/{skill}/SKILL.md"


# ----------------------------------------------------------------------------
# Plan: which uses happen, grouped into sessions
# ----------------------------------------------------------------------------


def plan_uses(rng: random.Random) -> list[Use]:
    uses: list[Use] = []
    for skill, count in IN_WINDOW.items():
        for i in range(count):
            offset = EDGE_OFFSET.get(skill) if i == 0 and skill in EDGE_OFFSET else rng.randint(0, 43)
            harness = pick(rng, HARNESS_WEIGHTS)
            uses.append(Use(skill, harness, pick(rng, TRIGGERS[harness]), offset))
    for skill, offsets in BEFORE_WINDOW.items():
        for offset in offsets:
            harness = pick(rng, HARNESS_WEIGHTS)
            uses.append(Use(skill, harness, pick(rng, TRIGGERS[harness]), offset, note="before window"))
    # The library renamed changelog-writer to release-notes 20 days ago.
    for offset in (21, 24, 27, 33, 38):
        harness = pick(rng, HARNESS_WEIGHTS[:2])
        uses.append(Use("changelog-writer", harness, pick(rng, TRIGGERS[harness]), offset, note="old name, not in library"))
    # Skills outside the shared library.
    for skill, harness, offset in [
        ("local-scratch", "claude", 3), ("local-scratch", "claude", 9),
        ("web-deploy", "claude", 5), ("web-deploy", "codex", 12),
        ("old-linter", "codex", 15), ("old-linter", "pi", 18),
    ]:
        uses.append(Use(skill, harness, pick(rng, TRIGGERS[harness]), offset, note="not a library skill"))
    rng.shuffle(uses)
    return uses


def group_sessions(rng: random.Random, uses: list[Use]) -> list[Session]:
    sessions: list[Session] = []
    open_by_key: dict[tuple[str, int], Session] = {}
    for use in uses:
        key = (use.harness, use.offset)
        session = open_by_key.get(key)
        same_skill = session is not None and any(u.skill == use.skill for u in session.uses)
        if session is None or same_skill or len(session.uses) >= 3 or rng.random() < 0.45:
            session = Session(use.harness, uid(rng), rng.choice(PROJECTS), use.offset)
            sessions.append(session)
            open_by_key[key] = session
        session.uses.append(use)
    return sessions


# ----------------------------------------------------------------------------
# Skill library on disk
# ----------------------------------------------------------------------------


def skill_md(name: str, manual: bool, rng: random.Random) -> str:
    words = name.replace("-", " ")
    lines = ["---", f"name: {name}", f"description: Use when the task involves {words}. Gives a checklist and the house style for {words}."]
    if manual:
        lines.append("disable-model-invocation: true")
    elif rng.random() < 0.15:
        lines.append("disable-model-invocation: false")
    lines += ["---", "", f"# {words.title()}", "", "1. Read the request.", f"2. Apply the {words} checklist below.", "3. Report what you changed.", ""]
    return "\n".join(lines)


def openai_yaml(name: str, implicit: bool) -> str:
    return (
        "interface:\n"
        f'  display_name: "{name.replace("-", " ").title()}"\n'
        "policy:\n"
        f"  allow_implicit_invocation: {'true' if implicit else 'false'}\n"
    )


def write_library(home: Path, rng: random.Random, library=LIBRARY, manual=MANUAL_IN_CLAUDE, implicit=CODEX_IMPLICIT) -> None:
    lib = home / ".agents" / "skills"
    for name in library:
        folder = lib / name
        folder.mkdir(parents=True)
        (folder / "SKILL.md").write_text(skill_md(name, name in manual, rng))
        if name in implicit:
            (folder / "agents").mkdir()
            (folder / "agents" / "openai.yaml").write_text(openai_yaml(name, implicit[name]))
        if rng.random() < 0.3:
            (folder / "references").mkdir()
            (folder / "references" / "examples.md").write_text(f"# Examples for {name}\n")
    # Folders and files in the library that are not skills.
    (lib / "README.md").write_text("# Shared skills\nSynced from the team repo.\n")
    (lib / ".DS_Store").write_bytes(b"\x00\x00\x00\x01Bud1")
    (lib / "synced").mkdir()
    (lib / "synced" / "manifest.json").write_text('{"source": "team-skills", "count": 36}\n')
    (lib / "notes").mkdir()
    (lib / "notes" / "ideas.md").write_text("- a skill for release trains\n")
    for trashed in ("old-linter", "release-notes"):
        (lib / ".trash" / trashed).mkdir(parents=True)
        (lib / ".trash" / trashed / "SKILL.md").write_text(skill_md(trashed, trashed == "release-notes", rng))
    (lib / "_drafts" / "draft-skill").mkdir(parents=True)
    (lib / "_drafts" / "draft-skill" / "SKILL.md").write_text(skill_md("draft-skill", False, rng))
    # Claude Code does not read ~/.agents/skills; `npx skills` links each skill in.
    claude_skills = home / ".claude" / "skills"
    claude_skills.mkdir(parents=True)
    for name in library:
        (claude_skills / name).symlink_to(Path("..", "..", ".agents", "skills", name))
    # Claude Code only skills, outside the shared library.
    for name, manual in (("local-scratch", False), ("claude-only-review", True)):
        (home / ".claude" / "skills" / name).mkdir(parents=True)
        (home / ".claude" / "skills" / name / "SKILL.md").write_text(skill_md(name, manual, rng))


# ----------------------------------------------------------------------------
# Use-case overlays. Only the `update` and `share` tasks get them, so the
# `usage` task reads the same home folder as before.
# ----------------------------------------------------------------------------

TASKS = ("usage", "update", "share")
PACKS = {
    "acme-labs/review-skills": [
        "sql-review", "pr-summary", "api-contract-check", "commit-message",
        "test-triage", "design-review", "docs-sync", "security-headers",
    ],
    "orbit-dev/ops-skills": [
        "cron-audit", "incident-timeline", "oncall-handoff", "rollback-plan", "queue-retry",
        "cost-report", "log-scrubber", "env-check", "schema-migrate", "cache-tuning",
    ],
    "northwind/frontend-skills": [
        "accessibility-pass", "bundle-size", "css-tokens", "form-validation", "i18n-strings",
        "image-optim", "perf-budget", "storybook-stories", "e2e-smoke", "error-copy",
    ],
}
UPDATED_PACK = "acme-labs/review-skills"
# Still in the lock file, but its folder is only in .trash.
REMOVED_FROM_PACK = "old-linter"
# Real copies the user made for one agent, before the pack update: (agent, folder, skill, matches the new version).
AGENT_COPIES = [
    ("codex", ".codex/skills", "sql-review", False),
    ("codex", ".codex/skills", "pr-summary", True),
    ("codex", ".codex/skills", "flaky-test-hunter", False),
    ("opencode", ".config/opencode/skills", "test-triage", False),
    ("cursor", ".cursor/skills", "sql-review", False),
    ("cursor", ".cursor/skills", "docs-sync", False),
]
# Links follow the library, so they always have the new version.
AGENT_LINKS = [("pi", ".pi/agent/skills", "commit-message")]
# Hand-made skills whose text names a pack, which does not make them installed from it.
ADAPTED_FROM = {"flaky-test-hunter": "northwind/frontend-skills", "release-notes": "acme-labs/review-skills"}


def pack_of(skill: str) -> str | None:
    return next((repo for repo, skills in PACKS.items() if skill in skills), None)


def new_version(text: str) -> str:
    return text + "\n## Changes in 2.0\n\n- Check the query plan before you approve an index.\n"


def write_lock(home: Path, now: datetime) -> None:
    skills = {}
    for repo, names in PACKS.items():
        for name in [*names, *([REMOVED_FROM_PACK] if repo == UPDATED_PACK else [])]:
            updated = now - timedelta(days=1 if repo == UPDATED_PACK else 40)
            skills[name] = {
                "source": repo,
                "sourceType": "github",
                "sourceUrl": f"https://github.com/{repo}.git",
                "skillPath": f"skills/{name}/SKILL.md",
                "skillFolderHash": hashlib.sha1(f"{repo}/{name}/{updated.date()}".encode()).hexdigest(),
                "installedAt": iso(now - timedelta(days=90)),
                "updatedAt": iso(updated),
            }
    lock = {"version": 3, "skills": dict(sorted(skills.items()))}
    (home / ".agents" / ".skill-lock.json").write_text(json.dumps(lock, indent=2) + "\n")


def write_overlay(home: Path, task: str, now: datetime) -> dict:
    lib = home / ".agents" / "skills"
    write_lock(home, now)
    for name in PACKS[UPDATED_PACK]:
        old = (lib / name / "SKILL.md").read_text()
        for _agent, folder, skill, current in AGENT_COPIES:
            if skill == name:
                copy = home / folder / skill
                copy.mkdir(parents=True, exist_ok=True)
                (copy / "SKILL.md").write_text(new_version(old) if current else old)
        (lib / name / "SKILL.md").write_text(new_version(old))
    for _agent, folder, skill, current in AGENT_COPIES:
        if pack_of(skill) != UPDATED_PACK:
            copy = home / folder / skill
            copy.mkdir(parents=True, exist_ok=True)
            text = (lib / skill / "SKILL.md").read_text()
            (copy / "SKILL.md").write_text(text if current else text.replace("1. Read the request.", "1. Read the request and the failing test log."))
    for _agent, folder, skill in AGENT_LINKS:
        (home / folder).mkdir(parents=True, exist_ok=True)
        (home / folder / skill).symlink_to(lib / skill)
    for skill, repo in ADAPTED_FROM.items():
        path = lib / skill / "SKILL.md"
        path.write_text(path.read_text() + f"\nAdapted from the `{repo}` pack, with our own steps.\n")
    if task == "update":
        stale = sorted(
            ({"skill": skill, "agent": agent} for agent, _f, skill, current in AGENT_COPIES if pack_of(skill) == UPDATED_PACK and not current),
            key=lambda r: (r["skill"], r["agent"]),
        )
        return {"pack": UPDATED_PACK, "u1": len(PACKS[UPDATED_PACK]), "u2": stale}
    return {
        "s1": {repo: len(names) for repo, names in PACKS.items()},
        "s2": sorted(s for s in LIBRARY if pack_of(s) is None),
    }


# ----------------------------------------------------------------------------
# Claude Code: ~/.claude/projects/<encoded cwd>/<session>.jsonl
# ----------------------------------------------------------------------------


class ClaudeWriter:
    def __init__(self, session: Session, clock: Clock, rng: random.Random):
        self.s = session
        self.clock = clock
        self.rng = rng
        self.cwd = f"{MAC_HOME}/src/{session.project}"
        self.records: list[dict] = []
        self.minute = 0
        self.parent: str | None = None

    def rec(self, kind: str, message: dict, **extra) -> dict:
        self.minute += 1
        u = uid(self.rng)
        r = {
            "parentUuid": self.parent, "isSidechain": False, "userType": "external",
            "cwd": self.cwd, "sessionId": self.s.sid, "version": "2.1.300",
            "gitBranch": "main", "type": kind, "message": message, "uuid": u,
            "timestamp": iso(self.clock.at(self.s.offset, self.minute)),
        }
        r.update(extra)
        self.parent = u
        self.records.append(r)
        return r

    def user_text(self, text: str, **extra) -> dict:
        return self.rec("user", {"role": "user", "content": text}, **extra)

    def tool(self, name: str, tool_input: dict, result: str) -> None:
        tid = "toolu_" + uid(self.rng).replace("-", "")[:24]
        self.rec("assistant", {"role": "assistant", "model": "claude-sonnet-5-5", "content": [{"type": "tool_use", "id": tid, "name": name, "input": tool_input}]})
        self.rec("user", {"role": "user", "content": [{"type": "tool_result", "tool_use_id": tid, "content": result}]})

    def say(self, text: str) -> None:
        self.rec("assistant", {"role": "assistant", "model": "claude-sonnet-5-5", "content": [{"type": "text", "text": text}]})

    def skill_body(self, skill: str) -> None:
        self.user_text(f"Base directory for this skill: {LIB}/{skill}\n\n# {skill}\n1. Read the request.", isMeta=True)

    def use(self, use: Use) -> None:
        if use.trigger == "agent":
            self.tool("Skill", {"skill": use.skill}, f"Launching skill: {use.skill}")
            self.skill_body(use.skill)
        elif use.trigger == "user":
            self.user_text(f"<command-message>{use.skill} is running…</command-message>\n<command-name>/{use.skill}</command-name>")
            self.skill_body(use.skill)
        elif self.rng.random() < 0.5:
            self.tool("Read", {"file_path": skill_md_path(use.skill)}, f"---\nname: {use.skill}\n---")
        else:
            self.tool("Bash", {"command": f"cat {skill_md_path(use.skill)}", "description": "Read the skill"}, f"---\nname: {use.skill}\n---")

    def filler(self) -> None:
        self.user_text(self.rng.choice(FILLER_PROMPTS))
        self.tool("Read", {"file_path": f"{self.cwd}/{self.rng.choice(FILLER_FILES)}"}, "...")
        self.tool("Bash", {"command": "git status --short", "description": "Show changes"}, " M src/app.ts")
        self.say("Done. I changed one file.")


def write_claude(home: Path, sessions: list[Session], clock: Clock, rng: random.Random) -> None:
    for s in sessions:
        w = ClaudeWriter(s, clock, rng)
        w.user_text(
            "<system-reminder>\nThe following skills are available for use with the Skill tool:\n"
            + "\n".join(f"- {n}: Use when the task involves {n.replace('-', ' ')}." for n in rng.sample(LIBRARY, 8))
            + "\n</system-reminder>",
            isMeta=True,
        )
        w.filler()
        for use in s.uses:
            if use.trigger == "subagent":
                continue
            w.use(use)
            w.say(f"Applied {use.skill}.")
        if "read-after-invoke" in s.extras and s.uses:
            w.tool("Read", {"file_path": skill_md_path(s.uses[0].skill)}, "---")
        if "repeat-read" in s.extras and s.uses:
            w.tool("Bash", {"command": f"head -40 {skill_md_path(s.uses[0].skill)}"}, "---")
        if "skill-list-text" in s.extras:
            w.user_text("Which is better for this, sql-review or api-contract-check? Do not run either yet.")
        if "edit-skill" in s.extras:
            w.tool("Edit", {"file_path": skill_md_path("docs-sync"), "old_string": "1.", "new_string": "1)"}, "ok")
            w.tool("Bash", {"command": f"echo '- new rule' >> {skill_md_path('docs-sync')}"}, "")
        records = list(w.records)
        if "resume" in s.extras:
            # A resumed session repeats its earlier records (same uuid and timestamp).
            records = records + records[1 : len(records)] + [w.records[-1]]
        folder = home / ".claude" / "projects" / ("-" + w.cwd.strip("/").replace("/", "-"))
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"{s.sid}.jsonl").write_text(jsonl(records))
        if "subagent" in s.extras:
            sub = folder / s.sid / "subagents"
            sub.mkdir(parents=True, exist_ok=True)
            sw = ClaudeWriter(s, clock, rng)
            sw.minute = w.minute
            sw.user_text("Review the diff for SQL problems.", isSidechain=True)
            sw.tool("Skill", {"skill": "sql-review"}, "Launching skill: sql-review")
            sw.say("No SQL problems found.")
            for r in sw.records:
                r["isSidechain"] = True
                r["agentId"] = "a1b2c3"
            (sub / "agent-a1b2c3.jsonl").write_text(jsonl(sw.records))


# ----------------------------------------------------------------------------
# Codex: ~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl
# ----------------------------------------------------------------------------


class CodexWriter:
    def __init__(self, session: Session, clock: Clock, rng: random.Random, start_minute: int = 0):
        self.s = session
        self.clock = clock
        self.rng = rng
        self.cwd = f"{MAC_HOME}/src/{session.project}"
        self.lines: list[dict] = []
        self.minute = start_minute

    def ts(self) -> str:
        self.minute += 1
        return iso(self.clock.at(self.s.offset, self.minute))

    def item(self, payload: dict) -> None:
        self.lines.append({"timestamp": self.ts(), "type": "response_item", "payload": payload})

    def meta(self, sid: str, extra: dict | None = None) -> None:
        payload = {"id": sid, "timestamp": self.ts(), "cwd": self.cwd, "originator": "codex_cli_rs", "cli_version": "0.160.0", "source": "cli", "model_provider": "openai"}
        payload.update(extra or {})
        self.lines.append({"timestamp": self.ts(), "type": "session_meta", "payload": payload})

    def message(self, role: str, text: str) -> None:
        kind = "output_text" if role == "assistant" else "input_text"
        self.item({"type": "message", "role": role, "content": [{"type": kind, "text": text}]})

    def exec(self, cmd: str) -> None:
        call = "call_" + uid(self.rng).replace("-", "")[:20]
        self.item({"type": "custom_tool_call", "status": "completed", "call_id": call, "name": "exec", "input": f"const r = await tools.exec_command({json.dumps({'cmd': cmd})});\ntext(r.output);"})
        self.item({"type": "custom_tool_call_output", "call_id": call, "output": "..."})

    def use(self, use: Use) -> None:
        if use.trigger == "user":
            self.message("user", f"<skill>\n<name>{use.skill}</name>\n<path>{skill_md_path(use.skill)}</path>\n# {use.skill}\n1. Read the request.\n</skill>")
        elif use.trigger == "agent":
            call = "call_" + uid(self.rng).replace("-", "")[:20]
            self.item({"type": "function_call", "namespace": "skills", "name": "read", "arguments": json.dumps({"package": skill_md_path(use.skill)}, separators=(",", ":")), "call_id": call})
            self.item({"type": "function_call_output", "call_id": call, "output": f"# {use.skill}"})
        else:
            self.exec(self.rng.choice([f"sed -n '1,200p' {skill_md_path(use.skill)}", f"cat {skill_md_path(use.skill)}"]))

    def start(self) -> None:
        self.meta(self.s.sid)
        self.lines.append({"timestamp": self.ts(), "type": "turn_context", "payload": {"cwd": self.cwd, "model": "gpt-6.1-sol", "approval_policy": "on-request"}})
        skills = "\n".join(f"- {n}: Use when the task involves {n.replace('-', ' ')}. (file: {skill_md_path(n)})" for n in self.rng.sample(LIBRARY, 10))
        self.message("developer", f"## Skills\nThese skills are available in this session:\n{skills}")
        self.message("user", self.rng.choice(FILLER_PROMPTS))
        self.exec("rg --files src | head -50")


def codex_path(home: Path, s: Session, clock: Clock, archived: bool) -> Path:
    t = clock.at(s.offset)
    name = f"rollout-{t.strftime('%Y-%m-%dT%H-%M-%S')}-{s.sid}.jsonl"
    if archived:
        return home / ".codex" / "archived_sessions" / name
    return home / ".codex" / "sessions" / t.strftime("%Y/%m/%d") / name


def write_codex(home: Path, sessions: list[Session], clock: Clock, rng: random.Random) -> None:
    for s in sessions:
        w = CodexWriter(s, clock, rng)
        w.start()
        for use in s.uses:
            w.use(use)
            w.message("assistant", f"Applied {use.skill}.")
        if "repeat-read" in s.extras and s.uses:
            w.exec(f"head -30 {skill_md_path(s.uses[0].skill)}")
        if "quoted-skill-block" in s.extras:
            w.message("user", "Why does the log show a <skill> block with <name>pr-summary</name> in it? Explain, do not run it.")
        path = codex_path(home, s, clock, archived="archived" in s.extras)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(jsonl(w.lines))
        for n in range(s.extras.count("fork")):
            # A sub-agent rollout: its own session_meta, then the parent's
            # session_meta and the parent's history copied with new timestamps.
            child = Session("codex", uid(rng), s.project, s.offset)
            cw = CodexWriter(child, clock, rng, start_minute=w.minute + 5 + n * 10)
            cw.meta(child.sid, {"forked_from_id": s.sid, "parent_thread_id": s.sid, "subagent_history_start_ordinal": len(w.lines), "source": {"subagent": "worker"}})
            for line in w.lines:
                copy = json.loads(json.dumps(line))
                copy["timestamp"] = cw.ts()
                cw.lines.append(copy)
            cw.message("user", "Run the unit tests for the changed package and report failures.")
            cw.exec("pnpm test --filter changed")
            cw.message("assistant", "All tests pass.")
            cpath = codex_path(home, child, clock, archived=False)
            cpath.parent.mkdir(parents=True, exist_ok=True)
            cpath.write_text(jsonl(cw.lines))


# ----------------------------------------------------------------------------
# pi: ~/.pi/agent/sessions/--<encoded cwd>--/<time>_<id>.jsonl
# ----------------------------------------------------------------------------


def write_pi(home: Path, sessions: list[Session], clock: Clock, rng: random.Random) -> None:
    for s in sessions:
        cwd = f"{MAC_HOME}/src/{s.project}"
        minute = 0
        lines = []

        def ts() -> str:
            nonlocal minute
            minute += 1
            return iso(clock.at(s.offset, minute))

        lines.append({"type": "session", "version": 3, "id": s.sid, "timestamp": ts(), "cwd": cwd})
        parent = None

        def msg(role: str, content) -> None:
            nonlocal parent
            mid = uid(rng)[:8]
            lines.append({"type": "message", "id": mid, "parentId": parent, "timestamp": ts(), "message": {"role": role, "content": content}})
            parent = mid

        def call(name: str, args: dict) -> None:
            msg("assistant", [{"type": "toolCall", "id": "call_" + uid(rng)[:8], "name": name, "arguments": args}])
            msg("toolResult", [{"type": "text", "text": "..."}])

        msg("user", [{"type": "text", "text": rng.choice(FILLER_PROMPTS)}])
        call("read", {"path": rng.choice(FILLER_FILES)})
        for use in s.uses:
            if use.trigger == "user":
                msg("user", [{"type": "text", "text": f'<skill name="{use.skill}" location="{skill_md_path(use.skill)}">\n# {use.skill}\n</skill>\n\nUse it on this repo.'}])
            elif rng.random() < 0.6:
                call("read", {"path": skill_md_path(use.skill)})
            else:
                call("bash", {"command": f"cat {skill_md_path(use.skill)}"})
            msg("assistant", [{"type": "text", "text": f"Applied {use.skill}."}])
        if "literal-command" in s.extras:
            msg("user", [{"type": "text", "text": "I typed /skill:cost-report earlier but nothing loaded. Ignore it for now."}])
        stamp = clock.at(s.offset).strftime("%Y-%m-%dT%H-%M-%S-000Z")
        folder = home / ".pi" / "agent" / "sessions" / ("--" + cwd.strip("/").replace("/", "-") + "--")
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"{stamp}_{s.sid}.jsonl").write_text(jsonl(lines))


# ----------------------------------------------------------------------------
# Cursor: ~/.cursor/projects/<name>/agent-transcripts/<session>/<session>.jsonl
# Records have no timestamps; the use time is the file's modified time.
# ----------------------------------------------------------------------------


def write_cursor(home: Path, sessions: list[Session], clock: Clock, rng: random.Random) -> list[tuple[Path, datetime]]:
    mtimes = []
    for s in sessions:
        cwd = f"{MAC_HOME}/src/{s.project}"
        lines = [{"role": "user", "message": {"content": [{"type": "text", "text": rng.choice(FILLER_PROMPTS)}]}}]

        def tool(name: str, tool_input: dict) -> None:
            lines.append({"role": "assistant", "message": {"content": [{"type": "tool_use", "name": name, "input": tool_input}]}})

        tool("Read", {"path": f"{cwd}/{rng.choice(FILLER_FILES)}"})
        for use in s.uses:
            if rng.random() < 0.7:
                tool(rng.choice(["Read", "ReadFile"]), {"path": skill_md_path(use.skill)})
            else:
                tool("Shell", {"command": f"cat {skill_md_path(use.skill)}"})
            lines.append({"role": "assistant", "message": {"content": [{"type": "text", "text": f"Applied {use.skill}."}]}})
        if "repeat-read" in s.extras and s.uses:
            tool("Read", {"path": skill_md_path(s.uses[0].skill)})
        folder = home / ".cursor" / "projects" / cwd.strip("/").replace("/", "-") / "agent-transcripts" / s.sid
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"{s.sid}.jsonl"
        path.write_text(jsonl(lines))
        mtimes.append((path, clock.at(s.offset, 30)))
    return mtimes


# ----------------------------------------------------------------------------
# OpenCode v2: ~/.local/share/opencode/opencode.db
# ----------------------------------------------------------------------------


def write_opencode(home: Path, sessions: list[Session], clock: Clock, rng: random.Random) -> None:
    folder = home / ".local" / "share" / "opencode"
    folder.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(folder / "opencode.db")
    db.executescript(
        """
        CREATE TABLE session_v2 (id TEXT PRIMARY KEY, project_id TEXT, directory TEXT NOT NULL,
          title TEXT NOT NULL, version TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
        CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL,
          time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
        CREATE INDEX session_message_session ON session_message(session_id, time_created);
        """
    )
    for s in sessions:
        sid = "ses_" + s.sid.replace("-", "")[:26]
        cwd = f"{MAC_HOME}/src/{s.project}"
        start = clock.at(s.offset)
        minute = 0

        def row(kind: str, data: dict) -> int:
            nonlocal minute
            minute += 1
            t = ms(start + timedelta(minutes=minute))
            mid = "msg_" + uid(rng).replace("-", "")[:26]
            db.execute("INSERT INTO session_message VALUES (?,?,?,?,?,?)", (mid, sid, kind, t, t, json.dumps(data, separators=(",", ":"))))
            return t

        def tool(name: str, tool_input: dict, status: str = "completed") -> None:
            t = ms(start + timedelta(minutes=minute + 1))
            row("assistant", {"role": "assistant", "modelID": "gpt-6.1-sol", "content": [
                {"type": "tool", "id": "call_" + uid(rng)[:8], "name": name, "time": {"created": t},
                 "state": {"status": status, "input": tool_input, "output": "..." if status == "completed" else None}}]})

        row("user", {"role": "user", "content": [{"type": "text", "text": rng.choice(FILLER_PROMPTS)}]})
        tool("read", {"filePath": f"{cwd}/{rng.choice(FILLER_FILES)}"})
        for use in s.uses:
            if use.trigger == "user":
                row("skill", {"name": use.skill, "source": "user", "content": f"# {use.skill}"})
            elif use.trigger == "agent":
                tool("skill", {"name": use.skill})
            else:
                tool("read", {"filePath": skill_md_path(use.skill)})
            row("assistant", {"role": "assistant", "content": [{"type": "text", "text": f"Applied {use.skill}."}]})
        if "failed-skill-call" in s.extras:
            tool("skill", {"name": "perf-budget"}, status="error")
        if "read-after-invoke" in s.extras and s.uses:
            tool("read", {"filePath": skill_md_path(s.uses[0].skill)})
        t0 = ms(start)
        db.execute("INSERT INTO session_v2 VALUES (?,?,?,?,?,?,?)", (sid, "prj_" + s.project, cwd, "Session", "2.3.0", t0, ms(start + timedelta(minutes=minute))))
    db.commit()
    db.close()


# ----------------------------------------------------------------------------
# Ground truth
# ----------------------------------------------------------------------------


def ground_truth(uses: list[Use], now: datetime, library=LIBRARY, manual=MANUAL_IN_CLAUDE, implicit=CODEX_IMPLICIT) -> dict:
    counts: dict[str, int] = {}
    harnesses: dict[str, set[str]] = {}
    for u in uses:
        if u.counted and u.skill in library and u.offset < WINDOW_DAYS:
            counts[u.skill] = counts.get(u.skill, 0) + 1
            harnesses.setdefault(u.skill, set()).add(u.harness)
    ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    assert ranked[4][1] > ranked[5][1], "top 5 must not tie with the 6th"
    manual_auto = sorted(n for n in manual if implicit.get(n, True))
    start = (now - timedelta(days=WINDOW_DAYS)).date()
    return {
        "fixture_version": FIXTURE_VERSION,
        "today": now.date().isoformat(),
        "window_start": start.isoformat(),
        "q1": len(library),
        "q2": len(counts),
        "q3": [{"skill": k, "uses": v, "agents": sorted(harnesses[k])} for k, v in ranked[:5]],
        "q4": len(manual_auto),
        "q4_skills": manual_auto,
        "uses_in_window": dict(ranked),
    }


def mark_extras(sessions: list[Session], rng: random.Random) -> None:
    by_harness: dict[str, list[Session]] = {}
    for s in sessions:
        by_harness.setdefault(s.harness, []).append(s)

    def with_uses(harness: str, skill: str | None = None) -> list[Session]:
        return [s for s in by_harness.get(harness, []) if s.uses and s.offset < 40 and (skill is None or any(u.skill == skill and u.counted for u in s.uses))]

    claude = with_uses("claude")
    for tag, n in (("resume", 2), ("read-after-invoke", 3), ("repeat-read", 2), ("skill-list-text", 3), ("edit-skill", 1), ("subagent", 1)):
        for s in rng.sample(claude, n):
            s.extras.append(tag)
    for s in sessions:
        if "subagent" in s.extras:
            s.uses.append(Use("sql-review", "claude", "subagent", s.offset, note="sub-agent use"))
    for skill in ("test-triage", "commit-message", "sql-review"):
        options = [s for s in with_uses("codex", skill) if any(u.trigger == "user" and u.skill == skill for u in s.uses)]
        if options:
            rng.choice(options).extras.append("fork")
    codex = with_uses("codex")
    for tag, n in (("repeat-read", 2), ("quoted-skill-block", 2), ("archived", 2)):
        for s in rng.sample(codex, min(n, len(codex))):
            s.extras.append(tag)
    for tag in ("failed-skill-call", "read-after-invoke"):
        rng.choice(with_uses("opencode")).extras.append(tag)
    rng.choice(by_harness["pi"]).extras.append("literal-command")
    rng.choice(with_uses("cursor")).extras.append("repeat-read")


def fixture_hash(home: Path) -> str:
    """SHA-256 over every path, mode, size, mtime and content, except Skill Studio's own state folder."""
    h = hashlib.sha256()
    for root, dirs, files in os.walk(home):
        rel_root = Path(root).relative_to(home)
        dirs[:] = sorted(d for d in dirs if not (rel_root == Path(".") and d == SKILL_STUDIO_STATE))
        for name in sorted(files) + [d + "/" for d in dirs]:
            p = Path(root) / name.rstrip("/")
            st = p.lstat()
            h.update(f"{rel_root / name}\0{stat.S_IMODE(st.st_mode):o}\0{st.st_size}\0{int(st.st_mtime)}\0".encode())
            if p.is_symlink():
                h.update(os.readlink(p).encode())
            elif p.is_file():
                h.update(p.read_bytes())
    return h.hexdigest()


def make_read_only(home: Path) -> None:
    for root, dirs, files in os.walk(home, topdown=False):
        if SKILL_STUDIO_STATE in Path(root).relative_to(home).parts[:1]:
            continue
        for name in files:
            p = Path(root) / name
            p.chmod(p.stat().st_mode & ~0o222)
        for name in dirs:
            if name == SKILL_STUDIO_STATE and Path(root) == home:
                continue
            p = Path(root) / name
            p.chmod(p.stat().st_mode & ~0o222)


def scale_plan(size: int, seed: int):
    if size < 36:
        raise ValueError("size must be at least 36")
    rng = random.Random(seed)
    library = [*LIBRARY, *(f"workshop-{i:04d}" for i in range(36, size))]
    manual = MANUAL_IN_CLAUDE | {n for i, n in enumerate(library[36:]) if i % 4 == 0}
    implicit = {**CODEX_IMPLICIT, **{n: False for i, n in enumerate(library[36:]) if i % 8 == 0}}
    uses = plan_uses(rng)
    for i, name in enumerate(library[36:]):
        for offset in [*range(i % 5), 46 + i % 135]:
            harness = pick(rng, HARNESS_WEIGHTS)
            uses.append(Use(name, harness, pick(rng, TRIGGERS[harness]), offset))
    sessions = group_sessions(rng, uses)
    mark_extras(sessions, rng)
    # Repeat the original traps without changing the published small fixture.
    for _ in range((size - 36) // 36):
        batch = group_sessions(rng, plan_uses(rng))
        mark_extras(batch, rng)
        sessions.extend(batch)
    return rng, library, manual, implicit, sessions


def scale_overlay(home: Path, task: str, library: list[str], truth: dict) -> None:
    lock_path = home / ".agents/.skill-lock.json"
    lock = json.loads(lock_path.read_text())
    repos = list(PACKS)
    for i, name in enumerate(library[36:]):
        if i % 4 == 3:
            if task == "share":
                truth["s2"].append(name)
            continue
        repo = repos[i % 3]
        lock["skills"][name] = {**lock["skills"][PACKS[repo][0]], "skillPath": f"skills/{name}/SKILL.md"}
        if task == "share":
            truth["s1"][repo] += 1
        lib = home / ".agents/skills" / name
        if repo == UPDATED_PACK:
            if task == "update":
                truth["u1"] += 1
            copy = home / ".codex/skills" / name
            copy.mkdir(parents=True)
            text = (lib / "SKILL.md").read_text()
            (copy / "SKILL.md").write_text(text)
            (lib / "SKILL.md").write_text(new_version(text))
            if task == "update":
                truth["u2"].append({"skill": name, "agent": "codex"})
        else:
            link = home / ".pi/agent/skills" / name
            link.symlink_to(Path("../../../.agents/skills") / name)
    lock_path.write_text(json.dumps(lock, indent=2) + "\n")


def build(out: Path, now: datetime, task: str = "usage", size: int = 36, seed: int = SEED) -> dict:
    rng = random.Random(seed)
    home = out / "home"
    if out.exists():
        for root, dirs, files in os.walk(out):
            for name in dirs + files:
                p = Path(root) / name
                if not p.is_symlink():
                    p.chmod(p.stat().st_mode | 0o200)
        import shutil

        shutil.rmtree(out)
    home.mkdir(parents=True)
    clock = Clock(now, rng)
    uses = plan_uses(rng)
    sessions = group_sessions(rng, uses)
    mark_extras(sessions, rng)
    library, manual, implicit = LIBRARY, MANUAL_IN_CLAUDE, CODEX_IMPLICIT
    if size != 36:
        rng, library, manual, implicit, sessions = scale_plan(size, seed)
    write_library(home, rng, library, manual, implicit)
    for i in range((size - 36) // 36):
        folder = home / ".agents/skills" / f"notes-{i:04d}"
        folder.mkdir()
        (folder / "ideas.md").write_text("Ideas for next month.\n")
    write_claude(home, [s for s in sessions if s.harness == "claude"], clock, rng)
    write_codex(home, [s for s in sessions if s.harness == "codex"], clock, rng)
    write_pi(home, [s for s in sessions if s.harness == "pi"], clock, rng)
    mtimes = write_cursor(home, [s for s in sessions if s.harness == "cursor"], clock, rng)
    write_opencode(home, [s for s in sessions if s.harness == "opencode"], clock, rng)
    all_uses = [u for s in sessions for u in s.uses]
    truth = ground_truth(all_uses, now, library, manual, implicit)
    truth["sessions"] = {h: sum(1 for s in sessions if s.harness == h) for h, _ in HARNESS_WEIGHTS}
    truth["traps"] = sorted({f"{s.harness}:{e}" for s in sessions for e in s.extras})
    truth["task"] = task
    if task != "usage":
        truth |= write_overlay(home, task, now)
        if size != 36:
            scale_overlay(home, task, library, truth)
            link = home / ".pi/agent/skills/commit-message"
            link.unlink()
            link.symlink_to("../../../.agents/skills/commit-message")
    (home / SKILL_STUDIO_STATE).mkdir()
    for path, t in mtimes:
        os.utime(path, (t.timestamp(), t.timestamp()))
    if size != 36:
        cursor_paths = {p for p, _ in mtimes}
        for root, dirs, files in os.walk(home, topdown=False):
            for name in dirs + files:
                path = Path(root) / name
                if path not in cursor_paths:
                    os.utime(path, (now.timestamp(), now.timestamp()), follow_symlinks=False)
    make_read_only(home)
    truth["fixture_hash"] = fixture_hash(home)
    return truth


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--task", choices=TASKS, default="usage")
    parser.add_argument("--size", type=int, default=36)
    parser.add_argument("--seed", type=int, default=SEED)
    parser.add_argument("--now", help="ISO clock (default: real time at size 36; 2026-11-14 noon UTC otherwise)")
    args = parser.parse_args()
    now = datetime.fromisoformat(args.now.replace("Z", "+00:00")) if args.now else datetime(2026, 11, 14, 12, tzinfo=timezone.utc) if args.size != 36 else datetime.now(timezone.utc)
    truth = build(args.out.resolve(), now, args.task, args.size, args.seed)
    keys = {"usage": ("q1", "q2", "q3", "q4"), "update": ("pack", "u1", "u2"), "share": ("s1", "s2")}[args.task]
    print(json.dumps({k: truth[k] for k in ("today", *keys, "sessions")}, indent=1))


if __name__ == "__main__":
    main()
