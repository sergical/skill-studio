# skill-studio

Tidy up your agent skills. `skill-studio` finds your skills across Claude Code, Codex, OpenCode, pi, Cursor, Grok Build and the shared `.agents` folder. It finds broken, duplicate and unused skills. You can undo most changes.

It runs on macOS, on Apple silicon and Intel Macs. It needs Node.js 18 or later.

## Run it

```bash
npx skill-studio@latest diagnose
```

Or install it:

```bash
npm install -g skill-studio
skill-studio --help
```

Some commands:

- `skill-studio scan` lists every skill installed for your agents.
- `skill-studio diagnose` finds broken skills and says what is wrong with each.
- `skill-studio usage` shows which skills your agents used, and which they never used.
- `skill-studio park <skill>` turns a skill off for all agents. `skill-studio unpark <skill>` turns it on again.
- `skill-studio turn-off <skill> --agent <agent> --check` shows how a skill goes off for one agent only. Run it again with `--confirm` to do it.
- `skill-studio doctor` checks how much of each agent's skill listing budget your skills use.
- `skill-studio undo` reverses your last change. If the last change was a park or an unpark, it stops: run `skill-studio unpark <skill>` or `skill-studio park <skill>` instead.

## Use it from your agent (MCP)

`skill-studio mcp` starts an MCP server. To let your agent add it, copy the setup prompt from the [MCP docs](https://useskillstudio.com/docs/mcp/) and paste it into your agent. To add it yourself:

```bash
# Claude Code
claude mcp add --scope user --transport stdio skill-studio -- npx -y skill-studio@latest mcp

# Codex
codex mcp add skill-studio -- npx -y skill-studio@latest mcp

# OpenCode
opencode mcp add --global skill-studio -- npx -y skill-studio@latest mcp

# pi
pi mcp add skill-studio -- npx -y skill-studio@latest mcp

# Grok Build
grok mcp add skill-studio -- npx -y skill-studio@latest mcp
```

`@latest` makes `npx` use the published package. The first download can take more than ten seconds. If Codex stops waiting, set `startup_timeout_sec = 30` under `[mcp_servers.skill-studio]` in `~/.codex/config.toml`.

Cursor, VS Code, Gemini CLI, Claude Desktop and other clients: see the [MCP docs](https://useskillstudio.com/docs/mcp/).

## Privacy

No account. It runs on your Mac. It can send anonymous crash reports, which you can turn off.

## License

MIT
