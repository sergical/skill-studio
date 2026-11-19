"""Write-job timings: pack add, pack update, park and unpark of N skills from one
pack, through the built CLI and the real `npx skills`, against a local
smart-HTTP git server (`git http-backend`), so no number depends on GitHub.

Run by hand, like `perf:baseline`; nothing in CI reads the result:

    cargo build --workspace --all-features --bins
    python3 tools/perf-writes.py --cli target/debug/skill-studio --label "$(git rev-parse --short HEAD)"

Each run uses a new temporary home whose npm cache is warmed before its first
timed job, so `npx` does not download the skills package inside a sample.
Upstream changes before each `pack update`, and the script checks that the new
content arrived, so the update really fetches and writes. A wrapper on PATH
counts the `npx` calls of each job: the count is the deterministic part (see
crates/skill-studio-core/tests/write_spawn_count.rs); the times include
process start, git over loopback and the skills CLI.
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


def clean_env(home):
    """The caller's environment without anything that points git or npm at
    the user's own repositories, config or cache, with HOME set to `home`."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("GIT_", "SKILL_STUDIO_", "XDG_", "CODEX_", "CLAUDE_"))
           and not key.lower().startswith("npm_config_")}
    env.update(HOME=str(home), GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
               GIT_TERMINAL_PROMPT="0")
    return env


def cli_npm_env(home, env):
    """`env` with the npm and git settings the CLI gives `npx` for `--home`
    (skill-studio-host `RealProcessSpawner::isolate_environment`), so a
    warm-up fills the cache the timed jobs read."""
    env = dict(env)
    for name, path in [("cache", home / ".npm"), ("userconfig", home / ".npmrc"),
                       ("globalconfig", home / ".npm-global/etc/npmrc"),
                       ("prefix", home / ".npm-global")]:
        env[f"npm_config_{name}"] = str(path)
        env[f"NPM_CONFIG_{name.upper()}"] = str(path)
    env.update(HOME=str(home), GIT_CONFIG_GLOBAL=str(home / ".gitconfig"),
               GIT_CONFIG_NOSYSTEM="1")
    return env


