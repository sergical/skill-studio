// ============================================================================
// Skill Studio - Agent setup markdown
// Fills the setup placeholders in the docs markdown from the shared agent
// setup list, so the docs page, its .md copy and llms.txt show the same
// commands the desktop app runs.
// ============================================================================

import {
  AGENT_SETUP_PROMPT,
  AGENT_SETUPS,
  MCP_SERVER_COMMAND_LINE,
  type AgentSetup,
} from "@skill-studio/lib/agent-setup";

const SETUP_PROMPT_PLACEHOLDER = "<!-- setup-prompt -->";
const AGENT_SETUPS_PLACEHOLDER = "<!-- agent-setups -->";
const SERVER_COMMAND_PLACEHOLDER = "{{mcp-server-command}}";

function fence(language: string, body: string): string {
  return ["```" + language, body, "```"].join("\n");
}

function agentSection(setup: AgentSetup): string {
  const parts = [`### ${setup.name}`];
  if (setup.installLink) {
    parts.push(
      `[${setup.installLink.label}](${setup.installLink.url}) opens ${setup.name}, which asks you to confirm.`,
    );
  }
  if (setup.command) {
    parts.push(setup.installLink ? "Or run:" : "Run:", fence("sh", setup.command));
  }
  if (setup.remove) {
    parts.push(
      `If ${setup.name} already has a \`skill-studio\` entry, remove it first with \`${setup.remove}\`.`,
    );
  }
  if (setup.configFile) {
    const lead = setup.command || setup.installLink ? "Or add it to" : "Add it to";
    parts.push(
      `${lead} \`${setup.configFile.path}\`:`,
      fence(setup.configFile.language, setup.configFile.snippet),
    );
  }
  if (setup.check) {
    parts.push(`To check, run \`${setup.check}\` and look for \`skill-studio\`.`);
  }
  return parts.join("\n\n");
}

function agentSetupMarkdown(): string {
  return AGENT_SETUPS.map(agentSection).join("\n\n");
}

/** Replaces the setup placeholders in a docs page; a page without them comes back unchanged. */
export function fillAgentSetup(markdown: string): string {
  return markdown
    .replace(SETUP_PROMPT_PLACEHOLDER, fence("text", AGENT_SETUP_PROMPT))
    .replace(AGENT_SETUPS_PLACEHOLDER, agentSetupMarkdown())
    .replaceAll(SERVER_COMMAND_PLACEHOLDER, MCP_SERVER_COMMAND_LINE);
}

/** The llms.txt lines that tell an agent how to add the server to itself. */
export function agentSetupLlmsLines(): string[] {
  return [
    `Add a stdio MCP server named \`skill-studio\` that runs \`${MCP_SERVER_COMMAND_LINE}\`. Per agent:`,
    "",
    ...AGENT_SETUPS.map((setup) => {
      const how = setup.command ?? setup.installLink?.url ?? `edit ${setup.configFile?.path}`;
      return `- ${setup.name}: \`${how}\``;
    }),
  ];
}
