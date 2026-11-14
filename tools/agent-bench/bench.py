#!/usr/bin/env python3
# ============================================================================
# Skill Studio - agent benchmark runner
# Runs each (harness, model, method) cell on a fresh synthetic fixture, checks
# that the fixture did not change, scores the answer against ground truth and
# appends one record per run to a JSONL file. `report` aggregates the records.
# ============================================================================
"""Agent benchmark: do agents answer skill questions better with Skill Studio?

  python3 tools/agent-bench/bench.py run --matrix tools/agent-bench/matrix.json
  python3 tools/agent-bench/bench.py report
"""

from __future__ import annotations

import argparse
import json
import shutil
import re
import statistics
import subprocess
import sys
import threading
import time
import urllib.request
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path

import fixture
from harnesses import ADAPTERS, Run

BENCH = Path(__file__).resolve().parent
REPO = BENCH.parents[1]
RESULTS = BENCH / "results"
RAW = RESULTS / "runs.jsonl"
PRICES = RESULTS / "prices.json"
SUMMARY = RESULTS / "results.json"
MARKETING = REPO / "packages" / "marketing" / "src" / "data" / "agent-benchmark.json"
# An agent that reaches these folders read the user's real skills, not the fixture.
REAL_SKILL_ROOTS = tuple(f"{Path.home() / d}/" for d in (".agents", ".claude", ".codex", ".cursor", ".pi", ".config/opencode"))
TIMEOUT_S = 900
WRITE_LOCK = threading.Lock()


def load_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def parse_stream(text: str) -> list[dict]:
    events = []
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("{"):
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return events


TASK_PROMPTS = {"usage": "task.md", "update": "task_update.md", "share": "task_share.md"}
QUESTIONS = {"usage": ("q1", "q2", "q3", "q4"), "update": ("u1", "u2"), "share": ("s1", "s2")}


def task_of(record: dict) -> str:
    return record.get("task", "usage")


def run_id_for(cell: dict, rep: int) -> str:
    base = f"{cell['harness']}--{cell['model'].replace('/', '_')}--{cell['method']}--{rep}"
    return base if cell["task"] == "usage" else f"{cell['task']}--{base}"


def prompt_for(task: str, method: str, home: Path, truth: dict) -> str:
    text = (BENCH / "prompts" / TASK_PROMPTS[task]).read_text()
    text = text.replace("{home}", str(home)).replace("{today}", truth["today"]).replace("{window_start}", truth["window_start"])
    return text + "\n" + (BENCH / "prompts" / f"method_{method}.md").read_text()


# ----------------------------------------------------------------------------
# Scoring
# ----------------------------------------------------------------------------


def extract_answer(text: str) -> dict | None:
    blocks = re.findall(r"```(?:json)?\s*(\{.*?\})\s*```", text, re.S)
    for block in reversed(blocks):
        try:
            return json.loads(block)
        except json.JSONDecodeError:
            continue
    return None


def as_int(value) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


AGENT_ALIASES = {"claude": "claude-code", "claude code": "claude-code", "open-code": "opencode", "pi-agent": "pi", "cursor-agent": "cursor"}


def norm(value) -> str:
    text = str(value or "").strip().lower()
    return AGENT_ALIASES.get(text, text)


def f1(got: set, want: set) -> float:
    if not got and not want:
        return 1.0
    hits = len(got & want)
    return round(2 * hits / (len(got) + len(want)), 4)


def score(task: str, answer: dict | None, truth: dict) -> dict:
    return {"usage": score_usage, "update": score_update, "share": score_share}[task](answer or {}, truth)


def finish(out: dict, task: str) -> dict:
    out["correct"] = sum(bool(out[q]) for q in QUESTIONS[task])
    out["questions"] = len(QUESTIONS[task])
    return out


def score_update(answer: dict, truth: dict) -> dict:
    want = {(r["skill"], r["agent"]) for r in truth["u2"]}
    got = {(norm(r.get("skill")), norm(r.get("agent"))) for r in answer.get("u2") or [] if isinstance(r, dict)}
    return finish({"u1": as_int(answer.get("u1")) == truth["u1"], "u2": got == want, "u2_f1": f1(got, want)}, "update")


def score_share(answer: dict, truth: dict) -> dict:
    got_repos = {}
    for row in answer.get("s1") or []:
        if isinstance(row, dict) and row.get("repo"):
            repo = norm(row["repo"]).removeprefix("https://github.com/").removesuffix(".git")
            got_repos[repo] = as_int(row.get("skills"))
    want_files = set(truth["s2"])
    got_files = {norm(x) for x in answer.get("s2") or [] if isinstance(x, str)}
    return finish({"s1": got_repos == truth["s1"], "s2": got_files == want_files, "s2_f1": f1(got_files, want_files)}, "share")


