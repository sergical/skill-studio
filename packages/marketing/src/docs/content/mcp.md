# MCP server

Connect Skill Studio to your agent. Then you can ask the agent to check, fix and park your skills.

The server is the stdio command `{{mcp-server-command}}`, named `skill-studio`. It needs Node.js and macOS. The first start downloads the package.

## Let your agent set it up

Copy this prompt into your agent. It adds the server, checks your skills without changing anything, and tells you what to try next.

<!-- setup-prompt -->

The Skill Studio app can do this too: open **Settings**, then **Connect your agents**.

## Add it yourself

Each command adds the server for your user, so it works in every project. Restart the agent after you add the server.

<!-- agent-setups -->

### Any other agent

Add a stdio MCP server named `skill-studio` with the command `npx` and the arguments `-y skill-studio@latest mcp`.

## What your agent can do

- List skills in supported agents' home and project folders and find problems.
- Find skills with no recorded use in the last 30 days of available history, and skills that are past their review date.
- Find skills that have copies with different content.
- Check how much of each agent's skill listing budget your skills use.
- Fix broken skills.
- List hook events, MCP server names and instruction file paths.
- Set or clear a review date.
- Park and unpark skills.
- Turn a skill off for one agent only. The agent checks first, then splits the shared skill and parks one copy.
- Set whether a skill runs only when you ask for it, or when the model picks it too.
- Split a shared skill into separate copies for selected agents.
- Remove a duplicate copy.
- Install skills and check for updates. Pack updates apply to copies with no local edits that are managed by a supported installer.
- List past changes and restore one.

## Try it

Ask your agent:

```text
Show my broken skills and skills with no recorded use in the last 30 days. Show me what you would park.
```

## Undo

Changes with a restore action appear in the same history as app changes. Open **Activity** and select **Restore**, or use `restore_event` with the event ID. Restore can refuse after later edits or when a backup has expired. The CLI command `npx skill-studio@latest undo` tries the latest change. To reverse a park, use `unpark`. To reverse an unpark, use `park`.
