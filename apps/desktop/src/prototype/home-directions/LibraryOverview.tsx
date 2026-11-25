import { useEffect, useMemo } from "react";
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
import { currentListingRequest } from "../../components/Listing/useSkillVisibility";
import { ScanPartialBanner } from "../../components/SkillList/ScanPartialBanner";
import { issueRowState } from "../../components/SkillList/skill-row-state";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { useClientIds } from "../../hooks/useClientRows";
import { listingReportKey, useAppStore } from "../../store/appStore";
import { DashboardActivity } from "./DashboardActivity";
import "./overview.css";

const request = currentListingRequest(null);
const listingKey = listingReportKey(request);
const shortDate = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

function previewSummary(row: HarnessListing) {
  if (row.evidence === "discovery") return "Folder scan only";
  if (hasAssumedClaudeWindow(row)) return "Model window unknown";
  const counts = { described: 0, truncated: 0, name_only: 0, omitted: 0, hidden: 0 };
  for (const entry of row.entries) if (entry.scanned) counts[entry.state]++;
  const exceptions = [
    counts.omitted && counts.omitted + " omitted",
    counts.name_only && counts.name_only + " name only",
    counts.truncated && counts.truncated + " shortened",
  ].filter(Boolean);
  if (exceptions.length) return exceptions.join(" · ");
  return counts.described ? counts.described + " described" : "No visible entries";
}

function ClientVisibility({ clients, active }: { clients?: string[]; active: boolean }) {
  const load = useAppStore((s) => s.listingReports[listingKey]);
  const refresh = useAppStore((s) => s.refreshListing);
  const navigate = useAppStore((s) => s.setActiveView);
  const detectionError = useAppStore((s) => s.harnessDetection.error);
  const hasClients = Boolean(clients?.length);
  useEffect(() => {
    if (active && hasClients && !load) refresh(request);
  }, [active, hasClients, load, refresh]);
  return (
    <section aria-labelledby="visibility-title">
      <div className="overview-section-head">
        <h2 id="visibility-title">Client visibility</h2>
        <span>Global scope</span>
      </div>
      {!clients && <p role="status">{detectionError || "Detecting clients…"}</p>}
      {clients?.length === 0 && (
        <Button variant="outline" onClick={() => navigate({ kind: "settings" })}>
          Choose clients in Settings
        </Button>
      )}
      {hasClients && (
        <div className="overview-table-wrap">
          <table className="overview-table">
            <thead>
              <tr>
                <th>Client</th>
                <th>Found on disk</th>
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
                        <span className="overview-client-label">
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
                const capture = row?.observed_session;
                const when = capture?.at ? new Date(capture.at) : null;
                return (
                  <tr key={id}>
                    <th scope="row">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="overview-client-link"
                        onClick={() => navigate({ kind: "listing", scope: "global", harness: id })}
                      >
                        {icon && <HarnessIcon harness={icon} size={16} />}
                        {label}
                        <ChevronRight size={12} aria-hidden />
                      </Button>
                    </th>
                    <td>{row.entries.filter((entry) => entry.scanned).length.toLocaleString()}</td>
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
          <Button size="sm" variant="outline" onClick={() => refresh(request)}>
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
    return [...broken, ...warnings].filter(({ skill }) => {
      if (seen.has(skill.name)) return false;
      seen.add(skill.name);
      return true;
    });
  }, [library]);
  return (
    <div className="library-overview">
      <header className="overview-intro">
        <h2>Library overview</h2>
        <p>
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
      <ClientVisibility clients={clients} active={active} />
      <section aria-labelledby="attention-title">
        <div className="overview-section-head">
          <h2 id="attention-title">Needs attention</h2>
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
              className="overview-issue"
              onClick={() => open(issue.skill.name, path)}
            >
              <CircleAlert
                aria-hidden
                size={15}
                className={state.level === "error" ? "text-error" : "text-warning"}
              />
              <strong>{issue.skill.name}</strong>
              <span>{state.label}</span>
              <ChevronRight aria-hidden size={13} />
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
