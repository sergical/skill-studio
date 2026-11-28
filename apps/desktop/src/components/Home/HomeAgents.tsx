// ============================================================================
// HomeAgents - One tile per set-up agent: what it sees, what its skill list
// cuts and its uses this week; each opens its What it sees tab. The agents
// not set up share one line.
// ============================================================================

import { ChevronRight } from "lucide-react";
import { formatCount } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { HarnessIcon, harnessIdFromLabel } from "../ui/HarnessIcon";
import type { HomeAgent } from "./home-agents";
import { plural } from "./home-asks";

interface HomeAgentsProps {
  agents: HomeAgent[];
  missing: { id: string; label: string }[];
  /** One agent's uses would repeat the week's total, so tiles show uses only for two or more. */
  showUses: boolean;
  open: (agent: string) => void;
}

export function HomeAgents({ agents, missing, showUses, open }: HomeAgentsProps) {
  return (
    <ul aria-label="Agents" className="-mx-3 flex flex-wrap gap-1">
      {agents.map((agent) => {
        const icon = harnessIdFromLabel(agent.label);
        const cut = [
          agent.nameOnly?.length ? `${formatCount(agent.nameOnly.length)} name only` : "",
          agent.notListed?.length ? `${formatCount(agent.notListed.length)} not listed` : "",
        ].filter(Boolean);
        return (
          <li key={agent.id} className="flex w-36">
            <Button
              variant="ghost"
              className="h-auto min-w-0 flex-1 flex-col items-stretch justify-start gap-0.5 rounded-md px-3 py-2 text-left font-normal whitespace-normal"
              onClick={() => open(agent.id)}
            >
              <span className="flex min-w-0 items-center gap-1.5 text-caption text-text-secondary">
                {icon && <HarnessIcon harness={icon} size={12} />}
                <span className="truncate">{agent.label}</span>
                <ChevronRight
                  aria-hidden
                  className="-ml-0.5 size-3 shrink-0 opacity-40 group-hover/button:opacity-100 group-focus-visible/button:opacity-100"
                />
              </span>
              <span className="text-caption text-text-tertiary tabular-nums">
                <span className="text-body font-semibold text-text-primary">
                  {agent.sees === null ? "–" : formatCount(agent.sees)}
                </span>{" "}
                {agent.seesUnit}
              </span>
              {agent.noEstimate && (
                <span className="truncate text-caption text-text-tertiary">No estimate</span>
              )}
              {showUses && (
                <span className="truncate text-caption text-text-tertiary tabular-nums">
                  {agent.uses === null
                    ? "No readable history"
                    : `${plural(agent.uses, "use")} this week`}
                </span>
              )}
              {cut.length > 0 && (
                <span className="truncate text-caption text-warning tabular-nums">
                  {cut.join(" · ")}
                </span>
              )}
            </Button>
          </li>
        );
      })}
      {missing.length > 0 && (
        <li
          className="flex items-start gap-1.5 px-3 py-2 text-caption text-text-tertiary"
          title={`Not set up: ${missing.map((row) => row.label).join(", ")}`}
        >
          {missing.map((row) => {
            const icon = harnessIdFromLabel(row.label);
            return (
              icon && (
                <span key={row.id} className="opacity-50" aria-hidden>
                  <HarnessIcon harness={icon} size={12} />
                </span>
              )
            );
          })}
          <span className="whitespace-nowrap">{formatCount(missing.length)} not set up</span>
          <span className="sr-only">: {missing.map((row) => row.label).join(", ")}</span>
        </li>
      )}
    </ul>
  );
}
