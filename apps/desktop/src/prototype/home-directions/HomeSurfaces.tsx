import { useEffect, useMemo } from "react";
import { ArrowRight, Check, ChevronRight, CircleAlert, Clock3 } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { deploymentLabel, deploymentLabelFromAgentId } from "@skill-studio/lib";
import type { HealthIssue, SkillSnapshot } from "@skill-studio/lib";
import { computeHomeGroups, issueDeploymentPath } from "../../components/Home/home-inbox-data";
import { issueRowState } from "../../components/SkillList/skill-row-state";
import { currentListingRequest } from "../../components/Listing/useSkillVisibility";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { useClientIds } from "../../hooks/useClientRows";
import { listingReportKey, sessionsReportKey, useAppStore } from "../../store/appStore";

interface HomeData {
  snapshot: SkillSnapshot;
  open: (name: string, deploymentPath?: string) => void;
}

const title = "text-[28px] leading-tight font-semibold tracking-[-.025em] text-text-primary";
const row =
  "h-auto w-full min-w-0 justify-start gap-3 rounded-none border-b border-border-subtle px-1 py-3 text-left font-normal";
const sub = "text-small text-text-secondary";
const request = currentListingRequest(null);
const reportKey = listingReportKey(request);

function IssueRow({
  issue,
  open,
  path = issueDeploymentPath(issue),
}: {
  issue: HealthIssue;
  open: HomeData["open"];
  path?: string;
}) {
  const state = issueRowState(issue);
  return (
    <Button variant="ghost" className={row} onClick={() => open(issue.skill.name, path)}>
      <CircleAlert
        size={16}
        className={state.level === "error" ? "shrink-0 text-error" : "shrink-0 text-warning"}
      />
      <span className="flex min-w-0 flex-1 flex-col items-start gap-1">
        <span className="flex max-w-full flex-wrap items-baseline gap-x-3 gap-y-1">
          <span className="break-all font-medium">{issue.skill.name}</span>
          <span className="text-caption text-text-secondary">{state.label}</span>
        </span>
        <span className="max-w-full whitespace-normal break-words text-small font-normal text-text-secondary [overflow-wrap:anywhere]">
          {issue.detail}
        </span>
      </span>
      <ChevronRight aria-hidden size={15} className="shrink-0 text-text-tertiary" />
    </Button>
  );
}

export function RepairQueue({ snapshot, open }: HomeData) {
  const groups = useMemo(() => computeHomeGroups(snapshot), [snapshot]);
  const navigate = useAppStore((s) => s.setActiveView);
  const setFilter = useAppStore((s) => s.replaceSkillListFilter);
  const seenLinks = new Set<string>();
  const repairs = groups.broken.flatMap((issue) => {
    if (issue.kind !== "broken-symlink") return [{ issue, path: issueDeploymentPath(issue) }];
    if (seenLinks.has(issue.skill.name)) return [];
    seenLinks.add(issue.skill.name);
    return issue.skill.deployments
      .filter((copy) => copy.symlink_is_broken)
      .map((copy) => ({
        issue: {
          ...issue,
          detail: deploymentLabel(copy) + " → " + (copy.symlink_target ?? copy.path),
        },
        path: copy.path,
      }));
  });
  return (
    <section className="mx-auto max-w-[880px] space-y-6" aria-label="Repair queue">
      <div className="border-b border-border-subtle pb-5">
        <h2 className={title}>
          {groups.broken.length
            ? `${groups.broken.length} ${groups.broken.length === 1 ? "issue" : "issues"} to fix`
            : "No broken skills"}
        </h2>
        {!groups.broken.length && (
          <p className="mt-2 flex items-center gap-2 text-small text-text-secondary">
            <Check size={15} /> No broken links or rejected skill files found.
          </p>
        )}
      </div>
      {groups.broken.length > 0 && (
        <div>
          {repairs.map(({ issue, path }, i) => (
            <IssueRow
              key={path ?? issue.skill.name + issue.kind + i}
              issue={issue}
              path={path}
              open={open}
            />
          ))}
        </div>
      )}
      {groups.warnings.length > 0 && (
        <details className="border-b border-border-subtle pb-3">
          <summary className="cursor-pointer rounded-sm py-1 text-body font-medium focus-visible:outline-accent">
            {groups.warnings.length} warnings
          </summary>
          {groups.warnings.map((issue, i) => (
            <IssueRow key={issue.skill.name + issue.kind + i} issue={issue} open={open} />
          ))}
        </details>
      )}
      <div className="flex flex-wrap items-center gap-3">
        {groups.updates.length > 0 && (
          <Button
            variant="outline"
            onClick={() => {
              setFilter({ update: "available" });
              navigate({ kind: "skills" });
            }}
          >
            Review {groups.updates.length} updates <ArrowRight size={14} />
          </Button>
        )}
        <Button
          variant="ghost"
          onClick={() => {
            setFilter({});
            navigate({ kind: "skills" });
          }}
        >
          Open skill library <ArrowRight size={14} />
        </Button>
      </div>
    </section>
  );
}

