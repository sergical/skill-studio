import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  AGENT_SETUP_PROMPT,
  AGENT_SETUPS,
  MCP_SERVER_ARGS,
  MCP_SERVER_COMMAND_LINE,
  agentSetup,
} from "./agent-setup";

const desktopConnect = readFileSync(
  new URL("../../../apps/desktop/src-tauri/src/skills/skill_mcp_connect.rs", import.meta.url),
  "utf8",
);

describe("agent setup", () => {
  it("the Cursor link the docs show is the one the desktop app opens, or Add to Cursor installs a different server", () => {
    const rustLink = /const CURSOR_INSTALL_LINK: &str =\s*"([^"]+)";/.exec(desktopConnect)?.[1];
    expect(rustLink).toBe(agentSetup("cursor")?.installLink?.url);
  });

  it("the server command the docs show is the argv the desktop app writes, or a docs reader and an app user get different servers", () => {
    const rustArgv = /const SERVER_COMMAND: \[&str; \d+\] = \[([^\]]+)\]/.exec(desktopConnect)?.[1];
    const argv = rustArgv?.split(",").map((part) => part.trim().replace(/^"|"$/g, ""));
    expect(argv).toEqual(["npx", ...MCP_SERVER_ARGS]);
  });

  it("the setup prompt holds every agent's add command, or an agent reading it has no line to run", () => {
    for (const setup of AGENT_SETUPS) {
      if (setup.command) expect(AGENT_SETUP_PROMPT).toContain(setup.command);
    }
  });

  it("every add command runs the published package, or npx picks a local skill-studio package with no bin", () => {
    for (const setup of AGENT_SETUPS) {
      if (setup.command) expect(setup.command).toContain("skill-studio@latest");
    }
    expect(MCP_SERVER_COMMAND_LINE).toContain("skill-studio@latest");
  });

  it("the docs give a remove step for exactly the agents whose add the desktop app runs after a remove, or a re-add fails on an existing entry", () => {
    const removeFirst = [
      ...desktopConnect.matchAll(
        /"([a-z-]+)" => \(\s*"[^"]+",\s*"[^"]+",\s*&\[[^\]]*\],\s*true,?\s*\)/g,
      ),
    ].map((match) => match[1]);
    const withRemove = AGENT_SETUPS.filter((setup) => setup.remove).map((setup) => setup.id);
    expect(removeFirst).toEqual(["claude-code", "grok-build"]);
    expect(withRemove).toEqual(removeFirst);
  });
});
