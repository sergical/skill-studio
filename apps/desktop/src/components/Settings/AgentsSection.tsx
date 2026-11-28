// ============================================================================
// AgentsSection - Settings' one table of agents: a row for every agent in
// the catalog, with whether Skill Studio shows its skills, whether its
// history is searched for project folders, and whether it has the Skill
// Studio MCP server. An agent detection did not find gets a row that says so.
// The agent's own `mcp add` command (or, for Cursor, its install dialog)
// writes its MCP config; this section only starts it and reads back which
// agents have the server. Any other agent gets the setup prompt.
// ============================================================================

import { Check } from "lucide-react";
import { Button } from "@skill-studio/ui";
import {
  AGENT_SETUP_PROMPT,
  HARNESS_CATALOG,
  MCP_SERVER_COMMAND_LINE,
  agentSetup,
  type AgentId,
  type DiscoverySourceSetting,
  type HarnessDetection,
} from "@skill-studio/lib";
import { changeHarnessEnabled, useDetectedHarnesses } from "../../hooks/useDetectedHarnesses";
import { useMcpConnections, type McpConnections } from "../../hooks/useMcpConnections";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon } from "../ui/HarnessIcon";
import { SwitchControl } from "../ui/SwitchControl";
import { CopyTextButton } from "../ui/CopyTextButton";
import { SettingsError } from "./SettingsCard";

const CATALOG_IDS = new Set<string>(HARNESS_CATALOG.map((row) => row.id));

function isCatalogAgent(id: string): id is AgentId {
  return CATALOG_IDS.has(id);
}

/** Cursor is an app with an install dialog, so its row needs no CLI on PATH. */
const OPENS_APP = "cursor";

interface AgentsSectionProps {
  /** Which agents' history is searched for project folders; `null` until read. */
  sources: DiscoverySourceSetting[] | null;
  onToggleSource: (agent: string, enabled: boolean) => void;
}