export function ClientDirectory() {
  const clients = useClientIds();
  const navigate = useAppStore((s) => s.setActiveView);
  const load = useAppStore((s) => s.listingReports[reportKey]);
  const refresh = useAppStore((s) => s.refreshListing);
  useEffect(() => {
    if (!useAppStore.getState().listingReports[reportKey]) refresh(request);
  }, [refresh]);
  return (
    <section aria-label="Client overview" className="space-y-6">
      <header>
        <h2 className={title}>Your clients</h2>
        <p className="mt-2 text-body text-text-secondary">
          Skills in each client’s global configuration.
        </p>
      </header>
      <div className="grid grid-cols-2 gap-3">
        {clients?.map((id) => {
          const label = deploymentLabelFromAgentId(id),
            icon = harnessIdFromLabel(label);
          const listing = load?.data?.listings.find(
            (r) => r.scope.scope === "global" && r.harness === id,
          );
          const preview = listing?.evidence === "preview";
          const count = listing?.entries.filter((e) =>
            preview ? e.state !== "hidden" && e.state !== "omitted" : e.state !== "hidden",
          ).length;
          return (
            <Button
              key={id}
              variant="outline"
              className="h-auto min-h-[164px] flex-col items-stretch justify-between gap-6 whitespace-normal rounded-lg border-border-subtle bg-bg-elevated p-5 text-left"
              onClick={() => navigate({ kind: "listing", harness: id })}
            >
              <span className="flex items-center gap-2.5 text-heading font-medium">
                {icon && <HarnessIcon harness={icon} size={20} />} {label}
                <ChevronRight size={14} className="ml-auto text-text-tertiary" />
              </span>
              <span className="flex flex-col items-start gap-1">
                {count !== undefined && (
                  <span className="text-[29px] font-medium leading-none tracking-tight tabular-nums">
                    {count}
                  </span>
                )}
                <span className={sub}>
                  {listing
                    ? preview
                      ? "Skills in listing preview"
                      : "Skills found in folders"
                    : load?.error
                      ? "Report unavailable"
                      : load?.data
                        ? "No global report"
                        : "Reading configuration…"}
                </span>
              </span>
            </Button>
          );
        })}
      </div>
      {clients && !clients.length && (
        <Button variant="outline" onClick={() => navigate({ kind: "settings" })}>
          Choose clients
        </Button>
      )}
      {!clients && (
        <p role="status" className={sub}>
          Checking configured clients…
        </p>
      )}
      {load?.error && (
        <Button variant="outline" onClick={() => refresh(request)}>
          Retry global reports
        </Button>
      )}
    </section>
  );
}

const sessionRequest = { project_path: null, sessions: 10 };
const sessionKey = sessionsReportKey(sessionRequest);
const day = new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric", year: "numeric" });
const clock = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dateOf = (at: string | null) => {
  const stamp = Date.parse(at ?? "");
  return Number.isNaN(stamp) ? null : new Date(stamp);
};

