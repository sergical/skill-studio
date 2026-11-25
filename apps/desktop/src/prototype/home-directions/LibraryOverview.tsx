import { useMemo } from "react";
import { ArrowUpRight, ChevronRight, CircleAlert } from "lucide-react";
import {
  attentionGroups,
  collectDashboardIssues,
  deploymentLabelFromAgentId,
  ownSkillsView,
} from "@skill-studio/lib";
import type { HarnessListing, SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { issueDeploymentPath } from "../../components/Home/home-inbox-data";
import { hasAssumedClaudeWindow } from "../../components/Listing/client-allocation";
import { useHomeListing } from "./useHomeListing";
import { ScanPartialBanner } from "../../components/SkillList/ScanPartialBanner";
import { issueRowState } from "../../components/SkillList/skill-row-state";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { useClientIds } from "../../hooks/useClientRows";
import { useAppStore } from "../../store/appStore";
import { DashboardActivity } from "./DashboardActivity";

const sectionHead = "mb-2 flex min-h-7 items-center justify-between gap-3";
const shortDate = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

function previewSummary(row: HarnessListing) {
  if (row.evidence === "discovery") return "Folder scan only";
  if (hasAssumedClaudeWindow(row)) return "Model window unknown";
  const counts = { described: 0, truncated: 0, name_only: 0, omitted: 0, hidden: 0 };
  for (const entry of row.entries) counts[entry.state]++;
  const summary = [
    counts.described && counts.described + " described",
    counts.omitted && counts.omitted + " omitted",
    counts.name_only && counts.name_only + " name only",
    counts.truncated && counts.truncated + " shortened",
  ].filter(Boolean);
  return summary.join(" · ") || "No visible entries";
}

function ClientVisibility({
  clients,
  active,
  revision,
}: {
  clients?: string[];
  active: boolean;
  revision: number;
}) {
  const navigate = useAppStore((s) => s.setActiveView);
  const detectionError = useAppStore((s) => s.harnessDetection.error);
  const hasClients = Boolean(clients?.length);
  const { load, retry } = useHomeListing(revision, active && hasClients);
  return (
    <section aria-labelledby="visibility-title">
      <div className={sectionHead}>
        <h2 id="visibility-title" className="text-heading font-semibold">
          Client visibility
        </h2>
        <span className="text-small text-text-secondary">Global scope</span>
      </div>
      {!clients && <p role="status">{detectionError || "Detecting clients…"}</p>}
      {clients?.length === 0 && (
        <Button variant="outline" onClick={() => navigate({ kind: "settings" })}>
          Choose clients in Settings
        </Button>
      )}
      {hasClients && (
        <div className="-mx-2 overflow-x-auto px-2 py-1">
          <table className="w-full border-collapse text-left text-body tabular-nums [&_th]:font-medium [&_tr]:border-b [&_tr]:border-border-subtle [&_td]:py-2 [&_th]:py-2 [&_td]:pr-2 [&_th]:pr-2 [&_small]:text-small [&_small]:text-text-secondary">
            <thead className="text-small text-text-secondary">
              <tr>
                <th>Client</th>
                <th title="Includes hidden entries in scanned folders">Found on disk</th>
                <th>Listing preview</th>
                <th>Last saved listing</th>
              </tr>
            </thead>
            <tbody>
              {clients?.map((id) => {
                const row = load?.data?.listings.find(
                  (item) => item.scope.scope === "global" && item.harness === id,
                );
                const label = deploymentLabelFromAgentId(id);
                const icon = harnessIdFromLabel(label);
                const waiting = !load?.data && !load?.error;
                if (!row)
                  return (
                    <tr key={id}>
                      <th scope="row">
                        <span className="inline-flex min-h-7 items-center gap-2">
                          {icon && <HarnessIcon harness={icon} size={16} />}
                          {label}
                        </span>
                      </th>
                      <td colSpan={3}>
                        <small>
                          {waiting ? "Reading global folders…" : "Global report unavailable"}
                        </small>
                      </td>
                    </tr>
                  );
                const found = row.entries.filter((entry) => entry.scanned);
                const hidden = found.filter((entry) => entry.state === "hidden").length;
                const capture = row.observed_session;
                const when = capture?.at ? new Date(capture.at) : null;
                return (
                  <tr key={id}>
                    <th scope="row">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="-ml-1 justify-start gap-2 px-1"
                        onClick={() => navigate({ kind: "listing", scope: "global", harness: id })}
                      >
                        {icon && <HarnessIcon harness={icon} size={16} />}
                        {label}
                        <ChevronRight size={12} aria-hidden />
                      </Button>
                    </th>
                    <td>
                      {found.length.toLocaleString()}
                      {hidden > 0 && <small> · {hidden} hidden</small>}
                    </td>
                    <td>{previewSummary(row)}</td>
                    <td>
                      {capture ? (
                        <>
                          {capture.listed.toLocaleString()} listed
                          {when && Number.isFinite(when.valueOf()) && (
                            <small> · {shortDate.format(when)}</small>
                          )}
                        </>
                      ) : (
                        <small>No capture</small>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
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
    </section>
  );
}

interface OverviewProps {
  snapshot: SkillSnapshot;
  active: boolean;
  usageRefreshing?: boolean;
  open: (name: string, path?: string) => void;
}

export function LibraryOverview({ snapshot, active, usageRefreshing, open }: OverviewProps) {
  const clients = useClientIds();
  const navigate = useAppStore((s) => s.setActiveView);
  const filter = useAppStore((s) => s.replaceSkillListFilter);
  const library = useMemo(() => ownSkillsView(snapshot.skills), [snapshot.skills]);
  const parked = library.filter((skill) => skill.parked).length;
  const installed = library.length - parked;
  const issues = useMemo(() => {
    const { broken, warnings } = attentionGroups(collectDashboardIssues(library));
    const seen = new Set<string>();
    return [...broken, ...warnings].filter((issue) => {
      const key =
        issue.kind === "linked-root" ? issue.harness + ":" + issue.root : issue.skill.name;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [library]);
  return (
    <div className="grid min-w-0 gap-6">
      <header>
        <h2 className="text-display leading-tight font-semibold tracking-tight">
          Library overview
        </h2>
        <p className="mt-1 text-small text-text-secondary">
          {installed.toLocaleString()} installed {installed === 1 ? "skill" : "skills"}
          {parked > 0 && <> · {parked} parked</>}
          {clients && (
            <>
              {" "}
              · {clients.length} {clients.length === 1 ? "client" : "clients"}
            </>
          )}
        </p>
      </header>
      {snapshot.scan_partial && (
        <ScanPartialBanner
          observations={snapshot.scan_observations}
          unreadLocationCount={snapshot.unread_roots.length}
        />
      )}
      <DashboardActivity
        snapshot={snapshot}
        clients={clients}
        usageRefreshing={usageRefreshing}
        open={open}
      />
      <ClientVisibility clients={clients} active={active} revision={snapshot.revision} />
      <section aria-labelledby="attention-title">
        <div className={sectionHead}>
          <h2 id="attention-title" className="text-heading font-semibold">
            Needs attention
          </h2>
          {issues.length > 0 && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                filter({ issue: "any" });
                navigate({ kind: "skills" });
              }}
            >
              View all issues <ArrowUpRight size={13} />
            </Button>
          )}
        </div>
        {issues.slice(0, 3).map((issue) => {
          const state = issueRowState(issue);
          const path =
            issue.kind === "broken-symlink"
              ? issue.skill.deployments.find((d) => d.symlink_is_broken)?.path
              : issueDeploymentPath(issue);
          return (
            <Button
              key={issue.kind + issue.skill.name + issue.detail}
              variant="ghost"
              className="flex h-auto min-h-9 w-full flex-wrap justify-start gap-2.5 rounded-none border-b border-border-subtle px-1 py-2 text-left"
              onClick={() => {
                if (issue.kind === "linked-root" && issue.harnessLabel) {
                  filter({ scope: "global", harness: issue.harnessLabel, issue: "linked-root" });
                  navigate({ kind: "skills" });
                } else open(issue.skill.name, path);
              }}
            >
              <CircleAlert
                aria-hidden
                size={15}
                className={state.level === "error" ? "text-error" : "text-warning"}
              />
              <strong className="font-medium">
                {issue.kind === "linked-root"
                  ? (issue.harnessLabel ?? issue.skill.name)
                  : issue.skill.name}
              </strong>
              <span className="text-small font-normal text-text-secondary">{state.label}</span>
              <ChevronRight aria-hidden size={13} className="ml-auto text-text-tertiary" />
            </Button>
          );
        })}
        {!issues.length && (
          <p className="text-small text-text-secondary">
            {snapshot.scan_partial
              ? "No issues in the folders scanned so far."
              : "No issues found in your library."}
          </p>
        )}
      </section>
    </div>
  );
}
