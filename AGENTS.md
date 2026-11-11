# AGENTS.md - Skill Studio

## Project Overview

Skill Studio is a Tauri 2.x desktop application to manage, sync, and test agent skills across Claude Code, Codex, OpenCode, pi, Cursor, and Grok Build, with skills.sh discovery built in.

### Core Features

1. **Skill Discovery** - Search 36,000+ skills from skills.sh
2. **Skill Installation** - Install/remove/update via `npx skills` CLI to global or project scope
3. **First-Class Agents** - Claude Code, Codex, OpenCode, pi, Cursor, Grok Build, plus a shared `.agents/skills` root
4. **Provenance** - Every installed skill is classified as `skills-sh`, `plugin`, `dotagents`, or `manual` (precedence: dotagents > plugin > skills-sh > manual)
5. **Native Plugin Enumeration** - Discovers skills shipped inside Claude Code (`~/.claude/plugins/cache`) and Codex (`~/.codex/plugins/cache`) plugin caches, per the agent-plugins.org manifest convention
6. **Spec Validation** - Flags agentskills.io SKILL.md spec violations (`spec_violations`) and detects the getsentry/skillet spec pattern (`has_spec`)

### Tech Stack

- **Frontend**: React 19 + TypeScript + Tailwind CSS 4.x + Zustand
- **Backend**: Tauri 2.x (Rust)
- **Skills Integration**: skills.sh API + `npx skills` CLI

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

```bash
# Build Rust backend
cd apps/desktop/src-tauri && cargo build

# Run Rust tests
cd apps/desktop/src-tauri && cargo test

# Run a single Rust test
cd apps/desktop/src-tauri && cargo test test_name

# Check Rust code
cd apps/desktop/src-tauri && cargo check

# Format Rust code
cd apps/desktop/src-tauri && cargo fmt

# Lint Rust code
cd apps/desktop/src-tauri && cargo clippy
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

# Lint (oxlint)
pnpm run lint
pnpm run lint:fix

# Format (oxfmt)
pnpm run format
pnpm run format:check

# React health check (react-doctor)
pnpm run doctor

# Full gate: typecheck + lint + format:check + doctor + knip + test + types:check + cargo fmt --all --check + clippy --workspace --all-targets -D warnings + cargo test --workspace (CI also runs cargo machete and cargo deny check)
pnpm run check
```

## Tech Stack

| Layer       | Technology                                                      |
| ----------- | --------------------------------------------------------------- |
| Framework   | Tauri 2.x (macOS desktop app)                                   |
| Frontend    | React 19.1, TypeScript 5.8                                      |
| Styling     | Tailwind CSS 4.x                                                |
| State       | Zustand 5.x                                                     |
| Linting     | oxlint (JS/TS), clippy (Rust)                                   |
| Formatting  | oxfmt (JS/TS), rustfmt (Rust)                                   |
| Lint plugin | anti-slop (local oxlint JS plugin in `tools/oxlint/anti-slop/`) |
| Icons       | lucide-react                                                    |
| Backend     | Rust 2021 Edition                                               |

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
│   └── server/                   # Node proxy for the skills.sh API (port 8787)
├── crates/
│   ├── skill-studio-core/        # Scope-confined core: scan, ops, events, DTOs, schema/
│   └── skill-studio-host/        # Real-world port adapters for the core crate
├── packages/
│   ├── lib/                      # Shared TypeScript library
│   ├── marketing/                # Marketing site (Vite + StyleX) and Remotion walkthroughs
│   ├── npm/                      # npm packages for the CLI; not workspaces (packages/* stops one level up)
│   └── ui/                       # Shared UI primitives (@skill-studio/ui)
├── docs/                         # Specs and reference docs
├── tools/
│   └── oxlint/anti-slop/         # Local oxlint JS plugin
└── package.json                  # npm workspaces root
```

## Code Style Guidelines

### TypeScript/React

**UI primitives:** Form controls and buttons come from `@skill-studio/ui`
(`Input`, `Textarea`, `Select`, `Button`, and the other exports in
`packages/ui/src/index.ts`). Raw `<input>`, `<textarea>`, `<select>`, and
`<button>` are allowed only inside `packages/ui`; the
`anti-slop/no-raw-form-elements` lint rule enforces this.

**Imports:** Group in order - React, external libs, internal modules, types

```typescript
import { useEffect, useCallback, useState } from "react";
import { motion } from "motion/react";
import { X, Save } from "lucide-react";
import { useAppStore } from "./store/appStore";
import type { AgentId, InstalledSkill } from "./lib/skill-types";
```

**Components:** Use function components with explicit prop interfaces

```typescript
interface PanelProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
}

