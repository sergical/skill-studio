// ============================================================================
// AgentMcpCard - adds the Skill Studio MCP server to Claude Code or Codex.
// The agent's own `mcp add` command writes its config; this card only runs
// it and reads back which agents have the server.
// ============================================================================

import { Check, Plug } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { AgentId } from "@skill-studio/lib";
import { useDetectedHarnesses } from "../../hooks/useDetectedHarnesses";
import { useMcpConnections } from "../../hooks/useMcpConnections";
import { SettingsCard } from "./SettingsCard";

const CONNECTABLE_AGENTS: ReadonlySet<string> = new Set<AgentId>(["claude-code", "codex"]);

export function AgentMcpCard() {
  const { report } = useDetectedHarnesses();
  const { connected, pending, error, connect } = useMcpConnections();

  const rows =
    report?.harnesses.filter(
      (row) => CONNECTABLE_AGENTS.has(row.id) && row.state !== "not_found",
    ) ?? [];

  return (
    <SettingsCard
      icon={<Plug size={16} />}
      title="Use from your agent"
      description="Add the Skill Studio MCP server so your agent can check, fix and park skills. The agent's own mcp add command writes its config."
    >
      {error && <p role="alert">{error}</p>}
      {rows.map((row) => (
        <div key={row.id} className="flex items-center gap-2 text-body">
          <span className="flex-1">{row.display_name}</span>
          {connected?.has(row.id) ? (
            <span className="flex items-center gap-1 text-small text-text-tertiary">
              <Check size={14} />
              Connected
            </span>
          ) : row.executable_on_path === false ? (
            <span className="text-small text-text-tertiary">Not on PATH</span>
          ) : (
            <Button
              variant="secondary"
              disabled={connected === null || pending !== null}
              onClick={() => void connect(row.id)}
            >
              {pending === row.id ? "Connecting…" : "Connect"}
            </Button>
          )}
        </div>
      ))}
      <p className="m-0 text-small text-text-tertiary">
        For other agents, add <code>npx -y skill-studio mcp</code> as a stdio MCP server.
      </p>
    </SettingsCard>
  );
}
