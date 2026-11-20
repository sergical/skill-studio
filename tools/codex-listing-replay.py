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
                body = text.split("### Available skills", 1)[1].split("</skills_instructions>", 1)[0]
                out = []
                for entry in body.splitlines():
                    if entry.startswith("- ") and " (file: " in entry:
                        head = entry[2:].rsplit(" (file: ", 1)[0]
                        name, sep, description = head.partition(": ")
                        out.append((name if sep else name.rstrip(":"), len(description)))
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
        predicted = [(e["name"], e["shown_description_chars"])
                     for e in row["entries"] if e["state"] not in ("hidden", "omitted")]
        names = [n for n, _ in predicted] == [n for n, _ in recorded]
        exact = sum(1 for p, r in zip(predicted, recorded) if p == r)
        totals["sessions"] += 1
        totals["exact_sessions"] += names and exact == len(recorded) == len(predicted)
        totals["entries"] += len(recorded)
        totals["listed"] += sum(1 for p, r in zip(predicted, recorded) if p[0] == r[0])
        totals["exact"] += exact
        print(f"{rollout}: {len(recorded)} recorded, {len(predicted)} predicted, "
              f"same names in order: {names}, exact lengths: {exact}, tier: {row['thresholds']['tier']}")
    print(f"{totals['sessions']} sessions, {totals['exact_sessions']} exact; "
          f"{totals['listed']}/{totals['entries']} entries in place, "
          f"{totals['exact']}/{totals['entries']} exact description lengths")


if __name__ == "__main__":
    main()
