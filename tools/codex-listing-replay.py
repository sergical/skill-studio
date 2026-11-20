#!/usr/bin/env python3
"""Check Skill Studio's Codex listing against real Codex sessions.

For each rollout, runs `skill-studio listing --codex-session <rollout>` and
compares the predicted Codex entries with the rollout's `### Available skills`
block: names in order, and description length per line. Session logs are
private, so this runs on your machine, not in CI.

Usage: codex-listing-replay.py <skill-studio binary> <rollout.jsonl>... [-- extra listing args]
Example: tools/codex-listing-replay.py target/debug/skill-studio ~/.codex/sessions/2026/11/20/rollout-*.jsonl
"""

import json
import os
import subprocess
import sys


def recorded_lines(path):
    for line in open(path, errors="ignore"):
        if "<skills_instructions>" not in line:
            continue
        payload = json.loads(line).get("payload", {})
        if payload.get("role") != "developer":
            continue
        for block in payload.get("content", []):
            text = block.get("text", "")
            if "### Available skills" in text:
                head_text, body = text.split("### Available skills", 1)
                body = body.split("</skills_instructions>", 1)[0]
                roots = {}
                for root in head_text.splitlines():
                    if root.startswith("- `") and "` = `" in root:
                        alias, path = root[3:].split("` = `", 1)
                        roots[alias] = path.rstrip("`")
                out = []
                for entry in body.splitlines():
                    if entry.startswith("- ") and " (file: " in entry:
                        head, locator = entry[2:].rsplit(" (file: ", 1)
                        locator = locator.rstrip(")")
                        alias, _, rest = locator.partition("/")
                        path = os.path.join(roots[alias], rest) if alias in roots else locator
                        name, sep, description = head.partition(": ")
                        out.append((name if sep else name.rstrip(":"), len(description),
                                    os.path.realpath(path)))
                return out
    return None


def main():
    args = sys.argv[1:]
    extra = args[args.index("--") + 1:] if "--" in args else []
    binary, rollouts = args[0], [a for a in args[1:args.index("--")] if a] if "--" in args else args[1:]
    totals = {"sessions": 0, "exact_sessions": 0, "entries": 0, "listed": 0, "exact": 0}
    for rollout in rollouts:
        recorded = recorded_lines(rollout)
        if recorded is None:
            continue
        result = subprocess.run(
            [binary, "listing", "--json", "--limit", "5000", "--codex-session", rollout, *extra],
            capture_output=True, text=True,
        )
        envelope = json.loads(result.stdout)
        if envelope["status"] != "ok":
            print(f"{rollout}: {envelope['errors']}")
            continue
        row = next(r for r in envelope["data"]["listings"] if r["harness"] == "codex")
        # Name, description length and the resolved SKILL.md path: two
        # copies of one skill must not stand in for each other.
        predicted = [(e["name"], e["shown_description_chars"],
                      os.path.realpath(os.path.join(e["path"] or "", "SKILL.md")))
                     for e in row["entries"] if e["state"] not in ("hidden", "omitted")]
        names = [p[0] for p in predicted] == [r[0] for r in recorded]
        by_file = {(p[0], p[2]): p[1] for p in predicted}
        found = [r for r in recorded if (r[0], r[2]) in by_file]
        exact = sum(1 for r in found if by_file[(r[0], r[2])] == r[1])
        missing = [r[0] for r in recorded if (r[0], r[2]) not in by_file]
        extra_entries = [p[0] for p in predicted if (p[0], p[2]) not in {(r[0], r[2]) for r in recorded}]
        totals["sessions"] += 1
        totals["exact_sessions"] += names and exact == len(recorded) == len(predicted)
        totals["entries"] += len(recorded)
        totals["listed"] += len(found)
        totals["exact"] += exact
        print(f"{rollout}: {len(recorded)} recorded, {len(predicted)} predicted, "
              f"same names in order: {names}, exact lengths: {exact}, tier: {row['thresholds']['tier']}"
              + (f", not predicted: {missing[:8]}" if missing else "")
              + (f", not recorded: {extra_entries[:8]}" if extra_entries else ""))
    print(f"{totals['sessions']} sessions, {totals['exact_sessions']} exact; "
          f"{totals['listed']}/{totals['entries']} recorded entries predicted (same name and file), "
          f"{totals['exact']}/{totals['entries']} exact description lengths")


if __name__ == "__main__":
    main()
