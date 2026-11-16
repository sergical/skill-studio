"""One read-only product call per benchmark question. No model or network."""

from __future__ import annotations

import json
import os
import queue
import subprocess
import tempfile
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

import fixture

BYTE_BUDGET = 2048
BASELINE = Path(__file__).parent / "contracts/baseline.json"
DAYS = {"days": fixture.WINDOW_DAYS}
# Each question: CLI arguments after `library`, then the MCP `library` arguments.
CALLS = {
    "q1": (["--limit", "0"], {"limit": 0}),
    "q2": (["--days", str(fixture.WINDOW_DAYS), "--limit", "0"], {**DAYS, "limit": 0}),
    "q3": (["--days", str(fixture.WINDOW_DAYS), "--top", "5", "--fields", "name,uses"],
           {**DAYS, "top": 5, "fields": ["name", "uses"]}),
    "q4": (["--mode", "claude-code=manual", "--mode", "codex=auto", "--limit", "0"],
           {"modes": {"claude-code": "manual", "codex": "auto"}, "limit": 0}),
    "u1": (["--source", fixture.UPDATED_PACK, "--limit", "0"], {"source": fixture.UPDATED_PACK, "limit": 0}),
    "u2": (["--source", fixture.UPDATED_PACK, "--fields", "name,copies"],
           {"source": fixture.UPDATED_PACK, "fields": ["name", "copies"]}),
    "s1": (["--count-by", "source", "--limit", "0"], {"count_by": "source", "limit": 0}),
    "s2": (["--source", "manual", "--fields", "name"], {"source": "manual", "fields": ["name"]}),
}


def environment(home: Path) -> dict:
    env = {k: os.environ[k] for k in ("PATH", "TMPDIR") if k in os.environ}
    env.update({
        "HOME": str(home), "SKILL_STUDIO_HOME": str(home),
        "CODEX_HOME": str(home / ".codex"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "PI_CODING_AGENT_DIR": str(home / ".pi/agent"),
        "SKILL_STUDIO_TELEMETRY": "0",
    })
    return env


def mcp_call(binary: Path, home: Path, tool: str, arguments: dict) -> bytes:
    with tempfile.TemporaryFile() as errors:
        proc = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=errors, env=environment(home), cwd=home)
        messages = queue.Queue()

        def read():
            for line in proc.stdout:
                messages.put(line)
            messages.put(b"")

        threading.Thread(target=read, daemon=True).start()

        def send(message):
            proc.stdin.write(json.dumps({"jsonrpc": "2.0", **message}).encode() + b"\n")
            proc.stdin.flush()

        def receive(request_id):
            deadline = time.monotonic() + 120
            while True:
                line = messages.get(timeout=max(0.01, deadline - time.monotonic()))
                if not line:
                    raise RuntimeError("MCP closed stdout")
                message = json.loads(line)
                if message.get("id") == request_id:
                    if "error" in message:
                        raise RuntimeError("MCP protocol error")
                    return message["result"]

        try:
            send({"id": 1, "method": "initialize", "params": {
                "protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "answer-contracts", "version": "1"},
            }})
            receive(1)
            send({"method": "notifications/initialized"})
            send({"id": 2, "method": "tools/call", "params": {"name": tool, "arguments": arguments}})
            result = receive(2)
            return json.dumps(result, separators=(",", ":"), ensure_ascii=False).encode()
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            proc.stdin.close()
            proc.stdout.close()


def payload(raw: bytes, transport: str) -> dict:
    data = json.loads(raw)
    if transport == "mcp":
        if data.get("isError"):
            raise ValueError("MCP tool error")
        data = json.loads(next(c["text"] for c in data["content"] if c["type"] == "text"))
    # Both transports return the shared operation envelope.
    if data.get("status") == "error":
        raise ValueError("operation error")
    return data


