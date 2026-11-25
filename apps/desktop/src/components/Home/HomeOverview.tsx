// ============================================================================
// HomeOverview - The top of Home: the two questions a skill author has after
// installing a skill. Did an agent use it in recent work (Skill use)? Did
// each agent get its description (What your agents see)? Each part opens the
// page that answers it in full: Activity, or the agent's What it sees tab.
// ============================================================================

import { ChevronRight } from "lucide-react";
import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type { HarnessListing, ListingState, SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { hasAssumedClaudeWindow } from "../Listing/client-allocation";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon, harnessIdFromLabel } from "../ui/HarnessIcon";
import { useClientIds } from "../../hooks/useClientRows";
import { useAppStore } from "../../store/appStore";
import { HomeActivity } from "./HomeActivity";
import { useHomeListing } from "./useHomeListing";

const shortDate = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

/** Counted states in the order they matter, phrased to follow a number. */
const COUNTED: [ListingState, string][] = [
  ["described", "with full description"],
  ["omitted", "not listed"],
  ["name_only", "name only"],
  ["truncated", "shortened"],
];

function seesNow(row: HarnessListing) {
  if (row.evidence === "discovery") return "Folders only, no estimate";
  if (hasAssumedClaudeWindow(row)) return "Model window unknown";
  const summary = COUNTED.flatMap(([state, words]) => {
    const count = row.entries.filter((entry) => entry.state === state).length;
    return count ? [`${count.toLocaleString()} ${words}`] : [];
  });
  return summary.join(" · ") || "No skills listed";
}

function AgentName({ id }: { id: string }) {
  const label = deploymentLabelFromAgentId(id);
  const icon = harnessIdFromLabel(label);
  return (
    <>
      {icon && <HarnessIcon harness={icon} size={16} />}
      {label}
    </>
  );
}

function AgentVisibility({
  agents,
  active,
  revision,
}: {
  agents?: string[];
  active: boolean;
  revision: number;
}) {
  const navigate = useAppStore((s) => s.setActiveView);
  const detectionError = useAppStore((s) => s.harnessDetection.error);
  const hasAgents = Boolean(agents?.length);
  const { load, retry } = useHomeListing(revision, active && hasAgents);
  const waiting = !load?.data && !load?.error;
  const reportFor = (id: string) =>
    load?.data?.listings.find((item) => item.scope.scope === "global" && item.harness === id);
  // Once the report is in, agents it doesn't cover share one line under the table instead of a
  // row each that says nothing.
  const rows = waiting ? agents : agents?.filter((id) => reportFor(id));
  const unreported = waiting ? [] : (agents?.filter((id) => !reportFor(id)) ?? []);
  return (
    <PageSection title="What your agents see" meta="Global skills">
      {!agents && <p role="status">{detectionError || "Finding your agents…"}</p>}
      {agents?.length === 0 && (
        <Button variant="outline" onClick={() => navigate({ kind: "settings" })}>
          Turn on agents in Settings
        </Button>
      )}
      {hasAgents && (
        <div className="-mx-2 overflow-x-auto px-2">
          <table className="w-full border-collapse text-left text-body tabular-nums [&_th]:font-medium [&_tr]:border-b [&_tr]:border-border-subtle [&_td]:py-2 [&_th]:py-2 [&_td]:pr-2 [&_th]:pr-2 [&_small]:text-small [&_small]:text-text-secondary">
            <thead className="text-small text-text-secondary">
              <tr>
                <th>Agent</th>
                <th title="Includes hidden skills in the folders it reads">In its folders</th>
                <th>What it sees now</th>
                <th>Last session</th>
              </tr>
            </thead>
            <tbody>
              {rows?.map((id) => {
                const row = reportFor(id);
                if (!row)
                  return (
                    <tr key={id}>
                      <th scope="row">
                        <span className="inline-flex min-h-7 items-center gap-2">
                          <AgentName id={id} />
                        </span>
                      </th>
                      <td colSpan={3}>
                        <small>Reading its folders…</small>
                      </td>
                    </tr>
                  );
                const found = row.entries.filter((entry) => entry.scanned);
                const hidden = found.filter((entry) => entry.state === "hidden").length;
                const session = row.observed_session;
                const when = session?.at ? new Date(session.at) : null;
                return (
                  <tr key={id}>
                    <th scope="row">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="-ml-1 justify-start gap-2 px-1"
                        onClick={() => navigate({ kind: "listing", scope: "global", harness: id })}
                      >
                        <AgentName id={id} />
                        <ChevronRight size={12} aria-hidden />
                      </Button>
                    </th>
                    <td>
                      {found.length.toLocaleString()}
                      {hidden > 0 && <small> · {hidden} hidden</small>}
                    </td>
                    <td>{seesNow(row)}</td>
                    <td>
                      {session ? (
                        <>
                          {session.listed.toLocaleString()} listed
                          {when && Number.isFinite(when.valueOf()) && (
                            <small> · {shortDate.format(when)}</small>
                          )}
                        </>
                      ) : (
                        <small>Not recorded</small>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {unreported.length > 0 && (
            <p className="mt-2 text-small text-text-secondary">
              No skill list report yet for {unreported.map(deploymentLabelFromAgentId).join(", ")}.
            </p>
          )}
        </div>
      )}
      {load?.error && (
        <div role="alert" className="mt-2 flex items-center gap-2 text-small text-text-secondary">
          {load.error}
          <Button size="sm" variant="outline" onClick={retry}>
            Retry
          </Button>
        </div>
      )}
    </PageSection>
  );
}

interface HomeOverviewProps {
  snapshot: SkillSnapshot;
  active: boolean;
  usageRefreshing?: boolean;
  open: (name: string) => void;
}

export function HomeOverview({ snapshot, active, usageRefreshing, open }: HomeOverviewProps) {
  const agents = useClientIds();
  return (
    <>
      <HomeActivity
        snapshot={snapshot}
        clients={agents}
        usageRefreshing={usageRefreshing}
        open={open}
      />
      <AgentVisibility agents={agents} active={active} revision={snapshot.revision} />
    </>
  );
}