def git(env, *args):
    return subprocess.run(["git", "-c", "user.name=bench", "-c", "user.email=bench@example.invalid",
                           "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args],
                          env=env, check=True, capture_output=True, text=True).stdout.strip()


def make_pack(root, count):
    """A bare git repo with `count` skills under skills/<name>/SKILL.md, made
    with git isolated from the user's config, hooks and repositories."""
    env = clean_env(root / "setup-home")
    (root / "setup-home").mkdir(parents=True, exist_ok=True)
    work = root / "work"
    names = [f"bench-skill-{i:02d}" for i in range(count)]
    for name in names:
        folder = work / "skills" / name
        folder.mkdir(parents=True)
        (folder / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: Write-job benchmark skill {name}.\n---\nRevision 0.\n")
    git(env, "init", "-q", "-b", "main", str(work))
    git(env, "-C", str(work), "add", "-A")
    git(env, "-C", str(work), "commit", "-q", "-m", "revision 0")
    bare = root / "srv" / "pack.git"
    git(env, "clone", "-q", "--bare", str(work), str(bare))
    git(env, "-C", str(bare), "config", "http.receivepack", "false")
    return names


def bump_pack(root, names, revision):
    """Commits `Revision <revision>.` into every skill and pushes it to the
    served repo, so the next update has new content to fetch."""
    env = clean_env(root / "setup-home")
    work = root / "work"
    for name in names:
        (work / "skills" / name / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: Write-job benchmark skill {name}.\n---\n"
            f"Revision {revision}.\n")
    git(env, "-C", str(work), "commit", "-q", "-am", f"revision {revision}")
    git(env, "-C", str(work), "push", "-q", str(root / "srv" / "pack.git"), "main")


def skills_folder_hashes(home, names):
    """Each installed skill folder's hash as `npx skills@1.8.1` computes it
    for a git source (`computeSkillFolderHash`, which `update` compares),
    taken from the skills package in `home`'s npm cache, so it cannot drift
    from the CLI's own rule."""
    chunks = [p for p in (home / ".npm" / "_npx").glob("*/node_modules/skills/dist/_chunks/*.mjs")
              if "computeSkillFolderHash," in p.read_text(errors="ignore")]
    if not chunks:
        sys.exit(f"no {SKILLS_CLI} with computeSkillFolderHash in {home / '.npm'}")
    script = ("const { computeSkillFolderHash } = await import(process.argv[1]);"
              "const out = {};"
              "for (const dir of process.argv.slice(2)) out[dir] = await computeSkillFolderHash(dir);"
              "console.log(JSON.stringify(out));")
    folders = [str(home / ".agents" / "skills" / name) for name in names]
    out = subprocess.run(["node", "--input-type=module", "-e", script, chunks[0].as_uri(), *folders],
                         check=True, capture_output=True, text=True).stdout
    by_folder = json.loads(out)
    return {name: by_folder[folder] for name, folder in zip(names, folders)}


def serve_git(projects_root):
    """Starts `git http-backend` behind a loopback HTTP server; returns it and its base URL."""
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
                "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
                "PATH_INFO": path, "QUERY_STRING": query,
                "REQUEST_METHOD": self.command,
                "CONTENT_TYPE": self.headers.get("Content-Type", ""),
                "CONTENT_LENGTH": str(len(body)), "REMOTE_ADDR": "127.0.0.1",
                "GIT_PROTOCOL": self.headers.get("Git-Protocol", ""),
                "HTTP_CONTENT_ENCODING": self.headers.get("Content-Encoding", ""),
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


def warm_cache(home, template, env):
    """Fills `home/.npm` with the skills package: once into `template` with the
    CLI's npm settings, then a copy for each later home."""
    if not (template / ".npm").exists():
        template.mkdir(parents=True, exist_ok=True)
        subprocess.run(["npx", "-y", SKILLS_CLI, "--version"], env=cli_npm_env(template, env),
                       check=True, capture_output=True)
    shutil.copytree(template / ".npm", home / ".npm", symlinks=True)


def write_lock_rows(home, names, source, hashes):
    """`npx skills@1.8.1` writes no lock row for a loopback git URL (checked for
    both 127.0.0.1 and localhost), so nothing would track the pack. Writes the
    rows it writes for a remote git source, with each folder's hash as skills
    computes it: skills skips a row whose hash is empty instead of checking
    it, and reinstalls a row whose hash does not match."""
    lock = home / ".agents" / ".skill-lock.json"
    doc = json.loads(lock.read_text()) if lock.exists() else {"version": 3, "skills": {}}
    for name in names:
        doc["skills"][name] = {
            "source": source, "sourceType": "git", "sourceUrl": source,
            "skillPath": f"skills/{name}/SKILL.md", "skillFolderHash": hashes[name],
            "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
        }
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
    if envelope.get("status") != "ok":
        sys.exit(f"{' '.join(args[:2])} ended with status {envelope.get('status')}: "
                 f"{envelope.get('errors')}")
    return elapsed, calls


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
        env = clean_env(root / "unused-home")
        env.update(PATH=f"{root / 'bin'}{os.pathsep}{env['PATH']}", SKILL_STUDIO_TELEMETRY="0")

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
            warm_cache(home, root / "warm-template", env)
            for job, job_args in jobs.items():
                if job == "pack_update":
                    bump_pack(root, names, run + 1)
                elapsed, calls = run_job(cli, job_args, home, env, log)
                samples[job].append({"ms": round(elapsed, 1), "npx_calls": calls})
                if job == "pack_add":
                    write_lock_rows(home, names, source, skills_folder_hashes(home, names))
                if job == "pack_update":
                    body = (home / ".agents/skills" / names[0] / "SKILL.md").read_text()
                    if f"Revision {run + 1}." not in body:
                        sys.exit(f"run {run}: pack update did not fetch the new revision:\n{body}")
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