def score_usage(answer: dict, truth: dict) -> dict:
    if not answer:
        return finish({"q1": False, "q2": False, "q3": False, "q4": False, "q3_names": 0.0, "q3_counts": 0.0}, "usage")
    want = {row["skill"]: row["uses"] for row in truth["q3"]}
    got = {}
    for row in answer.get("q3") or []:
        if isinstance(row, dict) and row.get("skill"):
            got[str(row["skill"]).strip().lower()] = as_int(row.get("uses"))
    names = sum(1 for s in want if s in got) / len(want)
    counts = sum(1 for s, n in want.items() if got.get(s) == n) / len(want)
    out = {
        "q1": as_int(answer.get("q1")) == truth["q1"],
        "q2": as_int(answer.get("q2")) == truth["q2"],
        "q3": counts == 1.0 and len(got) == len(want),
        "q4": as_int(answer.get("q4")) == truth["q4"],
        "q3_names": names,
        "q3_counts": counts,
    }
    return finish(out, "usage")


# ----------------------------------------------------------------------------
# Prices (for harnesses that report tokens but not cost)
# ----------------------------------------------------------------------------


def load_prices(refresh: bool = False) -> dict:
    if PRICES.exists() and not refresh:
        return json.loads(PRICES.read_text())
    matrix = json.loads((BENCH / "matrix.json").read_text())
    wanted = {c.get("price_id") or c["model"] for c in matrix["cells"]}
    with urllib.request.urlopen("https://openrouter.ai/api/v1/models", timeout=30) as resp:
        models = json.load(resp)["data"]
    prices = {
        "fetched": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "source": "https://openrouter.ai/api/v1/models (USD per token)",
        "models": {
            m["id"]: {k: float(m["pricing"].get(k) or 0) for k in ("prompt", "completion", "input_cache_read", "input_cache_write")}
            for m in models
            if m["id"] in wanted
        },
    }
    RESULTS.mkdir(parents=True, exist_ok=True)
    PRICES.write_text(json.dumps(prices, indent=1, sort_keys=True) + "\n")
    return prices


def estimate_cost(cell: dict, tokens: dict, prices: dict) -> float | None:
    price = prices["models"].get(cell.get("price_id") or cell["model"])
    if not price:
        return None
    cache_read = price["input_cache_read"] or price["prompt"]
    cache_write = price["input_cache_write"] or price["prompt"]
    return round(
        tokens.get("input", 0) * price["prompt"]
        + tokens.get("cache_read", 0) * cache_read
        + tokens.get("cache_write", 0) * cache_write
        + (tokens.get("output", 0) + tokens.get("reasoning", 0)) * price["completion"],
        6,
    )


# ----------------------------------------------------------------------------
# One run
# ----------------------------------------------------------------------------


def run_once(cell: dict, rep: int, args, prices: dict) -> dict:
    run_id = run_id_for(cell, rep)
    task = cell["task"]
    root = args.workdir / run_id
    started = datetime.now(timezone.utc)
    shutil.rmtree(root / "rt", ignore_errors=True)
    truth = fixture.build(root / "fixture", started, task)
    home = root / "fixture" / "home"
    cwd = root / "cwd"
    cwd.mkdir(parents=True, exist_ok=True)
    run = Run(
        harness=cell["harness"], model=cell["model"], method=cell["method"],
        prompt=prompt_for(task, cell["method"], home, truth), cwd=cwd, runtime=root / "rt",
        home=home, skill_studio=args.skill_studio, wrapper_dir=root / "bin",
    )
    command, parse = ADAPTERS[cell["harness"]]
    argv, env = command(run)
    t0 = time.monotonic()
    error = None
    try:
        proc = subprocess.run(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=TIMEOUT_S)
        stdout, stderr = proc.stdout, proc.stderr
        if proc.returncode != 0:
            error = f"exit {proc.returncode}"
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode() if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        error = f"timeout {TIMEOUT_S}s"
    duration = time.monotonic() - t0
    (root / "stdout.jsonl").write_text(stdout)
    (root / "stderr.txt").write_text(stderr)

    parsed = parse(parse_stream(stdout))
    error = parsed.get("error") or error
    if parsed["cost_usd"] is None:
        parsed["cost_usd"] = estimate_cost(cell, parsed["tokens"], prices)
    answer = extract_answer(parsed["answer"])
    if any(path in stdout for path in REAL_SKILL_ROOTS):
        error, answer = "read the real home", None
    intact = fixture.fixture_hash(home) == truth["fixture_hash"]
    record = {
        "id": run_id, "task": task, "harness": cell["harness"], "model": cell["model"], "label": cell.get("label", cell["model"]),
        "method": cell["method"], "rep": rep, "started": started.isoformat(timespec="seconds"),
        "duration_s": round(duration, 1), "cost_usd": parsed["cost_usd"], "cost_source": parsed["cost_source"],
        "tokens": parsed["tokens"], "tool_calls": parsed["tool_calls"], "turns": parsed["turns"],
        "fixture_intact": intact, "error": error, "answer": answer, "score": score(task, answer, truth),
        "truth": {k: truth[k] for k in (*QUESTIONS[task], "fixture_version")},
    }
    with WRITE_LOCK:
        with args.out.open("a") as f:
            f.write(json.dumps(record) + "\n")
    print(
        f"{run_id}: {record['score']['correct']}/{record['score']['questions']} {record['duration_s']}s "
        f"${record['cost_usd'] if record['cost_usd'] is not None else '?'} tools={record['tool_calls']}"
        f"{'' if intact else ' FIXTURE CHANGED'}{' ERROR ' + str(error) if error else ''}",
        flush=True,
    )
    return record


