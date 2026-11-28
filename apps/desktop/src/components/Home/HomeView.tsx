// ============================================================================
// HomeView - Your skills at a glance: each set-up agent and what it sees, the
// week's uses, and what needs the user as counts that open Skills plus one
// prompt to hand all of it to an agent with the Skill Studio MCP server.
// ============================================================================

import { Plus } from "lucide-react";
import { HARNESS_CATALOG } from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { useClientIds } from "../../hooks/useClientRows";
import { useMcpConnections } from "../../hooks/useMcpConnections";
import { useAppStore } from "../../store/appStore";
import { hasReadableHistory } from "../Listing/client-allocation";
import { PageShell } from "../Shell/PageShell";
import { dailyUses, homeAgent } from "./home-agents";
import { homeAsks, homeCounts } from "./home-asks";
import { HomeAgents } from "./HomeAgents";
import { HomeAsk } from "./HomeAsk";
import { HomeWeek } from "./HomeWeek";
import { useHomeListing } from "./useHomeListing";
import { useUpdateCheck } from "./useUpdateCheck";

const NO_UPDATE_CHECK: SkillSnapshot["update_check"] = {
  checked_at: null,
  gh_status: "ok",
  message: null,
  updates_available: 0,
  upstream_ahead: [],
};

interface HomeViewProps {
  snapshot: SkillSnapshot | undefined;
  isLoading: boolean;
  /** Whether Home is the view on screen right now - `false` while it's kept mounted but hidden
   * behind an open skill's page, so it reads agents' skill lists only while shown. */
  active: boolean;
}

/** First-scan loading state in the loaded layout's shape, so Home fades in without a jump and
 * "No skills yet" only ever means the scan found nothing. */
function HomeSkeleton() {
  const bar = "animate-pulse rounded-xs bg-bg-tertiary motion-reduce:animate-none";
  return (
    <PageShell title="Home">
      <div className="flex max-w-4xl flex-col gap-6" aria-hidden="true">
        <div className="flex gap-1">
          {[0, 1, 2].map((i) => (
            <div key={i} className="flex w-36 flex-col gap-1.5 px-0 py-2">
              <div className={`h-3 w-20 ${bar}`} />
              <div className={`h-4 w-24 ${bar}`} />
            </div>
          ))}
        </div>
        <div className="flex items-end justify-between gap-8">
          <div className="flex items-end gap-4">
            <div className={`h-9 w-16 ${bar}`} />
            <div className={`h-8 w-[5.5rem] ${bar}`} />
          </div>
          <div className={`h-8 w-56 ${bar}`} />
        </div>
      </div>
      <span className="sr-only" role="status">
        Scanning installed skills…
      </span>
    </PageShell>
  );
}

export function HomeView({ snapshot, isLoading, active }: HomeViewProps) {
  const updateCheck = useUpdateCheck(snapshot?.update_check ?? NO_UPDATE_CHECK);
  const navigate = useAppStore((s) => s.setActiveView);
  const replaceFilter = useAppStore((s) => s.replaceSkillListFilter);
  const openAddSkillSheet = useAppStore((s) => s.openAddSkillSheet);
  const kept = useClientIds();
  const { load, retry } = useHomeListing(
    snapshot?.revision ?? 0,
    active && snapshot !== undefined && Boolean(kept?.length),
  );
  const { connected } = useMcpConnections();

  if (!snapshot) {
    if (isLoading) return <HomeSkeleton />;
    return (
      <PageShell title="Home">
        <p className="flex h-full items-center justify-center text-wrap-pretty text-text-tertiary">
          No skill snapshot yet.
        </p>
      </PageShell>
    );
  }

  // Plugin skills and history count too: a user with only plugins still has a week to report.
  if (snapshot.skills.length === 0 && snapshot.invocations.length === 0)
    return (
      <PageShell title="Home">
        <section aria-labelledby="home-empty-title">
          <h2 id="home-empty-title" className="text-heading font-semibold">
            No skills yet
          </h2>
          <p className="mt-1 text-small text-text-secondary">
            Add a skill and Skill Studio shows here whether your agents can see it and use it.
          </p>
          <Button className="mt-3" onClick={() => openAddSkillSheet()}>
            <Plus size={14} aria-hidden /> Add a skill
          </Button>
        </section>
      </PageShell>
    );

  const listings = (load?.data?.listings ?? []).filter(
    (row) => row.scope.scope === "global" && (!kept || kept.includes(row.harness)),
  );
  const shown = new Set(kept ?? listings.map((row) => row.harness));
  const setUp = HARNESS_CATALOG.filter((row) => shown.has(row.id));
  const missing = HARNESS_CATALOG.filter((row) => !shown.has(row.id));
  const agentIds = setUp.map((row) => row.id);
  const days = dailyUses(snapshot, agentIds);
  const agents = setUp.map((row) =>
    homeAgent(
      row,
      listings.find((r) => r.harness === row.id),
      snapshot.invocations,
      days.byAgent.get(row.id),
    ),
  );
  // Only the kept agents' history counts: an agent turned off adds no use and no "unused".
  const hasHistory = agentIds.some((id) => hasReadableHistory(snapshot.invocations, id));
  const { tasks, chips } = homeAsks(homeCounts(snapshot, agentIds), agents);
  // Only a read that came back can say no agent has the server; until then, say nothing.
  const noServer = connected !== null && !setUp.some((row) => connected.has(row.id));

  return (
    <PageShell title="Home">
      <section aria-label="Your skills at a glance" className="flex max-w-4xl flex-col gap-6">
        <HomeAgents
          agents={agents}
          missing={missing}
          showUses={agents.length > 1 && hasHistory}
          open={(harness) => navigate({ kind: "listing", scope: "global", harness })}
        />
        {load?.error && (
          <p role="alert" className="-mt-3 flex items-center gap-2 text-small text-error">
            Couldn’t read your agents’ skill lists: {load.error}
            <Button variant="ghost" size="xs" onClick={retry}>
              Try again
            </Button>
          </p>
        )}
        <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-5">
          <HomeWeek
            days={hasHistory ? days : null}
            agentLabels={setUp.map((row) => row.label)}
            openActivity={() => navigate({ kind: "activity" })}
          />
          <HomeAsk
            tasks={tasks}
            chips={chips}
            noServer={noServer}
            updateCheck={updateCheck}
            openCount={(chip) => {
              replaceFilter(chip.filter);
              navigate({ kind: "skills" });
            }}
            openSettings={() => navigate({ kind: "settings" })}
          />
        </div>
      </section>
    </PageShell>
  );
}
