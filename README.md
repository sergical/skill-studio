# Skill Studio

Tidy up your agent skills. Skill Studio finds broken, duplicate, and unused
skills across Claude Code, Codex, OpenCode, pi, Cursor, and Grok Build. Then it
helps you fix or park them, and you can undo every change.

It comes as a macOS app, a CLI, and an MCP server. All three use the same Rust
core.

[Website](https://useskillstudio.com) ·
[Docs](https://useskillstudio.com/docs/) ·
[Download for Apple silicon](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-arm64.dmg) ·
[Download for Intel](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-intel.dmg)

## What it does

- **See every skill in one list.** Global and project skills for Claude Code,
  Codex, OpenCode, pi, Cursor, Grok Build, and the shared `.agents/skills`
  folder. Each skill shows where it came from: skills.sh, dotagents, a Claude
  Code or Codex plugin, the project repo, or a manual copy.
- **Find problems.** Broken `SKILL.md` frontmatter, agentskills.io spec
  violations, conflicts between copies of the same skill, outdated installs,
  and skills that your agents never use.
- **Fix them.** Preview a frontmatter repair before you apply it, or edit
  `SKILL.md` in the app or in your editor.
- **Park a skill.** Parking moves a skill out of the folders that agents read,
  so it is off for every agent. Unpark puts it back.
- **Install and update.** Search the [skills.sh](https://skills.sh) store,
  install a skill to global or project scope, update one skill or all of them,
  and remove skills you do not need.
- **Undo.** The Activity view records every change. You can restore any of
  them.
- **Manage plugins.** Enable, update, or uninstall the Claude Code and Codex
  plugins that ship skills.

## Install

### macOS app

Download the DMG for
[Apple silicon](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-arm64.dmg)
or [Intel](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-intel.dmg).
The app needs macOS 13 or later and updates itself.

The app sends anonymous crash reports. They do not include skill names, files,
or paths. To stop them, turn off **Settings > Telemetry**, or turn them off on
the first screen.

### CLI

```bash
# Run once
npx skill-studio diagnose

# Or install it
npm install -g skill-studio
```

| Command               | What it does                                    |
| --------------------- | ----------------------------------------------- |
| `scan`                | List every installed skill                      |
| `diagnose`            | Find broken skills and spec violations          |
| `conflicts`           | Find copies of the same skill that do not match |
| `usage`               | Show how often each skill is used               |
| `fix`                 | Repair broken frontmatter                       |
| `park` / `unpark`     | Turn a skill off or on for every agent          |
| `add`                 | Install a skill                                 |
| `outdated` / `update` | Find and apply skill updates                    |
| `remove`              | Remove a skill                                  |
| `events` / `restore`  | Show the change history and restore a change    |
| `undo`                | Undo the last change                            |
| `mcp`                 | Start the MCP server                            |

See the [CLI docs](https://useskillstudio.com/docs/cli/).

### MCP server

Connect Skill Studio to your agent. Then you can ask the agent to check, fix,
and park your skills.

```bash
# Claude Code
claude mcp add --transport stdio skill-studio -- npx -y skill-studio mcp

# Codex
codex mcp add skill-studio -- npx -y skill-studio mcp
```

See the [MCP docs](https://useskillstudio.com/docs/mcp/) for other agents.

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) ^22.18 or >=24.11 (the Vite 8 and Babel 8
  toolchain needs this range)
- [pnpm](https://pnpm.io/) 11 (pinned in `packageManager` in `package.json`;
  enable it with `corepack enable`)
- [Rust](https://rustup.rs/)
- [Tauri prerequisites](https://tauri.app/start/prerequisites/)

### Setup

```bash
pnpm install

# Run the desktop app in development mode
pnpm run tauri dev

# Run the full local gate (TypeScript and Rust)
pnpm run check
```

### Fixture launch mode

Set `SKILL_STUDIO_FIXTURE` to a folder to use that folder as `HOME`. Every
scan and change then stays inside the fixture, so you can work against a known
skill layout and not touch your real `~/.claude`, `~/.codex`, and other agent
folders.

```bash
SKILL_STUDIO_FIXTURE=/path/to/fixture-home pnpm run tauri dev
```

The MCP server and the TUI also read `SKILL_STUDIO_FIXTURE`.

### Repository layout

| Path                          | Contents                                           |
| ----------------------------- | -------------------------------------------------- |
| `apps/desktop`                | Tauri 2 app: React in `src/`, Rust in `src-tauri/` |
| `apps/cli`                    | `skill-studio` CLI                                 |
| `apps/mcp`                    | MCP server over stdio                              |
| `apps/server`                 | skills.sh API proxy (Cloudflare Worker)            |
| `apps/tui`                    | Terminal UI                                        |
| `crates/skill-studio-core`    | Shared core: scan, diagnose, park, install, events |
| `crates/skill-studio-host`    | Real file system, process, and telemetry adapters  |
| `packages/marketing`          | Website and docs at useskillstudio.com             |
| `packages/npm`                | npm packages for the CLI                           |
| `packages/lib`, `packages/ui` | Shared TypeScript code and UI components           |
| `docs/`                       | Specs and `flows.md`, the list of every user flow  |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the local gate, and how a
change lands. This project follows the
[Contributor Covenant](CODE_OF_CONDUCT.md).

## Security

See [SECURITY.md](SECURITY.md) to report a vulnerability.

## License

[MIT](LICENSE)
