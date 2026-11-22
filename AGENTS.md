# AGENTS.md - Skill Studio

## Project Overview

Skill Studio discovers and manages agent skills in a Tauri 2 desktop app
(React, TypeScript, Tailwind CSS and Zustand), a Rust CLI and a local MCP
server. The surfaces share core operations. The host crate provides
filesystem, process and other adapters.

## Build & Development Commands

```bash
# Install dependencies
pnpm install

# Development mode (starts Vite + Tauri)
pnpm run tauri dev

# Build for production
pnpm run tauri build

# Frontend only (Vite dev server)
pnpm run dev

# Type check + build frontend
pnpm run build

# Preview built frontend
pnpm run preview
```

### Rust Commands

Run from the workspace root with the pinned toolchain:

```sh
cargo build --workspace --all-features --bins
cargo test --workspace --all-features
cargo check --workspace --all-features
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

### Mutation Testing (`cargo mutants`)

CI runs `cargo mutants` on `crates/skill-studio-core` and `crates/skill-studio-host`
only: `--in-diff` against the PR's changed lines on every pull request
(report-only for now, `.github/workflows/rust.yml`'s `mutants-changed` job),
and a full, sharded run of both crates weekly
(`.github/workflows/mutants.yml`, `schedule` + `workflow_dispatch`).
Excludes live in `.cargo/mutants.toml`.

A full run is slow; do not run it locally. To check one file before pushing:

```bash
# Install once
cargo install cargo-mutants --locked

# Mutate a single module (swap the path and -p for the crate you touched)
cargo mutants -p skill-studio-core --all-features --file crates/skill-studio-core/src/registry.rs

# Read the survivors this run found
cat mutants.out/missed.txt
```

A surviving mutant is a question, not a task. Add a test only when the
mutant changes what a user sees or what lands on disk, and the test checks
that outcome. Otherwise add an exclude to `.cargo/mutants.toml` with the
reason.

### Worktrees and Disk Use

A shared `target/` grew to 156 GB in one night of parallel worktree builds
(measured 2026-11-10): incremental caches took 72 GB, macOS debug `.o` files
56 GB, and each `-p` selection, toolchain or `RUSTFLAGS` value a full copy. A
shared folder also let one worktree's build pass as fresh in another. These
rules are firm:

- **One build folder per worktree, outside the checkout.** Put this in
  `~/.cargo/config.toml` once per machine. It applies to every cargo call, so
  agents need no env vars:

  ```toml
  [build]
  build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"
  incremental = false
  [profile.test]
  debug = false
  ```

  Intermediate files go to `~/.cargo/build/<xx>/<hash>/`, one folder per
  worktree, about 4 to 5 GB each. Final binaries (`target/debug/skill-studio`)
  stay in the worktree's own `target/` (107 MB measured) and go with
  `git worktree remove`. Do not set `CARGO_BUILD_BUILD_DIR` or `--target-dir`,
  and do not set `CARGO_TARGET_DIR` except for `pnpm run perf:baseline` (see
  `docs/perf/README.md`): worktrees that share a `target/` overwrite each
  other's final binaries.

- **Delete old build folders automatically.** Cargo never deletes them.
  `tools/cargo-build-gc.pl` empties whole worktree folders under
  `~/.cargo/build/`, least recently used first, above a 40 GB total, below
  30 GB free disk, or after 3 days. It never empties a folder used in the last
  hour, a folder whose lock Cargo holds, or a folder that a process runs from.
  Cargo releases its lock before tests run, so the script checks the process
  list 20 times over 2 seconds: one check missed a running `cargo test` 24% of
  the time (gaps between test binaries), two checks in a row never did
  (measured). It holds the folder's locks while it checks and deletes, and
  keeps the lock files. While any Cargo process is stopped (Ctrl-Z, a
  debugger), it empties nothing. Known gap: a `cargo test` that gets no CPU
  for over 2 seconds between two test binaries can lose its folder; that run
  fails and the next one rebuilds. The cap is an eviction threshold: builds
  that run at the same time can go over it. Run it every 15 minutes with
  launchd (macOS):

  ```bash
  install -m 755 tools/cargo-build-gc.pl ~/.local/bin/cargo-build-gc
  # LaunchAgent: ProgramArguments = absolute path to cargo-build-gc
  # (launchd does not expand ~), StartInterval 900
  ```

  `pnpm run clean:target` runs it once. `CARGO_BUILD_GC_DRY_RUN=1` shows what
  it would empty and changes nothing.

- **Clean up when the work is done.** When a worktree's PR merges or closes, or
  the session ends with its work pushed, run `git worktree remove <path>`. If
  the worktree has uncommitted changes, keep it, delete its `target/` and
  `node_modules/`, and tell the user it is still there.
- **No stray copies.** Do not clone the repo or run `pnpm install` in `.scratch/`, the
  scratchpad or `/tmp` unless the task needs it, and delete the copy in the same
  session.
- **Check before you report.** Before the final message of a session that
  created worktrees or builds, run
  `du -sh .claude/worktrees .scratch 2>/dev/null` and say what is left and why.
- **Build with one command shape.** Each different package selection,
  toolchain, or flag set gets its own full copy: measured on 2026-11-10,
  `-p skill-studio-core` added 1.0 GB, `-p core -p host -p cli` 1.5 GB, and
  `RUSTC_WORKSPACE_WRAPPER` 1.8 GB on top of the 4.2 GB that
  `--workspace --all-features` tests plus clippy take. Always pass
  `--workspace --all-features` and narrow with `--test <file>` or a test-name
  filter, which adds nothing. Use the pinned toolchain. Never set `RUSTFLAGS`,
  `RUSTC_WRAPPER`, or `RUSTC_WORKSPACE_WRAPPER`.
- **Stay above the free-disk floor.** Do not start a build below 30 GB free.
  Run at most 2 Rust builds at once.

### Frontend Commands

```bash
# Type check only (no emit)
pnpm run typecheck

