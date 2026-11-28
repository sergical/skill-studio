import { useState } from "react";
import { ChevronRight } from "lucide-react";
import {
  agentIdFromDeploymentLabel,
  deploymentLabelFromAgentId,
  historyByHarness,
  homeRelativePath,
  LISTING_STATE_LABEL,
} from "@skill-studio/lib";
import type {
  AgentId,
  Deployment,
  HarnessListing,
  InstalledSkill,
  ListingEntry,
  ListingState,
  SkillInvocationStats,
} from "@skill-studio/lib";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger, cn } from "@skill-studio/ui";
import { useClientRows } from "../../hooks/useClientRows";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon } from "../ui/HarnessIcon";
import { hasAssumedClaudeWindow } from "../Listing/client-allocation";
import { useSkillVisibility } from "../Listing/useSkillVisibility";

interface KnownListing {
  /** A recorded session is what the agent showed; an estimate is what it would show now. */
  source: "session" | "estimate";
  state: ListingState;
  shownChars: number | null;
  text: NonNullable<ListingEntry["description_preview"]> | null;
}

/** What is known of this skill in the agent's skill list: the last session's record when
 * there is one, else the estimate. The estimate's text is cut where the session cut it. */
function knownListing(row: HarnessListing, entry: ListingEntry): KnownListing {
  const preview = entry.description_preview ?? null;
  if (!row.observed_session || !entry.observed)
    return {
      source: "estimate",
      state: entry.state,
      shownChars: entry.shown_description_chars,
      text: preview,
    };
  const shownChars = entry.observed_description_chars ?? null;
  const full = preview ? Array.from(preview.shown + preview.remainder) : null;
  const cut = entry.observed === "truncated" ? shownChars : null;
  // The session recorded a state and a length, not the text. Today's text stands in only when
  // the session showed a description and its length still matches today's.
  const shownText =
    entry.observed === "truncated" ||
    (entry.observed === "described" && shownChars === entry.full_description_chars);
  return {
    source: "session",
    state: entry.observed,
    shownChars,
    text:
      full && shownText
        ? {
            shown: (cut == null ? full : full.slice(0, cut)).join(""),
            remainder: cut == null ? "" : full.slice(cut).join(""),
          }
        : null,
  };
}

interface AgentRow {
  harness: string;
  row: HarnessListing | null;
  entry: ListingEntry | null;
}

const CUT_STATES: ReadonlySet<ListingState> = new Set(["truncated", "name_only", "omitted"]);

interface AgentState {
  label: string;
  /** Shortened, Name only or Not listed: the description was cut, shown in the warning color. */
  cut: boolean;
  /** Where the state comes from, and the characters kept when cut. */
  note: string;
}

/** The skill-list cell: the state word, and where it comes from. */
function stateOf({ row, entry }: AgentRow): AgentState {
  if (!row || !entry) return { label: "Not in its list", cut: false, note: "" };
  if (row.evidence !== "preview") {
    if (entry.hidden_reason) return { label: LISTING_STATE_LABEL.hidden, cut: false, note: "" };
    if (row.observed_session && entry.observed)
      return {
        label: LISTING_STATE_LABEL[entry.observed],
        cut: CUT_STATES.has(entry.observed),
        note: "Last session",
      };
    return { label: "No estimate", cut: false, note: "Found in a folder it reads" };
  }
  const known = knownListing(row, entry);
  const cut = CUT_STATES.has(known.state);
  const chars =
    known.state === "truncated" && known.shownChars != null
      ? ` · ${known.shownChars.toLocaleString()} of ${entry.full_description_chars.toLocaleString()} characters`
      : "";
  const source =
    known.source === "session"
      ? "Last session"
      : hasAssumedClaudeWindow(row)
        ? "Estimate, model window assumed"
        : "Estimate";
  return { label: LISTING_STATE_LABEL[known.state], cut, note: `${source}${chars}` };
}

/** What an opened row adds to its one line: the cut text, why it is hidden, or where the agent
 * found it. Nothing for a full description: the page's header already shows it. */