export function Panel({ isOpen, onClose, title, children }: PanelProps) {
  // ...
}
```

**Hooks:** Prefix with `use`, return object or array consistently

```typescript
export function useKeyboardNavigation(options: Options) { ... }
```

**Types:** Use `interface` for objects, `type` for unions/primitives

```typescript
export interface BaseEntityFields {
  id: string;
  name: string;
}
export type EntityType = "settings" | "memory" | "agent";
export type FilterScope = "all" | "global" | "project";
```

**Type Guards:** Create explicit type guards for discriminated unions

```typescript
export function isFlatEntity(entity: DisplayableEntity): entity is FlatEntity {
  return "path" in entity && "scope" in entity;
}
```

**File Headers:** Use comment blocks for major files

```typescript
// ============================================================================
// Skill Studio - Module Name
// Brief description of purpose
// ============================================================================
```

### Rust

**Structs:** Use `#[derive(Debug, Serialize, Deserialize, Clone)]`

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaseEntity {
    pub id: String,
    pub name: String,
}
```

**Tauri Commands:** Use `#[tauri::command]` attribute

```rust
#[tauri::command]
pub fn discover_all(project_paths: Option<Vec<String>>) -> Result<DiscoveryResult, String> {
    // ...
}
```

**Error Handling:** Return `Result<T, String>` for Tauri commands

```rust
fn get_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}
```

### Tailwind CSS

- Use CSS variables for theming: `var(--color-bg-primary)`, `var(--color-text-primary)`
- Prefer utility classes over custom CSS
- Use responsive prefixes: `sm:`, `md:`, `lg:`
- Common patterns: `flex items-center gap-2`, `px-4 py-2`, `rounded-md`

### State Management (Zustand)

- Single store in `apps/desktop/src/store/appStore.ts`
- Use selectors for performance: `useAppStore((state) => state.activeView)`
- Group related state and actions together
- Invalidate caches by setting `_cachedSections: null`

### Naming Conventions

| Type         | Convention      | Example                    |
| ------------ | --------------- | -------------------------- |
| Components   | PascalCase      | `DetailPanel`, `Toast`     |
| Hooks        | camelCase + use | `useKeyboardNavigation`    |
| Types        | PascalCase      | `EntityType`, `ViewType`   |
| Variables    | camelCase       | `selectedEntity`, `isOpen` |
| Constants    | SCREAMING_SNAKE | `ENTITY_TEMPLATES`         |
| Files (TS)   | PascalCase.tsx  | `DetailPanel.tsx`          |
| Files (Rust) | snake_case.rs   | `mod.rs`, `lib.rs`         |

### Naming files

No bare-role filenames (`types.ts`, `api.ts`, `utils.ts`); prefix the domain (`skill-types.ts`, `skill-api.ts`). Use `index.ts` only as a thin re-export.

### Error Handling

**Frontend:** Use try-catch with toast notifications

```typescript
try {
  await discoverAll([homeDir]);
} catch (err) {
  addToast({
    type: "error",
    title: "Discovery Failed",
    message: err instanceof Error ? err.message : "Unknown error",
  });
}
```

**Backend:** Return Result types, use `.ok_or()` for Option conversion

```rust
let home = get_home_dir().ok_or("Could not find home directory")?;
```

### Key Files

| Purpose            | File                                              |
| ------------------ | ------------------------------------------------- |
| Main App           | `apps/desktop/src/App.tsx`                        |
| State Store        | `apps/desktop/src/store/appStore.ts`              |
| Skill types        | `apps/desktop/src/lib/skill-types.ts`             |
| Tauri IPC wrappers | `apps/desktop/src/lib/skill-api.ts`               |
| Tauri commands     | `apps/desktop/src-tauri/src/skills/commands.rs`   |
| Scanner            | `apps/desktop/src-tauri/src/skills/scan.rs`       |
| Provenance         | `apps/desktop/src-tauri/src/skills/provenance.rs` |
| Agent paths        | `apps/desktop/src-tauri/src/skills/agents.rs`     |
| Lint config        | `.oxlintrc.json`                                  |
| Format config      | `.oxfmtrc.json`                                   |
| Tauri Config       | `apps/desktop/src-tauri/tauri.conf.json`          |
| TS Config          | `apps/desktop/tsconfig.json`                      |