# Lint (bun lint, reads .oxlintrc.json and runs the anti-slop plugin)
pnpm run lint
pnpm run lint:fix

# Format (bun fmt, reads .oxfmtrc.json)
pnpm run format
pnpm run format:check

# React health check (react-doctor)
pnpm run doctor

# Full gate (frontend checks plus workspace Rust checks and tests)
pnpm run check
```

### Versions

The app, CLI, MCP server, npm packages and workspace packages share one version, taken from `[workspace.package]` in the root `Cargo.toml`. Change it only with `pnpm run version:set <major.minor.patch>`, which writes every manifest and `Cargo.lock`. `pnpm run versions:check` fails on drift; CI runs it on every PR, and `release.yml` runs it against the tag.

## Project Structure

Top-level workspaces only. Update this tree when a workspace is added or
removed; do not list individual source files here.

```
/
├── apps/
│   ├── cli/                      # Rust CLI, thin adapter over skill-studio-core
│   ├── desktop/                  # Tauri app (npm package "skill-studio")
│   │   ├── src/                  # React frontend: components/, hooks/, lib/, store/
│   │   └── src-tauri/src/skills/ # Rust backend: scan, lifecycle, Tauri commands
│   ├── mcp/                      # Rust MCP server over skill-studio-core, stdio
│   └── server/                   # skills.sh proxy (Node dev server and Cloudflare Worker)
├── crates/
│   ├── skill-studio-core/        # Scope-confined core: scan, ops, events, DTOs, schema/
│   └── skill-studio-host/        # Real-world port adapters for the core crate
├── packages/
│   ├── lib/                      # Shared TypeScript library
│   ├── marketing/                # Marketing site (Vite + StyleX) and Remotion walkthroughs
│   ├── npm/                      # npm packages for the CLI, outside the pnpm workspace
│   └── ui/                       # Shared UI primitives (@skill-studio/ui)
├── docs/                         # Specs and reference docs
├── tools/
│   └── oxlint/anti-slop/         # Local oxlint JS plugin
└── pnpm-workspace.yaml           # pnpm workspace
```

## Code Style Guidelines

- Use form controls and buttons from [@skill-studio/ui](packages/ui/src/index.ts).
  Raw form elements belong inside that package.
- Group imports by React, external libraries, internal modules and types.
  Use explicit prop interfaces and type guards. Keep hooks prefixed with
  `use` and use selectors on the [Zustand store](apps/desktop/src/store/appStore.ts).
- Name components and types in PascalCase, variables and hooks in camelCase,
  constants in SCREAMING_SNAKE_CASE and Rust files in snake_case. Use
  domain-specific file names. Keep `index.ts` to re-exports.
- Prefer Tailwind utilities and CSS variables for theme colors. Catch UI
  action errors and show a toast.
- Follow [core error rule D7](docs/spec-core-primitives.md).
  Use typed errors and stable codes for shared operations. Return their
  result envelopes from adapters. Some desktop IPC commands still return
  strings. Keep their existing contract when editing unrelated code.

### Testing

- Rust: tests live in colocated `#[cfg(test)]` modules and crate-level `tests/` directories.
- Frontend: Vitest is configured; run `pnpm run test`
- End to end: `pnpm --filter skill-studio run test:e2e` runs the whole app on
  the dev harness (`src/dev/harness`) in WebKit (the Tauri engine on macOS)
  and Chromium, with real key presses and clicks. Specs live in
  `apps/desktop/e2e/*.e2e.ts`. A change to keyboard shortcuts, focus, menus
  or dialogs, or a fix for a bug that shows only across components, adds or
  updates a spec there, and the spec must fail without the change. Specs find
  controls by role and accessible name and check the outcome (the list, the
  switch state, the page), not copy or CSS. Component tests that set
  the store directly do not prove these flows: a window key handler in
  another component broke ⌘K while they all passed.
