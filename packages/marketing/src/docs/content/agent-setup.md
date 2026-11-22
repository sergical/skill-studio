# Set up with an agent

You'll have a connected MCP server and a read-only skill report from your agent.

Use a supported Mac, Node.js 18 or later, working `npx` and an MCP client that can launch a stdio process. The first start downloads the package from npm.

## Desktop

Open **Settings → Connect your agents**. Connect the agent you want to use. Cursor opens its own confirmation dialog. Restart the agent after adding the server.

## Setup prompt

Copy this prompt into your agent to have it configure and check Skill Studio.

<!-- setup-prompt -->

## Set up by hand

Add a server named `skill-studio` at user scope. Choose the instructions for your client.

<!-- agent-setups -->

For any other MCP client, add a stdio server named `skill-studio` with command `npx` and arguments `-y skill-studio@latest mcp`.

## Verify the connection

Restart the client and look for Skill Studio in its MCP server list. Ask it to call `diagnose` and show the issues it found. In the fixture run the MCP server identified itself as `skill-studio` version `0.4.1`. The tool response had `operation: "diagnose"` and `status: "ok"`. Read `data.issues` even when the tool call succeeds.

If the client shows the entry but the call fails, see [Troubleshooting](/docs/troubleshooting/). To undo setup, remove the `skill-studio` entry from the same client or config file and restart. Your skills stay in place.
