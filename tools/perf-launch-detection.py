#!/usr/bin/env python3
"""Measure the real desktop detection helper over a disposable fixture home."""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time


def read(*args):
    return subprocess.run(args, capture_output=True, text=True, check=False).stdout.strip()


def distribution(rows, key):
    values = sorted(row[key] for row in rows)
    return {"n": len(values), "median": statistics.median(values),
            "p95": values[math.ceil(len(values) * .95) - 1], "max": max(values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--version-binary", type=Path, required=True)
    parser.add_argument("--fixture-generator", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binaries = {key: path.resolve() for key, path in {
        "desktop": args.binary, "version_fixture": args.version_binary,
        "fixture_generator": args.fixture_generator}.items()}
    report = {"status": "provisional: recorded under load",
              "source_git_sha": read("git", "rev-parse", "HEAD"),
              "dirty": bool(read("git", "status", "--porcelain")),
              "recorded_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "hardware": {"cpu": read("/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"),
                           "memory_bytes": int(read("/usr/sbin/sysctl", "-n", "hw.memsize")),
                           "macos": read("/usr/bin/sw_vers", "-productVersion")},
              "binary_sha256": {key: hashlib.sha256(path.read_bytes()).hexdigest()
                                for key, path in binaries.items()},
              "cache_state": "fresh native process, Tauri runtime and login shell each sample; warm executable trust and OS caches",
              "percentile": "nearest rank", "phases": {}}
    with tempfile.TemporaryDirectory(prefix="skill-studio-launch-") as temporary:
        root = Path(temporary).resolve()
        home = root / "home"
        fixture = json.loads(subprocess.check_output(
            [str(binaries["fixture_generator"]), str(home), "378", "10"], text=True))
        report["fixture"] = {"skills": fixture["skills"], "global_skills": fixture["global_skills"],
                             "plugin_caches": ["claude", "codex"], "broken_skill_files": 3,
                             "seed": fixture["seed"], "agents": 6}
        (home / ".skill-studio-perf-fixture").write_text("fixture\n")
        bin_dir = root / "bin"
        bin_dir.mkdir()
        version = root / "version-fixture"
        shutil.copy2(binaries["version_fixture"], version)
        for name in ["claude", "codex", "opencode", "pi", "cursor-agent", "grok"]:
            (bin_dir / name).symlink_to(version)
        registry = home / ".agents/skill-studio.json"
        saved = json.loads(registry.read_text()) if registry.exists() else {}
        saved.update(version=4, projects={"added": fixture["projects"]},
                     harnesses={"kept": ["claude-code"], "search_project_folders": True,
                                "saved_at": "2026-01-01T00:00:00Z"})
        registry.parent.mkdir(exist_ok=True)
        registry.write_text(json.dumps(saved))
        (home / ".zshrc").write_text('export PATH="$SKILL_STUDIO_FIXTURE_PATH"\nexport EDITOR=vi\n')
        env = os.environ.copy()
        for key in list(env):
            if key.startswith(("SKILL_STUDIO_", "XDG_", "OPENCODE_", "CODEX_", "CLAUDE_")):
                del env[key]
        env.update(HOME=str(home), SKILL_STUDIO_FIXTURE=str(home),
                   SKILL_STUDIO_HOME=str(home), SKILL_STUDIO_TELEMETRY="0",
                   SKILL_STUDIO_PERF_BIN=str(bin_dir),
                   SKILL_STUDIO_FIXTURE_PATH=os.pathsep.join([str(bin_dir), "/usr/bin", "/bin"]),
                   XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".data"),
                   XDG_CACHE_HOME=str(home / ".cache"), CODEX_HOME=str(home / ".codex"),
                   ZDOTDIR=str(home), SHELL="/bin/zsh")
        cache = home / ".data/skill-studio/cache/version-probes.json"
        def sample():
            observed = {"load_average": read("/usr/sbin/sysctl", "-n", "vm.loadavg"),
                        "cargo_processes": len(read("/usr/bin/pgrep", "-x", "cargo").splitlines()),
                        "rustc_processes": len(read("/usr/bin/pgrep", "-x", "rustc").splitlines())}
            started = time.perf_counter_ns()
            result = subprocess.run([str(binaries["desktop"])], env=env, cwd=home, capture_output=True,
                                    text=True, timeout=120, check=False)
            round_trip = (time.perf_counter_ns() - started) / 1e6
            if result.returncode:
                raise RuntimeError(result.stderr)
            data = json.loads(result.stdout)
            if data["agents"] != report["fixture"]["agents"]:
                raise RuntimeError("fixture agent count changed")
            return {**observed, "detection_ms": data["elapsed_ms"], "native_round_trip_ms": round_trip}
        sample()
        for phase in ["cache_miss", "cache_hit"]:
            rows = []
            for _ in range(10):
                if phase == "cache_miss":
                    cache.unlink(missing_ok=True)
                rows.append(sample())
            report["phases"][phase] = {"samples": rows,
                **{key: distribution(rows, key) for key in ["detection_ms", "native_round_trip_ms"]}}
        if not cache.is_file():
            raise RuntimeError("desktop did not persist its probe cache")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print("| Phase | Metric | N | Median ms | p95 ms | Max ms |\n| --- | --- | ---: | ---: | ---: | ---: |")
    for phase, data in report["phases"].items():
        for key in ["detection_ms", "native_round_trip_ms"]:
            stats = data[key]
            print(f"| {phase} | {key} | {stats['n']} | {stats['median']:.3f} | {stats['p95']:.3f} | {stats['max']:.3f} |")


if __name__ == "__main__":
    main()