export function RecentWork({ snapshot, open }: HomeData) {
  const navigate = useAppStore((s) => s.setActiveView);
  const clients = useClientIds();
  const load = useAppStore((s) => s.sessionsReports[sessionKey]);
  const refresh = useAppStore((s) => s.refreshSkillSessions);
  useEffect(() => {
    if (!useAppStore.getState().sessionsReports[sessionKey]) refresh(sessionRequest);
  }, [refresh]);
  const sessions = useMemo(() => {
    const rows = (load?.data?.sessions ?? []).filter(
      (s) => !clients || clients.includes(s.harness),
    );
    return rows
      .sort((a, b) => (dateOf(b.at)?.getTime() ?? 0) - (dateOf(a.at)?.getTime() ?? 0))
      .slice(0, 8);
  }, [load?.data, clients]);
  const installed = useMemo(() => new Set(snapshot.skills.map((s) => s.name)), [snapshot.skills]);
  const dayOf = (at: string | null) => {
    const date = dateOf(at);
    return date ? day.format(date) : "Date not recorded";
  };
  return (
    <section aria-label="Recent sessions" className="mx-auto max-w-[880px]">
      <header className="mb-6 flex items-start justify-between gap-4">
        <h2 className={title}>Recent sessions</h2>
        <Button variant="ghost" onClick={() => navigate({ kind: "sessions" })}>
          All sessions <ArrowRight size={14} />
        </Button>
      </header>
      {!load?.data && !load?.error && (
        <p role="status" className={sub}>
          Reading local session history…
        </p>
      )}
      {load?.error && (
        <div className="flex flex-wrap items-center gap-2">
          <p role="alert">{load.error}</p>
          <Button variant="outline" onClick={() => refresh(sessionRequest)}>
            Try again
          </Button>
        </div>
      )}
      {load?.data && sessions.length === 0 && (
        <div className="flex items-center gap-3 border-y border-border-subtle py-5">
          <Clock3 size={18} />
          <p>No sessions found for the selected clients.</p>
        </div>
      )}
      {sessions.map((session, index) => {
        const date = dateOf(session.at);
        const label = deploymentLabelFromAgentId(session.harness);
        const icon = harnessIdFromLabel(label);
        const folder =
          session.project_path?.match(/\/\.t3\/worktrees\/([^/]+)\//)?.[1] ??
          session.project_path?.split("/").filter(Boolean).pop();
        const namedSkills = session.used
          .slice(0, 2)
          .map((use) => use.skill)
          .join(", ");
        const uses = session.used.length;
        const previous = sessions[index - 1];
        const newGroup =
          !previous ||
          previous.project_path !== session.project_path ||
          dayOf(previous.at) !== dayOf(session.at);
        return (
          <div key={session.harness + (session.id ?? session.transcript)}>
            {newGroup && (
              <h3 className="mt-5 mb-1 border-b border-border-subtle pb-2 text-small font-medium text-text-secondary">
                <span
                  className="mr-3 text-body font-medium text-text-primary"
                  title={session.project_path ?? undefined}
                >
                  {folder ?? "Working folder not recorded"}
                </span>
                {dayOf(session.at)}
              </h3>
            )}
            <details className="group border-b border-border-subtle">
              <summary className="flex min-h-14 cursor-pointer list-none items-center gap-4 rounded-sm px-2 py-2 outline-offset-1 hover:bg-bg-secondary focus-visible:outline-2 focus-visible:outline-accent [&::-webkit-details-marker]:hidden">
                {date && (
                  <time
                    dateTime={session.at ?? undefined}
                    className="w-[74px] shrink-0 text-small tabular-nums text-text-secondary"
                  >
                    {clock.format(date)}
                  </time>
                )}
                {icon && <HarnessIcon harness={icon} size={18} />}
                <span className="flex min-w-0 flex-1 flex-col gap-1">
                  <span
                    className="truncate text-body font-medium"
                    title={session.project_path ?? label}
                  >
                    {namedSkills ||
                      (session.listing_recorded ? "Saved skill listing" : "Session record")}
                  </span>
                  <span className="text-small text-text-secondary">{label}</span>
                </span>
                <span className="text-small text-text-secondary">
                  {uses > 1 ? `${uses} skills used` : ""}
                </span>
                <ChevronRight aria-hidden size={14} className="shrink-0 group-open:rotate-90" />
              </summary>
              <div className="space-y-2 px-4 pt-2 pb-4">
                {session.project_path && (
                  <p className="break-all font-mono text-caption text-text-secondary">
                    {session.project_path}
                  </p>
                )}
                <p className={sub}>
                  {session.listing_recorded
                    ? `${session.listed} entries in the saved listing.`
                    : "No skill listing in the saved record."}
                </p>
                {uses > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {session.used.map((use) =>
                      installed.has(use.skill) ? (
                        <Button
                          key={use.skill}
                          variant="outline"
                          size="sm"
                          onClick={() => open(use.skill)}
                        >
                          {use.skill}
                          <ChevronRight size={12} />
                        </Button>
                      ) : (
                        <span key={use.skill} className="text-small">
                          {use.skill}
                        </span>
                      ),
                    )}
                  </div>
                ) : (
                  <p className={sub}>No skill use recorded in this session.</p>
                )}
              </div>
            </details>
          </div>
        );
      })}
    </section>
  );
}