def expand(matrix: dict, args) -> list[dict]:
    cells = []
    for cell in matrix["cells"]:
        for task in cell.get("tasks", ["usage"]):
            for method in cell.get("methods", matrix["methods"]):
                cells.append({**cell, "task": task, "method": method})
    if args.task:
        cells = [c for c in cells if c["task"] in args.task]
    if args.harness:
        cells = [c for c in cells if c["harness"] in args.harness]
    if args.method:
        cells = [c for c in cells if c["method"] in args.method]
    if args.model:
        cells = [c for c in cells if c["model"] in args.model]
    return cells


def cmd_run(args) -> None:
    matrix = json.loads(args.matrix.read_text())
    cells = expand(matrix, args)
    runs = args.runs or matrix.get("runs", 3)
    done = {r["id"] for r in load_jsonl(args.out) if not r.get("error") or not args.retry_errors}
    prices = load_prices()
    queues: dict[str, list[tuple[dict, int]]] = defaultdict(list)
    for rep in range(1, runs + 1):
        for cell in cells:
            if run_id_for(cell, rep) not in done:
                queues[cell["harness"]].append((cell, rep))
    total = sum(len(q) for q in queues.values())
    print(f"{total} runs to do across {len(queues)} harnesses (skipping {len(done)} done)", flush=True)
    if args.dry_run:
        for harness, q in queues.items():
            for cell, rep in q:
                print(f"  {cell['task']} {harness} {cell['model']} {cell['method']} #{rep}")
        return
    args.out.parent.mkdir(parents=True, exist_ok=True)

    def drain(queue):
        for cell, rep in queue:
            try:
                run_once(cell, rep, args, prices)
            except Exception as exc:  # one broken run must not stop the matrix
                print(f"{cell['harness']} {cell['model']} {cell['method']} #{rep}: crashed: {exc}", file=sys.stderr, flush=True)

    with ThreadPoolExecutor(max_workers=len(queues) or 1) as pool:
        list(pool.map(drain, queues.values()))


# ----------------------------------------------------------------------------
# Report
# ----------------------------------------------------------------------------


def spread(values: list[float]) -> dict | None:
    values = [v for v in values if v is not None]
    if not values:
        return None
    return {"median": round(statistics.median(values), 4), "min": round(min(values), 4), "max": round(max(values), 4)}


def answer_key(task: str, answer: dict | None) -> str:
    a = answer or {}
    if task == "usage":
        rows = sorted((r.get("skill"), r.get("uses")) for r in (a.get("q3") or []) if isinstance(r, dict))
        return json.dumps([a.get("q1"), a.get("q2"), a.get("q4"), rows], default=str)
    if task == "update":
        rows = sorted((norm(r.get("skill")), norm(r.get("agent"))) for r in (a.get("u2") or []) if isinstance(r, dict))
        return json.dumps([a.get("u1"), rows], default=str)
    repos = sorted((norm(r.get("repo")), r.get("skills")) for r in (a.get("s1") or []) if isinstance(r, dict))
    return json.dumps([repos, sorted(norm(x) for x in (a.get("s2") or []) if isinstance(x, str))], default=str)


def agreement(task: str, answers: list[dict | None]) -> float:
    keys = [answer_key(task, a) for a in answers]
    return round(Counter(keys).most_common(1)[0][1] / len(keys), 2) if keys else 0.0