export function AgentsSection({ sources, onToggleSource }: AgentsSectionProps) {
  const { report, choice, error, saving } = useDetectedHarnesses();
  const mcp = useMcpConnections();
  const kept = new Set(choice?.kept ?? []);
  const searched = new Map((sources ?? []).map((source) => [source.harness, source.enabled]));
  const rows = (report?.harnesses ?? []).flatMap((row) =>
    isCatalogAgent(row.id) ? [{ ...row, id: row.id }] : [],
  );
  // An agent kept but no longer found keeps its row; the rest of the missing share one line.
  const isListed = (row: HarnessDetection) => row.state !== "not_found" || kept.has(row.id);
  const listed = rows.filter(isListed);
  const missing = rows.filter((row) => !isListed(row));

  return (
    <PageSection
      title="Agents"
      meta={<CopyTextButton text={AGENT_SETUP_PROMPT} label="Copy setup prompt" />}
    >
      <div className="flex flex-col gap-3">
        {error && <SettingsError message={error} />}
        {!report && !error && (
          <p role="status" className="m-0 text-small text-text-tertiary">
            Checking installed agents…
          </p>
        )}
        {rows.length > 0 && (
          <table className="w-full border-separate border-spacing-0 rounded-md border border-border-subtle text-body">
            <thead>
              <tr className="text-left text-small text-text-tertiary">
                <th scope="col" className="h-8 px-3 font-normal">
                  Agent
                </th>
                <th scope="col" className="w-28 px-3 font-normal">
                  Show skills
                </th>
                <th scope="col" className="w-32 px-3 font-normal">
                  Find projects
                </th>
                <th scope="col" className="w-44 px-3 font-normal">
                  MCP server
                </th>
              </tr>
            </thead>
            <tbody>
              {listed.map((row) => (
                <AgentRow
                  key={row.id}
                  row={row}
                  shown={kept.has(row.id)}
                  showDisabled={saving || choice === null}
                  searched={searched.get(row.id)}
                  onToggleSource={onToggleSource}
                  mcp={mcp}
                />
              ))}
              {missing.length > 0 && (
                <tr>
                  <td colSpan={4} className={`${CELL} py-2 text-small text-text-tertiary`}>
                    <span className="flex flex-wrap items-center gap-x-4 gap-y-1">
                      <span>Not found on this Mac</span>
                      {missing.map((row) => (
                        <span key={row.id} className="flex items-center gap-1.5">
                          <HarnessIcon harness={row.id} size={12} muted />
                          {row.display_name}
                        </span>
                      ))}
                    </span>
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
        {mcp.error && <SettingsError message={mcp.error} />}
        <p className="m-0 text-small text-text-tertiary">
          Another agent? Paste the setup prompt into it, or add{" "}
          <code className="font-mono text-text-secondary">{MCP_SERVER_COMMAND_LINE}</code> as a
          stdio MCP server.
        </p>
      </div>
    </PageSection>
  );
}

interface AgentRowProps {
  row: HarnessDetection & { id: AgentId };
  shown: boolean;
  showDisabled: boolean;
  /** Whether this agent's history is searched; `undefined` while the setting loads. */
  searched: boolean | undefined;
  onToggleSource: (agent: string, enabled: boolean) => void;
  mcp: McpConnections;
}

const CELL = "h-10 border-t border-border-subtle px-3 align-middle";

function AgentRow({ row, shown, showDisabled, searched, onToggleSource, mcp }: AgentRowProps) {
  const name = row.display_name;
  const offPath = row.id !== OPENS_APP && row.executable_on_path === false;
  return (
    <tr>
      <th scope="row" className={`${CELL} text-left font-normal`}>
        <span className="flex min-w-0 items-center gap-2">
          <HarnessIcon harness={row.id} size={14} />
          <span className="truncate text-text-primary">{name}</span>
          {offPath && <span className="shrink-0 text-small text-text-tertiary">Not on PATH</span>}
        </span>
      </th>
      <td className={CELL}>
        <SwitchControl
          checked={shown}
          disabled={showDisabled}
          ariaLabel={`Show ${name} skills`}
          title={`Show ${name}'s skills on every page`}
          onCheckedChange={(checked) => void changeHarnessEnabled(row.id, checked)}
        />
      </td>
      <td className={CELL}>
        {/* History is read only from agents whose skills are shown (`activity_sources` in
            skill_refresh.rs), so an agent left out gets no switch. */}
        {shown && searched !== undefined && (
          <SwitchControl
            checked={searched}
            ariaLabel={`Find projects in ${name} history`}
            title={`Look for project folders in ${name}'s sessions`}
            onCheckedChange={(enabled) => onToggleSource(row.id, enabled)}
          />
        )}
      </td>
      <td className={CELL}>
        <McpCell row={row} offPath={offPath} mcp={mcp} />
      </td>
    </tr>
  );
}

function McpCell({
  row,
  offPath,
  mcp,
}: {
  row: HarnessDetection & { id: AgentId };
  offPath: boolean;
  mcp: McpConnections;
}) {
  const { connected, pending, attempted, error, connect } = mcp;
  const name = row.display_name;
  const added = connected?.has(row.id) ?? false;
  const opensApp = row.id === OPENS_APP;
  const command = agentSetup(row.id)?.command;
  const waitingForCursor = opensApp && !added && attempted.has(row.id) && !error;

  return (
    <span className="flex items-center gap-2">
      {added && (
        <span className="flex items-center gap-1 text-small text-success">
          <Check size={14} aria-hidden="true" />
          Added
        </span>
      )}
      {waitingForCursor && <span className="text-small text-text-tertiary">Confirm in Cursor</span>}
      {offPath && command ? (
        <CopyTextButton text={command} label="Copy command" />
      ) : (
        <Button
          variant={added ? "ghost" : "outline"}
          size="sm"
          aria-label={added ? `Reconnect ${name}` : opensApp ? "Add to Cursor" : `Connect ${name}`}
          busy={pending === row.id}
          disabled={connected === null || pending !== null}
          onClick={() => void connect(row.id)}
        >
          {added ? "Reconnect" : opensApp ? "Add to Cursor" : "Connect"}
        </Button>
      )}
    </span>
  );
}