- Real CLI runs: the e2e harness and the Rust tests mock `npx skills`. To
  prove a change against the real `npx skills` and real files, use the
  `verify-skill-studio` skill (`.agents/skills/verify-skill-studio/`): a
  disposable home, a local git pack, and saved evidence for each command.

## Before you implement an issue

Review findings and follow-up issues often ask for a guarantee in one rare
case. If you implement each case as written, you get fixes on top of fixes:
the Park chain #485, #487 and #491 reached 11,000 lines and was closed.
The shared guard proposed in #531 was not built; #531 was also closed. Before you write code for an issue:

1. Write the user need in one sentence. Find the cause of the problem and fix
   it there. The test: will the next feature need another exception for the
   same problem? The next agent copies the code it finds, so a workaround
   spreads. A small diff can be the right root-cause fix, and a large diff can
   avoid the cause. Keep each commit to one vertical slice. If the issue asks
   for more than the need (for example byte-exact output), implement the need
   and say why in the PR.
2. Check that the approach follows the rules in
   `docs/agent-skill-conventions.md`. The main rule: the tool that owns a file
   writes it. Skill Studio runs `npx skills` and `npx @sentry/dotagents`. It
   does not hand-edit `agents.toml`, `agents.lock`, `.gitignore` or
   `.skill-lock.json`.
3. For edits that happen at the same time, use one shared guard: read, keep
   the hash, check again before the write, and refuse on change. Do not add a guard for one case only.
4. A rare race or a rare layout becomes a follow-up issue, not part of the PR.
5. If the change grows past about 500 lines, or a fix needs a fix, stop and
   write a design note in the issue before you continue.

## How a PR lands

1. Commit. The pre-commit hook checks the format and lint of what the commit
   will contain (under 1 s). It changes nothing; a failure names its fix
   (`pnpm run format`, `cargo fmt`).
2. Push. The pre-push hook runs `pnpm run check:fast` (about 1 minute). When
   Rust changed, it runs `pnpm run check` and the contract snapshots instead
   (about 6 minutes). When a workflow changed, it also runs zizmor. Fix what
   fails. Do not use `--no-verify`.
3. Open the PR with `.github/PULL_REQUEST_TEMPLATE.md`, then run
   `gh pr merge --auto --squash`.
4. Get a review from a model of another provider and post it with
   `scripts/gate/gate review-status` (see `scripts/gate/README.md`). Each push
   needs a new review.
5. GitHub merges when the required checks pass. Watch the PR only to fix a
   failing check.
6. `gate/risk` adds the `risk:owner` label and stays pending for: a release, a
   version change, deploy or secret files, a new dependency, a change to
   `package.json` scripts, workflows, `.githooks/`, `.gate/` or `deny.toml`, and
   policy files. The rules
   are in `.gate/config.toml`. The owner merges these PRs. The workflow posts a
   `Needed from you and why` comment on the PR: the decision, why a person must
   make it, and the files and rule that matched. Start your reply with
   `Needs you: #<number> <the one decision>`. End it with that section: link
   the PR, copy the comment, and add your pick and why. Then stop. Do not wait
   for the merge, use `--admin` without the owner's go-ahead in the chat, or
   remove the label.
7. Review rounds follow the `diff-review` skill. If the review still fails
   after 2 rounds of fixes, stop and start your reply with `Needs you:` and
   the disagreement. Findings still open at the end go in your
   `Needed from you and why` section.

## Reference docs

- [Machine requirements](docs/capabilities.md) lists what each feature needs
  and what users see when a requirement is missing.
- [Agent skill conventions](docs/agent-skill-conventions.md) covers roots,
  including cross-harness reads and plugin caches, invocation and provenance.
  The core owns [agent IDs and lifecycle ownership](crates/skill-studio-core/src/identity.rs).
  Skill Studio runs skills.sh and dotagents for their managed installs and
  moves removed Skill Studio copies and forks into quarantine.
- [Flow map](docs/flows.md) links each user flow to its entry points and tests.
  Add one row and a test at the lowest seam that proves a new flow's file
  effects. CLI and MCP parity comes from the `flow_parity.rs` table.
- [State machines](docs/state-machines.md) lists the states and legal moves.
  Add one row and one table test for a new transition rule.
- [Core contracts](docs/spec-core-primitives.md), [event store](docs/spec-event-store.md)
  and [headless performance](docs/spec-headless-performance-observability.md)
  describe shared behavior.
- [CLI](apps/cli/README.md), [MCP](apps/mcp/README.md) and
  [skills.sh proxy](apps/server/README.md) cover the adapters.
- [Shared skill types](packages/lib/src/skill-types.ts),
  [core scanner](crates/skill-studio-core/src/scan.rs) and
  [desktop commands](apps/desktop/src-tauri/src/skills/commands.rs) are
  starting points for code changes.
- [Performance checks](docs/perf/README.md), [verification notes](docs/verification)
  and the [verify-skill-studio skill](.agents/skills/verify-skill-studio/SKILL.md)
  cover checks against real files and tools.
