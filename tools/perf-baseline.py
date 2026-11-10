"""Release baseline over isolated FixtureBuilder homes. No duration gates."""
import argparse
import json
import math
import os
from pathlib import Path
import platform
import re
import select
import shutil
import statistics
import subprocess
import tempfile
import time


def command(args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs).strip()


def distribution(samples):
    ordered = sorted(samples)
    return {"n": len(samples), "median": statistics.median(samples),
            "p95": ordered[math.ceil(len(samples) * .95) - 1],
            "max": max(samples), "samples": samples}


def environment(home, projects):
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("SKILL_STUDIO_", "XDG_", "OPENCODE_", "CODEX_", "CLAUDE_")):
            del env[key]
    env.update(HOME=str(home), SKILL_STUDIO_FIXTURE=str(home),
               SKILL_STUDIO_HOME=str(home), SKILL_STUDIO_TELEMETRY="0",
               SKILL_STUDIO_PROJECT=os.pathsep.join(projects),
               XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".data"),
               XDG_CACHE_HOME=str(home / ".cache"), CODEX_HOME=str(home / ".codex"))
    return env


def measured_cli(binary, verb, home, projects, env):
    args = [str(binary), verb, "--fixture", str(home), "--json", "--read-timeout-ms", "60000"]
    for project in projects:
        args += ["--project", project]
    timer = ["/usr/bin/time", "-l"] if platform.system() == "Darwin" else ["/usr/bin/time", "-v"]
    start = time.perf_counter_ns()
    result = subprocess.run(timer + args, env=env, capture_output=True, text=True, timeout=120, check=False)
    elapsed = (time.perf_counter_ns() - start) / 1e6
    if result.returncode not in ([0, 1, 4] if verb == "doctor" else [0, 4] if verb == "scan" else [0]):
        raise RuntimeError(f"{verb}: {result.returncode}: {result.stderr}")
    envelope = json.loads(result.stdout)
    match = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
    if match:
        rss = int(match[1])
    else:
        match = re.search(r"Maximum resident set size \(kbytes\): (\d+)", result.stderr)
        if not match:
            raise RuntimeError("time did not report peak RSS")
        rss = int(match[1]) * 1024
    return elapsed, rss, envelope["status"]


def mcp_samples(binary, env, runs):
    child = subprocess.Popen([str(binary), "mcp"], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    def request(payload):
        child.stdin.write(json.dumps(payload) + "\n")
        child.stdin.flush()
    def response():
        if not select.select([child.stdout], [], [], 120)[0]:
            raise TimeoutError("MCP response exceeded the 120 second transport safety bound")
        line = child.stdout.readline()
        if not line:
            raise RuntimeError("MCP exited before response")
        return json.loads(line)
    try:
        request({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "perf-baseline", "version": "1"}}})
        assert "result" in response()
        request({"jsonrpc": "2.0", "method": "notifications/initialized"})
        samples = []
        statuses = []
        for i in range(runs):
            start = time.perf_counter_ns()
            request({"jsonrpc": "2.0", "id": i + 2, "method": "tools/call", "params": {"name": "scan", "arguments": {}}})
            reply = response()
            samples.append((time.perf_counter_ns() - start) / 1e6)
            assert reply["id"] == i + 2 and "result" in reply, reply
            result = reply["result"]
            envelope = result["structuredContent"]
            statuses.append({"status": envelope["status"], "skills": len(envelope.get("data", {}).get("skills", [])), "is_error": result.get("isError", False)})
        return samples, statuses
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("docs/perf/baseline.json"))
    parser.add_argument("--runs", type=int, default=10)
    args = parser.parse_args()
    if args.runs < 10:
        parser.error("at least 10 samples are required")
    target = Path(os.environ["CARGO_TARGET_DIR"]) / "release"
    with tempfile.TemporaryDirectory(prefix="skill-perf-binaries-") as temp:
        binaries = Path(temp)
        (binaries / "examples").mkdir()
        build_and_run(args, target, binaries)


def build_and_run(args, target, binaries):
    subprocess.run(["cargo", "build", "--release", "--all-features", "-p", "skill-studio-core", "--example", "perf_baseline", "-p", "skill-studio-cli"], check=True)
    for executable in ["skill-studio", "examples/perf_baseline"]:
        shutil.copy2(target / executable, binaries / executable)
    subprocess.run(["cargo", "build", "--release", "--all-features", "-p", "skill-studio", "--example", "perf_snapshot"], check=True)
    shutil.copy2(target / "examples/perf_snapshot", binaries / "examples/perf_snapshot")
    target = binaries
    mac = platform.system() == "Darwin"
    report = {"git_sha": command(["git", "rev-parse", "HEAD"]),
              "dirty": bool(command(["git", "status", "--porcelain"])),
              "os": command(["sw_vers"]) if mac else platform.platform(),
              "cpu": command(["sysctl", "-n", "machdep.cpu.brand_string"]) if mac else platform.processor(),
              "memory_bytes": int(command(["sysctl", "-n", "hw.memsize"])) if mac else os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES"),
              "rustc": command(["rustc", "-Vv"]), "profile": "release", "telemetry": "off",
              "cache_state": "Fresh runtime and reused runtime; OS cache uncontrolled, not cold disk. MCP persistent after initialize. Empty history.",
              "fixtures": {}}
    for label, count in [("small", 20), ("real-shaped", 378), ("large", 5000)]:
        with tempfile.TemporaryDirectory(prefix="skill-perf-") as temp:
            home = Path(temp) / "home"
            data = json.loads(command([str(target / "examples/perf_baseline"), str(home), str(count), str(args.runs)]))
            projects = data.pop("projects")
            env = environment(home, projects)
            metrics = {key: distribution(data.pop(key)) for key in list(data) if key.endswith("_ms")}
            outcomes = {}
            for verb in ["scan", "doctor", "history"]:
                samples = [measured_cli(target / "skill-studio", verb, home, projects, env) for _ in range(args.runs)]
                outcomes[verb] = [s[2] for s in samples]
                metrics[f"cli_{verb}_ms"] = distribution([s[0] for s in samples])
                metrics[f"cli_{verb}_peak_rss_bytes"] = distribution([s[1] for s in samples])
            mcp_times, mcp_results = mcp_samples(target / "skill-studio", env, args.runs)
            metrics["mcp_scan_round_trip_ms"] = distribution(mcp_times)
            outcomes["mcp_scan"] = mcp_results
            registry = home / ".agents/skill-studio.json"
            registry.write_text(json.dumps({"projects": {"added": projects}}))
            desktop = json.loads(command([str(target / "examples/perf_snapshot")], env=env))
            outcomes["desktop_skills"] = desktop["desktop_skills"]
            metrics["desktop_first_list_ipc_bytes"] = distribution(desktop["desktop_first_list_ipc_bytes"])
            report["fixtures"][label] = {**data, "outcomes": outcomes, "metrics": metrics}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    table = ["# Performance baseline", "", f"Commit: `{report['git_sha']}`. Dirty: {report['dirty']}.", "", report["cache_state"], "", "| Fixture | Metric | N | Median | p95 | Max |", "| --- | --- | ---: | ---: | ---: | ---: |"]
    for label, fixture in report["fixtures"].items():
        for metric, stats in fixture["metrics"].items():
            table.append(f"| {label} | {metric} | {stats['n']} | {stats['median']:.3f} | {stats['p95']:.3f} | {stats['max']:.3f} |")
    args.output.with_suffix(".md").write_text("\n".join(table) + "\n")
    print(f"Wrote {args.output} and {args.output.with_suffix('.md')}")


if __name__ == "__main__":
    main()
