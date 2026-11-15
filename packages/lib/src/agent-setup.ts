// ============================================================================
// Skill Studio - Agent setup
// How each coding agent adds the Skill Studio MCP server, and the setup prompt
// a user pastes into any agent. The desktop Settings card, the docs site, the
// homepage and llms.txt all read this file, so they cannot drift apart.
// ============================================================================

export const MCP_SERVER_NAME = "skill-studio";

// `@latest` makes npx fetch the published package. A bare `skill-studio`
// resolves to any local package with that name first (for example this
// repository's own desktop app), which has no bin and fails with
// "could not determine executable to run".
export const MCP_SERVER_ARGS = ["-y", "skill-studio@latest", "mcp"] as const;
export const MCP_SERVER_COMMAND_LINE = `npx ${MCP_SERVER_ARGS.join(" ")}`;
export const CLI_COMMAND_PREFIX = "npx -y skill-studio@latest";

export const SITE_ORIGIN = "https://useskillstudio.com";
export const MCP_DOCS_MARKDOWN_URL = `${SITE_ORIGIN}/docs/mcp.md`;

export interface AgentSetupConfigFile {
  /** Where the file lives, with `~` for the home folder. */
  path: string;
  language: "json" | "toml";
  snippet: string;
}

export interface AgentSetup {
  /** The `AgentId` for the agents Skill Studio manages; a plain slug for the rest. */
  id: string;
  name: string;
  /** Adds the server at user scope through the agent's own CLI. */
  command?: string;
  /** Removes the user-scope entry, for an agent whose add refuses a name it already has. */
  remove?: string;
  /** A link the agent's app opens to show its own install dialog. */
  installLink?: { label: string; url: string };
  /** The file to edit by hand when there is no command, or to share with a project. */
  configFile?: AgentSetupConfigFile;
  /** Shows that the agent sees the server. */
  check?: string;
}

const STDIO_SERVER = { command: "npx", args: [...MCP_SERVER_ARGS] };

/** Base64 of a UTF-8 string, in both the browser and Node. */
function base64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

const MCP_SERVERS_JSON = JSON.stringify(
  { mcpServers: { [MCP_SERVER_NAME]: STDIO_SERVER } },
  null,
  2,
);
const VS_CODE_SERVER = { name: MCP_SERVER_NAME, type: "stdio", ...STDIO_SERVER };
const addCommand = (prefix: string) => `${prefix} ${MCP_SERVER_NAME} -- ${MCP_SERVER_COMMAND_LINE}`;

/** Agents Skill Studio manages first, in the order the app lists them, then other MCP clients. */
export const AGENT_SETUPS: readonly AgentSetup[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    command: addCommand("claude mcp add --scope user --transport stdio"),
    remove: `claude mcp remove --scope user ${MCP_SERVER_NAME}`,
    configFile: { path: ".mcp.json (project)", language: "json", snippet: MCP_SERVERS_JSON },
    check: "claude mcp list",
  },
  {
    id: "codex",
    name: "Codex",
    command: addCommand("codex mcp add"),
    configFile: {
      path: "~/.codex/config.toml",
      language: "toml",
      snippet: [
        `[mcp_servers.${MCP_SERVER_NAME}]`,
        `command = "npx"`,
        `args = [${MCP_SERVER_ARGS.map((arg) => `"${arg}"`).join(", ")}]`,
        `startup_timeout_sec = 30`,
      ].join("\n"),
    },
    check: "codex mcp list",
  },
  {
    id: "cursor",
    name: "Cursor",
    installLink: {
      label: "Add to Cursor",
      url: `cursor://anysphere.cursor-deeplink/mcp/install?name=${MCP_SERVER_NAME}&config=${encodeURIComponent(base64(JSON.stringify(STDIO_SERVER)))}`,
    },
    configFile: { path: "~/.cursor/mcp.json", language: "json", snippet: MCP_SERVERS_JSON },
    check: "cursor-agent mcp list",
  },
  {
    id: "open-code",
    name: "OpenCode",
    command: addCommand("opencode mcp add --global"),
  },
  {
    id: "pi",
    name: "pi",
    command: addCommand("pi mcp add"),
    configFile: { path: "~/.pi/agent/mcp.json", language: "json", snippet: MCP_SERVERS_JSON },
    check: "pi mcp list",
  },
  {
    id: "grok-build",
    name: "Grok Build",
    command: addCommand("grok mcp add"),
    remove: `grok mcp remove ${MCP_SERVER_NAME}`,
    check: "grok mcp list",
  },
  {
    id: "vscode",
    name: "VS Code",
    command: `code --add-mcp '${JSON.stringify(VS_CODE_SERVER)}'`,
    installLink: {
      label: "Add to VS Code",
      url: `vscode:mcp/install?${encodeURIComponent(JSON.stringify(VS_CODE_SERVER))}`,
    },
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    // Gemini reads `<name> <command>` before `--`, so `npx` goes before it.
    command: `gemini mcp add --scope user ${MCP_SERVER_NAME} npx -- ${MCP_SERVER_ARGS.join(" ")}`,
    check: "gemini mcp list",
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    configFile: {
      path: "~/Library/Application Support/Claude/claude_desktop_config.json",
      language: "json",
      snippet: MCP_SERVERS_JSON,
    },
  },
];

export function agentSetup(id: string): AgentSetup | undefined {
  return AGENT_SETUPS.find((setup) => setup.id === id);
}

/** The text a user pastes into any agent: it adds the server, runs a first check, and says what to try. */
export const AGENT_SETUP_PROMPT = [
  "Set up Skill Studio for me. Skill Studio finds broken, duplicate and unused agent skills on this Mac and fixes or parks them, with undo. It runs as an MCP server and a CLI.",
  "",
  `1. Add the Skill Studio MCP server to yourself at user scope. The server name is "${MCP_SERVER_NAME}" and the stdio command is \`${MCP_SERVER_COMMAND_LINE}\`. Run the line for the agent you are.`,
  ...AGENT_SETUPS.filter((setup) => setup.command).map(
    (setup) =>
      `   - ${setup.name}: ${setup.command}` +
      (setup.remove ? ` (if it says the server exists, run ${setup.remove} first)` : ""),
  ),
  `   For any other agent, follow ${MCP_DOCS_MARKDOWN_URL}.`,
  `2. Check my skills now, without changing anything: run \`${CLI_COMMAND_PREFIX} diagnose\` and give me a short list of the problems it found. It exits with code 1 when it finds problems; that is not a failure.`,
  "3. Tell me to restart you so the skill-studio tools load. Then suggest one next step, for example: “Park the skills I haven’t used in 30 days.”",
].join("\n");
