# Start here

You'll have a first report of the skills and problems Skill Studio found in the supported agents' home and project folders.

Choose the Mac app for a visual list, the CLI for terminal output, or MCP to ask your agent. See [Requirements and install](/docs/requirements/) for your choice.

## Install

1. Download Skill Studio for your Mac:
   - [Apple Silicon](/download) (most Macs sold since 2020)
   - [Intel](/download/intel)
2. Move Skill Studio to your Applications folder.
3. Open Skill Studio.

Skill Studio needs macOS 13 or later. It runs on Apple Silicon and Intel Macs.

## Desktop

1. On first run, keep the agents you want to scan. Leave **Find my projects from agent history** on to include discovered projects.
2. Select **Continue**, then open **Home** and look under **Broken** and **Warnings**.
3. Open **Skills** to see the inventory, even when Home shows no problems.

## CLI

Run this with Node.js 18 or later. The first run downloads the npm package.

```sh
npx -y skill-studio@latest diagnose
```

On a clean Mac with no skills installed, the output is:

```text
No skills found.
No issues.
```

On a Mac with a broken link, the report includes a line like this. The path is an example.

```text
Issues:
  [error] broken link, example-missing: /Users/example/.claude/skills/example-missing links to a missing target
```

Exit code 1 means the scan completed and found issues. Read the findings before changing anything.

## MCP

[Set up the server](/docs/agent-setup/) and restart your client. Ask it to run Skill Studio's `diagnose` tool and show the findings without applying fixes. A clean tool result has an empty `data.issues` list.

The scan leaves skill files alone. If a skill is missing from the report, check [Troubleshooting](/docs/troubleshooting/). To act on a finding, use [Clean up and undo](/docs/cleanup/).