function AgentDetail({ row, entry }: { row: HarnessListing; entry: ListingEntry }) {
  const name = deploymentLabelFromAgentId(row.harness);
  if (row.evidence !== "preview")
    return (
      <div className="flex flex-col gap-1 text-small text-text-secondary">
        {entry.hidden_reason && <p>{entry.hidden_reason}</p>}
        {entry.path && (
          <p className="break-all font-mono text-caption">{homeRelativePath(entry.path)}</p>
        )}
        {row.observed_session?.model && <p>Last session · {row.observed_session.model}</p>}
      </div>
    );
  const known = knownListing(row, entry);
  const changed = known.source === "session" && entry.state !== known.state;
  return (
    <div className="flex flex-col gap-2 text-small text-text-secondary">
      {(known.state === "hidden" || known.state === "omitted") && (
        <p>{entry.hidden_reason ?? `${name} does not show this skill to the model.`}</p>
      )}
      {known.state === "truncated" && (
        <>
          {row.thresholds?.tier === "descriptions_shortened" ? (
            <p>
              {name} keeps about{" "}
              <strong className="font-medium text-text-primary">
                {row.thresholds.average_description_chars.toLocaleString()} characters
              </strong>{" "}
              of each description
              {row.setting && (
                <code className="ml-2 text-caption text-text-tertiary">{row.setting.key}</code>
              )}
            </p>
          ) : (
            row.description_char_limit != null && (
              <p>
                Per-skill limit{" "}
                <strong className="font-medium text-text-primary">
                  {row.description_char_limit.toLocaleString()} characters
                </strong>
                {row.description_limit_setting ? (
                  <code className="ml-2 text-caption text-text-tertiary">
                    {row.description_limit_setting.key}
                  </code>
                ) : (
                  <span className="ml-2 text-caption text-text-tertiary">fixed</span>
                )}
              </p>
            )
          )}
          {known.text ? (
            <div className="select-text rounded-md border border-border-subtle bg-bg-secondary px-3 py-2 text-body leading-relaxed text-text-primary">
              <p>
                <strong className="font-mono text-small">{entry.name}</strong> · {known.text.shown}
              </p>
              {known.text.remainder && (
                <div className="mt-2 border-t border-dashed border-border pt-2 text-text-tertiary">
                  <span className="text-caption font-medium">Beyond the cutoff</span>
                  <p className="mt-0.5 text-small">{known.text.remainder}</p>
                </div>
              )}
            </div>
          ) : (
            <p>
              {known.source === "session" && entry.description_preview
                ? "The description changed after this session."
                : "Description text unavailable for this copy."}
            </p>
          )}
          {known.source === "session" && known.text && (
            <p className="text-caption text-text-tertiary">
              Today’s text, cut where the last session cut it
            </p>
          )}
        </>
      )}
      {changed && <p>With the current settings · {LISTING_STATE_LABEL[entry.state]}</p>}
    </div>
  );
}

/** Whether an opened row would show anything its one line does not. */
function hasDetail({ row, entry }: AgentRow): boolean {
  if (!row || !entry) return false;
  if (row.evidence !== "preview") return Boolean(entry.hidden_reason || entry.path);
  const known = knownListing(row, entry);
  return (
    known.state === "truncated" ||
    known.state === "hidden" ||
    known.state === "omitted" ||
    (known.source === "session" && entry.state !== known.state)
  );
}

/** "3 uses", "No uses", or "No readable history" when the agent's history has no record at all. */
function usesLabel(harness: string, stats: SkillInvocationStats[], name: string): string {
  const uses = stats.find((s) => s.skill === name)?.by_harness_30_days[harness] ?? 0;
  if (uses > 0) return `${uses.toLocaleString()} ${uses === 1 ? "use" : "uses"}`;
  return historyByHarness(stats, "all")[harness] ? "No uses" : "No readable history";
}

/** One agent's line: chevron, icon, name, skill-list state, uses. The 16px chevron slot stays
 * on rows that do not open, so every row's columns line up. */
const ROW_GRID =
  "grid min-h-9 w-full grid-cols-[16px_16px_minmax(0,8rem)_minmax(0,1fr)_auto] items-center gap-x-2 rounded-sm px-2 text-left";

interface SkillVisibilityProps {
  skill: InstalledSkill;
  deployment: Deployment;
  /** The 30-day use history Home and Activity read. Without it the uses column stays out. */
  invocations?: SkillInvocationStats[];
  initialClient?: string;
  listingScope?: string;
}

/** Every agent that lists this copy, or used the skill: what its skill list shows of it, and
 * how often it used it. A row opens to the cut text or the reason when there is one. */
