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

- List your skills and find problems.
- Find unused skills, and skills that are past their review date.
- Find skills that have copies with different content.
- Check how much of each agent's skill listing budget your skills use.
- Fix broken skills.
- Park and unpark skills. Parking turns a skill off for every agent.
- Turn a skill off for one agent only. The agent checks first, then splits the shared skill and parks one copy.
- Set whether a skill runs only when you ask for it, or when the model picks it too.
- Remove a duplicate copy.
- Install skills, find skills that have a newer version, and update them.
- List past changes and restore one.

## Try it

Ask your agent:

```text
Find my unused and broken skills and park the ones I don't need.
```

## Undo

Each change that your agent makes goes into the same history that the app shows. To undo a change, open **Activity** in the app and select **Restore**, or run `npx skill-studio@latest undo`. To undo a park, unpark the skill.
