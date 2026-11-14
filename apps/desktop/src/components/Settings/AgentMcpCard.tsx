// ============================================================================
// AgentMcpCard - adds the Skill Studio MCP server to each agent the app
// found. The agent's own `mcp add` command (or, for Cursor, its install
// dialog) writes its config; this card only starts it and reads back which
// agents have the server. Connect again replaces an existing entry, for
// example one that points to a removed binary. Any other agent gets the setup
// prompt, which tells it how to add the server itself.
// ============================================================================

import { Check, Plug } from "lucide-react";
import { Button } from "@skill-studio/ui";
import {
  AGENT_SETUP_PROMPT,
  MCP_SERVER_COMMAND_LINE,
  agentSetup,
  type AgentId,
} from "@skill-studio/lib";
import { useDetectedHarnesses } from "../../hooks/useDetectedHarnesses";
import { useMcpConnections } from "../../hooks/useMcpConnections";
import { HarnessIcon } from "../ui/HarnessIcon";
import { CopyButton, SettingsCard, SettingsError, SettingsRow } from "./SettingsCard";

/** Agents the desktop backend can connect, through `connect_mcp`. */
const CONNECTABLE_AGENTS: readonly string[] = [
  "claude-code",
  "codex",
  "cursor",
  "open-code",
  "pi",
  "grok-build",
] satisfies AgentId[];

function isConnectable(id: string): id is AgentId {
  return CONNECTABLE_AGENTS.includes(id);
}

/** Cursor is an app with an install dialog, so its row needs no CLI on PATH. */
const OPENS_APP = "cursor";

export function AgentMcpCard() {
  const { report } = useDetectedHarnesses();
  const { connected, pending, attempted, error, connect } = useMcpConnections();

  const rows = (report?.harnesses ?? []).flatMap((row) =>
    isConnectable(row.id) && row.state !== "not_found" ? [{ ...row, id: row.id }] : [],
  );

  return (
    <SettingsCard
      icon={Plug}
      title="Connect your agents"
      description="Add the Skill Studio MCP server, then ask your agent to check, fix or park skills. Each agent's own command writes its config."
      action={<CopyButton text={AGENT_SETUP_PROMPT} label="Copy setup prompt" />}
    >
      {rows.length > 0 && (
        <ul className="m-0 flex list-none flex-col p-0">
          {rows.map((row) => {
            const added = connected?.has(row.id) ?? false;
            const opensApp = row.id === OPENS_APP;
            const command = agentSetup(row.id)?.command;
            const offPath = !opensApp && row.executable_on_path === false;
            const waitingForCursor = opensApp && !added && attempted.has(row.id) && !error;
            return (
              <SettingsRow
                key={row.id}
                icon={<HarnessIcon harness={row.id} size={14} />}
                label={row.display_name}
                status={
                  added ? (
                    <span className="flex items-center gap-1 text-small text-success">
                      <Check size={14} aria-hidden="true" />
                      Added
                    </span>
                  ) : waitingForCursor ? (
                    <span className="text-small text-text-tertiary">Confirm in Cursor</span>
                  ) : offPath ? (
                    <span className="text-small text-text-tertiary">Not on PATH</span>
                  ) : null
                }
                control={
                  offPath && command ? (
                    <CopyButton text={command} label="Copy command" />
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      className="min-w-24"
                      busy={pending === row.id}
                      disabled={connected === null || pending !== null}
                      onClick={() => void connect(row.id)}
                    >
                      {added ? "Reconnect" : opensApp ? "Add to Cursor" : "Connect"}
                    </Button>
                  )
                }
              />
            );
          })}
        </ul>
      )}
      {error && <SettingsError message={error} />}
      <p className="m-0 text-small text-text-tertiary">
        Another agent? Paste the setup prompt into it, or add{" "}
        <code className="font-mono text-text-secondary">{MCP_SERVER_COMMAND_LINE}</code> as a stdio
        MCP server.
      </p>
    </SettingsCard>
  );
}
