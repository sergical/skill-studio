import argparse
import hashlib
import json
import math
import os
import pathlib
import platform
import statistics
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument("binary")
parser.add_argument("volume")
parser.add_argument("output")
parser.add_argument("--runs", type=int, default=10)
args = parser.parse_args()
assert args.runs >= 10
binary = str(pathlib.Path(args.binary).resolve())


def call(command_args, home):
    env = dict(
        os.environ,
        HOME=str(home),
        SKILL_STUDIO_HOME=str(home),
        SKILL_STUDIO_TELEMETRY="0",
    )
    result = subprocess.run(
        [binary, *command_args, "--home", str(home), "--json"],
        env=env,
        capture_output=True,
        text=True,
    )
    data = json.loads(result.stdout)
    assert result.returncode == 0, (command_args, data, result.stderr)
    return data


report = {
    "hardware": subprocess.check_output(
        ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
    ).strip(),
    "platform": platform.platform(),
    "runs": args.runs,
    "binary": binary,
    "binary_sha256": hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),
    "fixture": (
        "Cross-volume project Codex copies; SKILL.md, README.md and scripts/run.sh "
        "per skill; 0750 skill root, 0710 scripts directory, 0751 executable. "
        "Real CLI, fresh HOME and history per sample."
    ),
    "results": [],
}
for count in [1, 100]:
    durations = {"park": [], "unpark": []}
    for run in range(args.runs):
        with (
            tempfile.TemporaryDirectory(prefix="park-bench-home-") as home_dir,
            tempfile.TemporaryDirectory(
                prefix="park-bench-project-", dir=args.volume
            ) as project_dir,
        ):
            home = pathlib.Path(home_dir).resolve()
            project = pathlib.Path(project_dir).resolve()
            assert home.stat().st_dev != project.stat().st_dev
            (project / ".git").mkdir()
            original = {}
            for n in range(count):
                name = f"bench-{n:03d}"
                root = project / ".codex/skills" / name
                (root / "scripts").mkdir(parents=True)
                content = {
                    "SKILL.md": (
                        f"---\nname: {name}\ndescription: Park timing fixture\n"
                        "---\nFixture.\n"
                    ),
                    "README.md": "Reference material.\n" * 16,
                    "scripts/run.sh": "#!/bin/sh\nexit 0\n",
                }
                for relative, body in content.items():
                    (root / relative).write_text(body)
                root.chmod(0o750)
                (root / "scripts").chmod(0o710)
                (root / "scripts/run.sh").chmod(0o751)
                original[name] = content
            for operation in ["park", "unpark"]:
                started = time.perf_counter()
                for name in original:
                    call([operation, name, "--project", str(project)], home)
                durations[operation].append((time.perf_counter() - started) * 1000)
            modes = {}
            for name, files in original.items():
                root = project / ".codex/skills" / name
                for relative, body in files.items():
                    assert (root / relative).read_text() == body, (name, relative)
                modes = {
                    "root": oct(root.stat().st_mode & 0o7777),
                    "scripts": oct((root / "scripts").stat().st_mode & 0o7777),
                    "file": oct((root / "scripts/run.sh").stat().st_mode & 0o7777),
                }
            print(f"{count} skills sample {run + 1}/{args.runs}", flush=True)
    report["results"].append({
        "skills": count,
        "modes_after_roundtrip": modes,
        **{
            operation: {
                "median_ms": statistics.median(values),
                "p95_ms": sorted(values)[math.ceil(0.95 * len(values)) - 1],
                "samples_ms": values,
            }
            for operation, values in durations.items()
        },
    })
    pathlib.Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