export function SkillVisibility({
  skill,
  deployment,
  invocations,
  initialClient,
  listingScope,
}: SkillVisibilityProps) {
  const project =
    listingScope === "global" ? null : (listingScope ?? deployment.project_path ?? null);
  const { listings, error } = useSkillVisibility(project, deployment.content_hash);
  const clients = useClientRows(listings);
  const [open, setOpen] = useState<string | null>(initialClient ?? null);
  // Any copy of the skill counts: an agent that reads its own copy still sees this skill.
  const paths = new Set(
    skill.deployments.flatMap((d) => [d.path, d.resolved_path ?? d.symlink_target ?? d.path]),
  );
  const listed: AgentRow[] = clients.flatMap((row) => {
    const entry = row.entries.find((e) => e.path && paths.has(e.path));
    return entry ? [{ harness: row.harness, row, entry }] : [];
  });
  const stats = invocations?.find((s) => s.skill === skill.name);
  const usedOnly: AgentRow[] = Object.entries(stats?.by_harness_30_days ?? {})
    .filter(([harness, uses]) => uses > 0 && !listed.some((a) => a.harness === harness))
    .map(([harness]) => ({ harness, row: null, entry: null }));
  const agents = [...listed, ...usedOnly];
  if (agents.length === 0)
    return error ? (
      <p role="alert" className="text-small text-text-secondary">
        {error}
      </p>
    ) : null;
  // Rows keep the order of the agents' own list; the one this copy belongs to leads.
  const own = agentIdFromDeploymentLabel(deployment.agent);
  agents.sort((a, b) => Number(b.harness === own) - Number(a.harness === own));

  return (
    <PageSection
      title="Agents"
      meta={<span className="text-text-tertiary">{project ? "Project" : "Global"} skill list</span>}
    >
      <div className={cn(ROW_GRID, "min-h-0 pb-1 text-caption text-text-tertiary")}>
        <span className="col-start-4">Skill list</span>
        {invocations && <span className="text-right">Last 30 days</span>}
      </div>
      <ul aria-label="Agents" className="m-0 flex list-none flex-col p-0">
        {agents.map((agent) => {
          const state = stateOf(agent);
          const expandable = hasDetail(agent);
          const isOpen = expandable && open === agent.harness;
          // SAFETY: listing rows and the use history key agents by their catalog id.
          const agentId = agent.harness as AgentId;
          const line = (
            <>
              <span className="flex size-4 items-center justify-center text-text-tertiary">
                {expandable && (
                  <ChevronRight
                    size={14}
                    aria-hidden="true"
                    className={cn(
                      "transition-transform duration-150 ease-out motion-reduce:transition-none",
                      isOpen && "rotate-90",
                    )}
                  />
                )}
              </span>
              <HarnessIcon harness={agentId} size={16} />
              <span className="truncate text-body text-text-primary">
                {deploymentLabelFromAgentId(agent.harness)}
              </span>
              <span className="flex min-w-0 items-baseline gap-2">
                <span
                  className={cn(
                    "shrink-0 text-body",
                    state.cut ? "text-warning" : "text-text-secondary",
                  )}
                >
                  {state.label}
                </span>
                {state.note && (
                  <span className="truncate text-caption text-text-tertiary tabular-nums">
                    {state.note}
                  </span>
                )}
              </span>
              {invocations && (
                <span className="text-right text-small text-text-secondary tabular-nums">
                  {usesLabel(agent.harness, invocations, skill.name)}
                </span>
              )}
            </>
          );
          return (
            <li key={agent.harness}>
              {expandable && agent.row && agent.entry ? (
                <Collapsible
                  open={isOpen}
                  onOpenChange={(next) => setOpen(next ? agent.harness : null)}
                >
                  <CollapsibleTrigger
                    className={cn(
                      ROW_GRID,
                      "cursor-pointer border-0 bg-transparent hover:bg-bg-hover",
                    )}
                  >
                    {line}
                  </CollapsibleTrigger>
                  <CollapsiblePanel>
                    <div className="pt-1 pr-2 pb-3 pl-[3.25rem]">
                      <AgentDetail row={agent.row} entry={agent.entry} />
                    </div>
                  </CollapsiblePanel>
                </Collapsible>
              ) : (
                <div className={ROW_GRID}>{line}</div>
              )}
            </li>
          );
        })}
      </ul>
      {error && <p className="mt-2 text-caption text-text-tertiary">{error}</p>}
    </PageSection>
  );
}