def cell_row(task: str, harness: str, model: str, method: str, runs: list[dict]) -> dict:
    valid = [r for r in runs if r["fixture_intact"]]
    questions = QUESTIONS[task]
    return {
        "task": task, "harness": harness, "model": model, "label": runs[0]["label"], "method": method,
        "runs": len(runs), "errors": sum(1 for r in runs if r["error"]),
        "fixture_changed": sum(1 for r in runs if not r["fixture_intact"]),
        "questions": len(questions),
        "correct": spread([r["score"]["correct"] for r in valid]),
        "correct_share": spread([r["score"]["correct"] / len(questions) for r in valid]),
        "per_question": {q: round(sum(bool(r["score"][q]) for r in valid) / len(valid), 2) if valid else None for q in questions},
        "duration_s": spread([r["duration_s"] for r in valid]),
        "cost_usd": spread([r["cost_usd"] for r in valid]),
        "cost_source": runs[0]["cost_source"],
        "tokens": spread([sum(r["tokens"].values()) for r in valid]),
        "tool_calls": spread([r["tool_calls"] for r in valid]),
        "agreement": agreement(task, [r["answer"] for r in valid]),
    }


def method_summary(rows: list[dict]) -> dict:
    return {
        "cells": len(rows),
        **{
            metric: spread([r[metric]["median"] for r in rows if r[metric]])
            for metric in ("correct_share", "duration_s", "cost_usd", "tool_calls")
        },
    }


def cmd_report(args) -> None:
    records = load_jsonl(args.out)
    grouped: dict[tuple, list[dict]] = defaultdict(list)
    for r in records:
        grouped[(task_of(r), r["harness"], r["model"], r["method"])].append(r)
    rows = [cell_row(*key, runs) for key, runs in sorted(grouped.items(), key=lambda kv: (fixture.TASKS.index(kv[0][0]), kv[0][1:]))]
    tasks = {}
    for task in fixture.TASKS:
        task_rows = [r for r in rows if r["task"] == task]
        if not task_rows:
            continue
        task_records = [r for r in records if task_of(r) == task]
        by_method: dict[str, list[dict]] = defaultdict(list)
        for row in task_rows:
            by_method[row["method"]].append(row)
        tasks[task] = {
            "runs": len(task_records),
            "questions": list(QUESTIONS[task]),
            "truth": task_records[0]["truth"],
            "overall": {method: method_summary(rs) for method, rs in sorted(by_method.items())},
        }
    summary = {
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "fixture_version": records[0]["truth"]["fixture_version"] if records else None,
        "total_runs": len(records),
        "total_cost_usd": round(sum(r["cost_usd"] or 0 for r in records), 2),
        "tasks": tasks,
        "cells": rows,
    }
    SUMMARY.write_text(json.dumps(summary, indent=1) + "\n")
    public_tasks = {task: {k: v for k, v in t.items() if k != "truth"} for task, t in tasks.items()}
    MARKETING.parent.mkdir(parents=True, exist_ok=True)
    MARKETING.write_text(json.dumps({**{k: summary[k] for k in ("generated", "fixture_version", "total_runs", "total_cost_usd")}, "tasks": public_tasks, "cells": rows}, indent=1) + "\n")
    print(f"{len(records)} runs, ${summary['total_cost_usd']} -> {SUMMARY.relative_to(REPO)} and {MARKETING.relative_to(REPO)}")
    for row in rows:
        c, d, k = row["correct"], row["duration_s"], row["cost_usd"]
        print(f"  {row['task']:6} {row['harness']:8} {row['label']:14} {row['method']:4} correct {c and c['median']}/{row['questions']}  {d and d['median']}s  ${k and k['median']}  agree {row['agreement']}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run", help="run the matrix; skips runs already in the results file")
    run.add_argument("--matrix", type=Path, default=BENCH / "matrix.json")
    run.add_argument("--runs", type=int, help="runs per cell (default: the matrix value)")
    run.add_argument("--harness", action="append")
    run.add_argument("--method", action="append")
    run.add_argument("--task", action="append", choices=fixture.TASKS)
    run.add_argument("--model", action="append")
    run.add_argument("--workdir", type=Path, default=Path("/tmp/skill-studio-bench"))
    run.add_argument("--skill-studio", type=Path, default=REPO / "target" / "debug" / "skill-studio")
    run.add_argument("--retry-errors", action="store_true", help="run again the runs that ended in an error")
    run.add_argument("--dry-run", action="store_true")
    report = sub.add_parser("report", help="aggregate the results file")
    sub.add_parser("prices", help="refresh the cached OpenRouter prices")
    for p in (run, report):
        p.add_argument("--out", type=Path, default=RAW)
    args = parser.parse_args()
    if args.cmd == "run":
        args.skill_studio = args.skill_studio.resolve()
        if not args.skill_studio.exists():
            raise SystemExit(f"{args.skill_studio} missing. Build it: cargo build --workspace --all-features")
        cmd_run(args)
    elif args.cmd == "report":
        cmd_report(args)
    else:
        load_prices(refresh=True)


if __name__ == "__main__":
    main()
