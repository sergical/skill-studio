"""Write-job timings: pack add, pack update, park and unpark of N skills from one
pack, through the built CLI and the real `npx skills`, against a local
smart-HTTP git server (`git http-backend`), so no number depends on GitHub.

Run by hand, like `perf:baseline`; nothing in CI reads the result:

    cargo build --workspace --all-features --bins
    python3 tools/perf-writes.py --cli target/debug/skill-studio --label "$(git rev-parse --short HEAD)"

Each run uses a new temporary home. One shared npm cache is warmed before the
first timed run, so `npx` does not download the skills package inside a
sample. A wrapper on PATH counts the `npx` calls of each job: the count is the
deterministic part (see crates/skill-studio-core/tests/write_spawn_count.rs);
the times include process start, git over loopback and the skills CLI.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time

SKILLS_CLI = "skills@1.8.1"


def make_pack(root, count):
    """A bare git repo with `count` skills under skills/<name>/SKILL.md."""
    work = root / "work"
    names = [f"bench-skill-{i:02d}" for i in range(count)]
    for name in names:
        folder = work / "skills" / name
        folder.mkdir(parents=True)
        (folder / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: Write-job benchmark skill {name}.\n---\nBody.\n")
    git = ["git", "-c", "user.name=bench", "-c", "user.email=bench@example.invalid"]
    subprocess.run([*git, "init", "-q", "-b", "main", str(work)], check=True)
    subprocess.run([*git, "-C", str(work), "add", "-A"], check=True)
    subprocess.run([*git, "-C", str(work), "commit", "-q", "-m", "pack"], check=True)
    bare = root / "srv" / "pack.git"
    subprocess.run(["git", "clone", "-q", "--bare", str(work), str(bare)], check=True)
    subprocess.run(["git", "-C", str(bare), "config", "http.receivepack", "false"], check=True)
    return names


def serve_git(projects_root):
    """Starts `git http-backend` behind a loopback HTTP server; returns its base URL."""
    exec_path = subprocess.check_output(["git", "--exec-path"], text=True).strip()
    backend = f"{exec_path}/git-http-backend"

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def handle_one(self):
            path, _, query = self.path.partition("?")
            length = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(length) if length else b""
            env = {
                "GIT_PROJECT_ROOT": str(projects_root), "GIT_HTTP_EXPORT_ALL": "1",
                "PATH_INFO": path, "QUERY_STRING": query,
                "REQUEST_METHOD": self.command,
                "CONTENT_TYPE": self.headers.get("Content-Type", ""),
                "CONTENT_LENGTH": str(len(body)), "REMOTE_ADDR": "127.0.0.1",
                "GIT_PROTOCOL": self.headers.get("Git-Protocol", ""),
                "PATH": os.environ["PATH"],
            }
            out = subprocess.run([backend], input=body, env=env, capture_output=True,
                                 check=False).stdout
            head, _, payload = out.partition(b"\r\n\r\n")
            status = 200
            headers = []
            for line in head.decode().split("\r\n"):
                key, _, value = line.partition(":")
                if key.lower() == "status":
                    status = int(value.strip().split()[0])
                elif key:
                    headers.append((key, value.strip()))
            self.send_response(status)
            for key, value in headers:
                self.send_header(key, value)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        do_GET = handle_one
        do_POST = handle_one

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_address[1]}"


def npx_wrapper(bin_dir, log):
    """An `npx` first on PATH that logs each call, then runs the real one."""
    real = shutil.which("npx")
    if real is None:
        sys.exit("npx not found on PATH; install Node.js")
    bin_dir.mkdir(parents=True)
    wrapper = bin_dir / "npx"
    wrapper.write_text(f'#!/bin/sh\necho "$*" >> "{log}"\nexec "{real}" "$@"\n')
    wrapper.chmod(0o755)


def write_lock_rows(home, names, source):
    """`npx skills` writes no lock row for a loopback git URL (checked with
    skills@1.8.1 for both 127.0.0.1 and localhost), so nothing would track the
    pack. Writes the rows it writes for a remote git source, so `pack update`
    finds the pack and `npx skills update` fetches it from the local server."""
    lock = home / ".agents" / ".skill-lock.json"
    doc = json.loads(lock.read_text()) if lock.exists() else {"version": 3, "skills": {}}
    for name in names:
        doc["skills"].setdefault(name, {
            "source": source, "sourceType": "git", "sourceUrl": source,
            "skillPath": f"skills/{name}/SKILL.md", "skillFolderHash": "",
            "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
        })
    lock.write_text(json.dumps(doc, indent=2))


def run_job(cli, args, home, env, log):
    log.write_text("")
    start = time.monotonic()
    done = subprocess.run([str(cli), *args, "--home", str(home), "--json"], env=env,
                          capture_output=True, text=True, check=False)
    elapsed = (time.monotonic() - start) * 1000
    try:
        envelope = json.loads(done.stdout)
    except json.JSONDecodeError:
        sys.exit(f"{' '.join(args)} printed no JSON:\n{done.stdout}\n{done.stderr}")
    calls = len([line for line in log.read_text().splitlines() if line.strip()])
    steps = (envelope.get("data") or {}).get("steps")
    if steps is not None and not steps:
        sys.exit(f"{' '.join(args[:2])} planned no step: {envelope.get('data')}")
    return elapsed, calls, envelope.get("status")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cli", type=Path, required=True, help="built skill-studio CLI")
    parser.add_argument("--skills", type=int, default=5)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--out", type=Path, default=Path("docs/perf/writes.json"))
    parser.add_argument("--label", required=True,
                        help="what the CLI was built from, such as a commit SHA")
    args = parser.parse_args()
    cli = args.cli.resolve()

    with tempfile.TemporaryDirectory(prefix="skill-studio-writes-") as tmp:
        root = Path(tmp)
        names = make_pack(root, args.skills)
        server, base = serve_git(root / "srv")
        source = f"{base}/pack.git"
        log = root / "npx.log"
        npx_wrapper(root / "bin", log)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("SKILL_STUDIO_", "XDG_", "CODEX_", "CLAUDE_"))}
        env.update(PATH=f"{root / 'bin'}{os.pathsep}{env['PATH']}",
                   npm_config_cache=str(root / "npm-cache"), SKILL_STUDIO_TELEMETRY="0")
        subprocess.run(["npx", "-y", SKILLS_CLI, "--version"], env=env, check=True,
                       capture_output=True)

        skill_flags = [flag for name in names for flag in ("--skill", name)]
        jobs = {
            "pack_add": ["pack", "add", source, *skill_flags],
            "pack_update": ["pack", "update", "--all"],
            "park": ["park", *names],
            "unpark": ["unpark", *names],
        }
        samples = {job: [] for job in jobs}
        for run in range(args.runs):
            home = root / f"home-{run}"
            home.mkdir()
            for job, job_args in jobs.items():
                elapsed, calls, status = run_job(cli, job_args, home, env, log)
                if status != "ok":
                    sys.exit(f"run {run}: {job} ended with status {status}")
                samples[job].append({"ms": round(elapsed, 1), "npx_calls": calls})
                if job == "pack_add":
                    write_lock_rows(home, names, source)
        server.shutdown()

    result = {
        "skills": args.skills,
        "runs": args.runs,
        "skills_cli": SKILLS_CLI,
        "cli": str(args.cli),
        "label": args.label,
        "machine": f"{platform.system()} {platform.release()} {platform.machine()}",
        "jobs": {
            job: {
                "median_ms": statistics.median(s["ms"] for s in rows),
                "max_ms": max(s["ms"] for s in rows),
                "npx_calls": sorted({s["npx_calls"] for s in rows}),
                "samples": rows,
            }
            for job, rows in samples.items()
        },
    }
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    lines = [f"{args.label}: {args.skills} skills from one local pack, {args.runs} runs, "
             f"{SKILLS_CLI}.", "",
             "| Job | Median | Max | `npx` calls |", "| --- | --: | --: | --: |"]
    for job, row in result["jobs"].items():
        calls = ", ".join(str(c) for c in row["npx_calls"])
        lines.append(f"| {job} | {row['median_ms']:.0f} ms | {row['max_ms']:.0f} ms | {calls} |")
    args.out.with_suffix(".md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