### TypeScript Strictness

Enabled in `apps/desktop/tsconfig.json`:

- `strict: true`
- `noUnusedLocals: true`
- `noUnusedParameters: true`
- `noFallthroughCasesInSwitch: true`

### Testing

- Rust: `cargo test` in `apps/desktop/src-tauri`, tests live in colocated `#[cfg(test)]` modules
- Frontend: Vitest is configured; run `pnpm run test`

## Before you implement an issue

Review findings and follow-up issues often ask for a guarantee in one rare
case. If you implement each case as written, you get fixes on top of fixes:
the Park chain #485, #487 and #491 reached 11,000 lines and was closed.
The shared guard proposed in #531 was not built; #531 was also closed. Before you write code for an issue:

1. Write the user need in one sentence. Write the smallest change that meets
   it. If the issue asks for more than the need (for example byte-exact
   output), implement the need and say why in the PR.
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

## Reference docs

- `docs/agent-skill-conventions.md` — agentskills.io spec rules, per-agent discovery paths, invocation control (explicit vs model-invocable), the agent settings that hide a skill (read only; Park is the only off), and the local data sources Skill Studio reads. Check it before researching agent behavior again.
- `docs/flows.md` — every user-facing flow with its UI entry, command, and flow-level test. A new flow adds a row and a test.
- `docs/state-machines.md` — every explicit state machine (states, legal moves, guard test) and the ones still implicit. A new status field or phase enum adds a row and a table test.

## Skills.sh Integration

Skill Studio integrates with skills.sh for skill discovery and installation.

### API Endpoint

- Default: requests route through the local Skill Studio server (`apps/server`, default `http://127.0.0.1:8787/api/v1`), which holds the real skills.sh key - no client key needed. Settings shows the resolved mode; see `apps/server/README.md`. Development builds use `http://127.0.0.1:8787`; release builds use the hosted proxy at `https://api.useskillstudio.com`.
- Developer override: a non-empty `skills_sh_api_key` in `~/.agents/skill-studio.json` instead sends `Authorization: Bearer <key>` straight to `https://skills.sh/api/v1`, bypassing the server.
- **List**: `GET /skills?view=all-time&page=<0-indexed>&per_page=<n>` - paginated, sorted by install count
- **Search**: `GET /skills/search?q=<query>&limit=<n>` - no pagination, one shot up to `limit`
- **Details**: `GET /skills/{owner/repo}/{slug}` - returns the skill's files, including its `SKILL.md`/`AGENTS.md` body

### Lock File (`~/.agents/.skill-lock.json`)

Tracks installed skills with their sources and hashes:

```json
{
  "version": 3,
  "skills": {
    "skill-name": {
      "source": "owner/repo",
      "sourceType": "github",
      "sourceUrl": "https://github.com/...",
      "skillFolderHash": "abc123",
      "installedAt": "2024-01-31T...",
      "updatedAt": "2024-01-31T..."
    }
  }
}
```

### CLI Dependency

- Installation: Uses `npx skills add <skill>` for battle-tested install logic
- Removal: Uses `npx skills remove <skill>`
- Updates: Uses `npx skills update <skill>`
- Requires Node.js ^22.18 or >=24.11 (the `npx skills` CLI itself needs only 18+, but the build toolchain needs the newer range)

### First-Class Agents

| Agent       | Project Path                        | Global Path                                  |
| ----------- | ----------------------------------- | -------------------------------------------- |
| Claude Code | `.claude/skills/`                   | `~/.claude/skills/`                          |
| Codex       | `.codex/skills/`                    | `~/.codex/skills/`                           |
| OpenCode    | `.opencode/skills/` (also `skill/`) | `~/.config/opencode/skills/` (also `skill/`) |
| pi          | `.pi/skills/`                       | `~/.pi/agent/skills/`                        |
| Cursor      | `.cursor/skills/`                   | `~/.cursor/skills/`                          |
| Grok Build  | `.grok/skills/`                     | `~/.grok/skills/`                            |
| shared      | `.agents/skills/`                   | `~/.agents/skills/`                          |

`npx skills` can still target the full agent list; see `apps/desktop/src-tauri/src/skills/agents.rs` for `AgentId`.