def extract(question: str, envelope: dict):
    data = envelope["data"]
    if question in {"q1", "q4", "u1"}:
        return data["count"]
    if question == "q2":
        return data["used"]
    if question == "s1":
        return [{"repo": repo, "skills": n} for repo, n in envelope["page"]["counts"].items() if repo != "null"]
    if envelope.get("page", {}).get("next_cursor"):
        raise ValueError("answer spans more than one page")
    if question == "q3":
        return [{"skill": r["name"], "uses": r["uses"]} for r in data["rows"]]
    if question == "u2":
        return [{"skill": r["name"], "agent": agent} for r in data["rows"]
                for agent, status in r["copies"].items() if status in {"old", "changed"}]
    return [r["name"] for r in data["rows"]]


def run(args, score, questions, repo: Path) -> int:
    binaries = {"cli": repo / "target/debug/skill-studio", "mcp": repo / "target/debug/skill-studio-mcp"}
    for binary in binaries.values():
        if not binary.is_file():
            raise SystemExit("Build first: cargo build --workspace --all-features")
    results = []
    started = time.monotonic()
    print("task   question size  call result bytes reason")
    for size in args.size or [40, 400]:
        for task, keys in questions.items():
            with tempfile.TemporaryDirectory(prefix="answer-contract-") as temp:
                now = datetime.now(timezone.utc).replace(hour=12, minute=0, second=0, microsecond=0)
                truth = fixture.build(Path(temp) / "fixture", now, task, size, args.seed)
                home = Path(temp) / "fixture/home"
                for question in keys:
                    for transport, binary in binaries.items():
                        flags, arguments = CALLS[question]
                        tool = "library"
                        argv = [str(binary), "library", "--home", str(home), "--json", *flags]
                        raw, answer, error = b"", None, None
                        try:
                            if transport == "cli":
                                proc = subprocess.run(argv, env=environment(home), cwd=home, capture_output=True, timeout=120)
                                raw = proc.stdout
                                if proc.returncode:
                                    raise ValueError(f"CLI exit {proc.returncode}")
                            else:
                                raw = mcp_call(binary, home, tool, arguments)
                            answer = extract(question, payload(raw, transport))
                        except (ValueError, KeyError, StopIteration, RuntimeError, queue.Empty, subprocess.TimeoutExpired) as exc:
                            error = str(exc) or type(exc).__name__
                        correct = score(task, {question: answer}, truth)[question]
                        reason = error or ("answer not in output" if answer is None else
                                 f"count wrong: {answer} vs {truth[question]}" if isinstance(answer, int) and not correct else
                                 "answer wrong" if not correct else "answer correct")
                        if len(raw) > BYTE_BUDGET:
                            reason += f"; over budget: {len(raw) / 1024:.1f} KB"
                        row = {"task": task, "question": question, "size": size, "seed": args.seed,
                               "transport": transport, "pass": bool(correct and len(raw) <= BYTE_BUDGET and not error),
                               "bytes": len(raw), "reason": reason,
                               "call": argv[1:] if transport == "cli" else {"tool": tool, "arguments": arguments}}
                        # Temporary paths belong to this run, not to the saved report.
                        row["call"] = json.loads(json.dumps(row["call"]).replace(str(home), "<fixture>"))
                        results.append(row)
                        print(f"{task:6} {question:8} {size:4} {transport:4} {'PASS' if row['pass'] else 'FAIL':6} {len(raw):7} {reason}", flush=True)
    report = {"byte_budget": BYTE_BUDGET, "duration_s": round(time.monotonic() - started, 3), "results": results}
    args.json_out.parent.mkdir(parents=True, exist_ok=True)
    args.json_out.write_text(json.dumps(report, indent=2) + "\n")
    key = lambda r: f"{r['task']}/{r['question']}/{r['size']}/{r['seed']}/{r['transport']}"
    passing = {key(r) for r in results if r["pass"]}
    tested = {key(r) for r in results}
    baseline = set(json.loads(BASELINE.read_text()))
    regressions = (baseline & tested) - passing
    if args.update_baseline:
        BASELINE.write_text(json.dumps(sorted(baseline | passing), indent=2) + "\n")
    print(f"{len(passing)}/{len(results)} pass; {report['duration_s']}s; JSON: {args.json_out}")
    if args.check and regressions:
        print("Regressions: " + ", ".join(sorted(regressions)))
        return 1
    return 0
