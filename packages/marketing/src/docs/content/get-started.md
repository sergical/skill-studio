# Get started

Skill Studio shows every agent skill on your Mac in one list. Use it to find broken, duplicate and unused skills, then fix or park them. You can undo each change.

## Install

1. Download Skill Studio for your Mac:
   - [Apple Silicon](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-arm64.dmg) (most Macs sold since 2020)
   - [Intel](https://github.com/sergical/skill-studio/releases/latest/download/Skill-Studio-intel.dmg)
   - Or see [all releases](https://github.com/sergical/skill-studio/releases) for older versions.
2. Move Skill Studio to your Applications folder.
3. Open Skill Studio.

Skill Studio needs macOS 13 or later. It runs on Apple Silicon and Intel Macs.

## First look

The app reads the skill folders of Claude Code, Codex, OpenCode, Cursor, pi and Grok Build, and the shared `.agents/skills` folder. It shows each skill once, with:

- The agents that load it.
- Its scope: global, or a project.
- When an agent last used it in the available history. The 30-day view shows skills with no recorded use in that window.
- Any problem, for example a broken link or bad frontmatter.

For a shared global skill installed through skills.sh, **Fork** keeps your edits in a separate copy. Use **Pull upstream** to review and bring in changes from the original.

## Fix a broken skill

A skill with a problem shows a warning. Select the skill, then select **Fix**. Skill Studio repairs what it can, for example a broken link or bad frontmatter. If it cannot fix a problem, it shows the path to the file, so you can fix it yourself.

## Park and unpark

Park a skill to hide it from every agent. Skill Studio does not delete it.

1. Select the skill.
2. Select **Park**.

To use the skill again, select **Unpark**.

## Undo

Activity history supports undo for installs, updates, removes, and repairs. Restore can refuse if a later edit changed the skill or its backup has expired.

1. Open **Activity**.
2. Find the change in **History**.
3. Select **Restore**.

To undo a park, select **Unpark**.

## Install from skills.sh

You can browse and install thousands of skills from skills.sh.

1. Select **Add skill**.
2. Select **Browse skills.sh**.
3. Find a skill, then install it.

Choose an install method before you install. The skills.sh method uses `npx skills` and tracks the install in its lock file. The dotagents method uses dotagents and tracks the install in `agents.toml`. You can also install a local copy.

## Privacy

- You can scan without an account. Scanning reads skill files. Fixing, installing, updating, parking, and removing write to skill folders. Project changes can show up in Git.
- The Skill Store, public install-count lookups, installs, updates, and update checks use the network. Skill update checks, copying from GitHub, and pulling upstream changes need a signed-in GitHub CLI (`gh`).
- Crash and timing reports are on by default in desktop setup. To turn them off, open **Settings**, find **Telemetry**, and switch off **Send crash and performance data**.

## Connect your agent

Let your agent check and tidy its own skills. Copy this prompt into it:

<!-- setup-prompt -->

To add the server by hand, or from the app, see [MCP server](/docs/mcp/).

## Next steps

- [Use the CLI](/docs/cli/) to check and fix skills from the terminal.
- [Connect the MCP server](/docs/mcp/) so your agent can tidy its own skills.
