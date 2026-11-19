---
name: verify-skill-studio
description: Drive Skill Studio the way a user does to prove a change works - the CLI against a disposable home with the real `npx skills` and a local git pack, and the desktop UI on the dev harness. Use before you report a Skill Studio change as done, when a PR would otherwise say "not checked: real npx skills runs", or to reproduce a bug in add, update, park, undo or packs.
---

# Verify Skill Studio

Skill Studio changes skills on disk through `npx skills` and dotagents. Unit
tests use an in-memory file system and fake process spawners, so they cannot
show that a real `npx skills` call does what the core expects. This skill runs
the built CLI and the real `npx skills@1.8.1` in a temporary home, against a
local pack served over smart HTTP, and keeps the evidence.

Surfaces:

- **CLI (`skill-studio`)**: the real core, the real `npx`, real files. Primary.
- **Desktop UI**: the React app on the dev harness (`src/dev/harness`), driven
  with Playwright. The harness mocks the Rust backend, so a UI run proves the
  UI flow, not the backend. The real Tauri window (WKWebView) has no
  automation port; no recipe here drives it. Say so in a PR.
- **MCP server**: the same core ops as the CLI. `apps/cli/tests/pack.rs`
  checks that both give the same result.

## Launch

```bash
S=.agents/skills/verify-skill-studio/scripts
$S/control-skill-studio launch --skills 5
```

It builds `target/debug/skill-studio` when missing (one
`cargo build --workspace --all-features --bins`), makes a run folder under
`${TMPDIR}/skill-studio-verify/` (override with `SKILL_STUDIO_VERIFY_ROOT`),
starts a pack server with 5 skills (`bench-skill-00` to `bench-skill-04`),
puts an `npx` wrapper that logs each call first on `PATH`, and warms the run's
npm cache. Ready when it prints the run folder. `SKILL_STUDIO_VERIFY_CLI`
selects another built CLI, such as one built from `main` for a before run.

One run at a time per `SKILL_STUDIO_VERIFY_ROOT`: `launch` replaces the
current run pointer. Use another root for a second run in parallel.

## Doctor

```bash
$S/control-skill-studio doctor
```

Read-only. It checks that the pack server process is running and the pack
answers `git ls-remote`, and it names the CLI build, the home and the evidence
folder. Run it first whenever a result looks wrong. If it fails, clean up and
launch again.

## Drive

```bash
url=$(cat "$(cat "${TMPDIR}/skill-studio-verify/current")/pack-url")
$S/control-skill-studio cli -- pack add "$url" --skill bench-skill-00 --skill bench-skill-01
$S/control-skill-studio state
$S/control-skill-studio track      # before pack list / pack update / outdated
$S/control-skill-studio cli -- pack update --all
$S/control-skill-studio cli -- restore --event-id <id from a step's state.event_id>
```

`cli -- <args>` runs `skill-studio <args> --home <run home> --json` and prints
the exit code, the number of `npx` calls, and the JSON. `state` lists the
skill folders, lock rows and journal. `track` writes the lock rows that
`npx skills` does not write for a loopback git URL (see Gotchas in
[features/add-from-repo.md](features/add-from-repo.md)).

For the desktop UI, run its e2e specs or drive the dev harness with
Playwright; see [features/desktop-harness.md](features/desktop-harness.md).

## Evidence

Each `cli` call saves `NN-<command>.cmd` (the exact command), `.json`
(stdout), `.stderr`, `.exit` and `.npx` (one line per `npx` call) in the run's
`evidence/` folder. Proof standards:

- Drive the user path: the CLI command or the UI control a user touches, not
  an internal op or a test-only hook.
- Prove the action and the resulting state: the command's JSON and a second,
  read-only view (`state`, `scan`, `events`, `pack list`).
- Check side effects: folders under `home/.agents/skills`, lock rows, journal
  rows, and the `npx` call count.
- For a speed claim, give a count or a time from evidence, and the build
  (`SKILL_STUDIO_VERIFY_CLI`) it came from. `tools/perf-writes.py` times write
  jobs over several runs.
- A dry run (`--dry-run`) must change nothing: check that the home did not change.

## Cleanup

```bash
$S/control-skill-studio cleanup
```

It stops the pack server this run started (by its recorded PID, never by
name), deletes the home, npm cache, wrapper and pack, and keeps `evidence/`,
`npx.log` and `server.log`. It prints where the evidence is. Delete old run
folders under `${TMPDIR}/skill-studio-verify/` yourself once a PR has its
proof.

## Helpers

- `scripts/control-skill-studio` - launch, doctor, cli, track, state, cleanup.
- `scripts/serve_pack.py` - the pack server `launch` starts; it reuses
  `tools/perf-writes.py`, so the benchmark and this skill serve the same pack.

## Feature map

[features/README.md](features/README.md) lists the mapped journeys. Keep it
true as the app changes: a new user flow in `docs/flows.md` gets a feature
file here.
