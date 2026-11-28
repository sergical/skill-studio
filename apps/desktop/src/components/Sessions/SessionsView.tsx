// ============================================================================
// SessionsView - The Sessions place: for each agent's recent coding sessions,
// which skills the model saw with a description, by name only, or not at all,
// which skills each session used, and which it possibly missed. Per skill,
// the findings say what to do. Data from the core's `skill_sessions`; reads
// only.
// ============================================================================

import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { SESSION_LISTED_LABEL, deploymentLabelFromAgentId } from "@skill-studio/lib";
import type {
  AgentSessions,
  SessionSummary,
  SkillSessionFlag,
  SkillSessionRollup,
  SkillSnapshot,
} from "@skill-studio/lib";
import { useClientRows } from "../../hooks/useClientRows";
import { AgentTabs } from "../Listing/AgentTabs";
import { AgentUses } from "./AgentUses";
import { PageSection } from "../Shell/PageSection";
import { PageShell } from "../Shell/PageShell";
import { SelectControl } from "../ui/SelectControl";
import { SummaryCard, SummaryMetric } from "../ui/SummaryCard";
import { useSessionsReport } from "../../hooks/useStoreReports";
import { useAppStore } from "../../store/appStore";

interface Finding {
  flag: SkillSessionFlag;
  label: string;
  advice: string;
}

const NOT_REACHABLE: Finding = {
  flag: "not_reachable",
  label: "Outside its folders",
  advice: "installed where this agent does not look. Install it in a folder it reads.",
};

/** Findings in the order a person acts on them. */
const FINDINGS: Finding[] = [
  {
    flag: "possibly_missed",
    label: "Possibly missed",
    advice:
      "a guess: a short first prompt matched the description and no skill ran. If it was right, make the description name the task.",
  },
  {
    flag: "used_while_name_only",
    label: "Used by name only",
    advice:
      "the model picked it from the name alone. Raise the budget under What it sees so it keeps a description.",
  },
  {
    flag: "listed_never_used",
    label: "Unused in the sessions with a skill list",
    advice:
      "in the skill list of nearly every one of these sessions, and no session read used it. Park it if you no longer need it.",
  },
  NOT_REACHABLE,
  {
    flag: "not_described_now",
    label: "No description now",
    advice: "the current settings shorten it to a name, leave it out or hide it.",
  },
];

/** `FINDINGS` for an agent whose sessions read include `withListing` that recorded a skill list:
 * Unused names those, the only sessions the core checks it against. */
function findingsFor(withListing: number): Finding[] {
  const these =
    withListing === 1
      ? "the one session with a skill list"
      : `these ${withListing.toLocaleString()} sessions with a skill list`;
  return FINDINGS.map((finding) =>
    finding.flag === "listed_never_used" ? { ...finding, label: `Unused in ${these}` } : finding,
  );
}

/** `skills` without the Unused finding on a skill the 30-day history shows as used: those
 * sessions are only the newest few, so parking it would take away a skill still in use. */
function withoutUsedSkillsUnused(
  skills: SkillSessionRollup[],
  snapshot: SkillSnapshot | undefined,
): SkillSessionRollup[] {
  const used = new Set(
    snapshot?.invocations.filter((stats) => stats.last_30_days > 0).map((stats) => stats.skill),
  );
  return skills.map((skill) =>
    used.has(skill.name)
      ? { ...skill, flags: skill.flags.filter((flag) => flag !== "listed_never_used") }
      : skill,
  );
}

const SESSION_COUNTS = [
  { value: "50", label: "Newest 50 sessions" },
  { value: "100", label: "Newest 100 sessions" },
  { value: "200", label: "Newest 200 sessions" },
];

function Glossary({ findings }: { findings: Finding[] }) {
  return (
    <section
      aria-label="What the words mean"
      className="flex flex-col gap-1 text-small text-text-secondary"
    >
      <p className="m-0">
        <span className="text-text-primary">Full description</span>: the model sees the name and
        description, so it can pick the skill from what you ask.
      </p>
      <p className="m-0">
        <span className="text-text-primary">Name only</span>: the model sees only the name. The
        skill can still run.
      </p>
      <p className="m-0">
        <span className="text-text-primary">Not listed</span>: the model does not see the skill.
      </p>
      {findings.map((finding) => (
        <p key={finding.flag} className="m-0">
          <span className="text-text-primary">{finding.label}</span>: {finding.advice}
        </p>
      ))}
    </section>
  );
}

function AgentNote({ agent }: { agent: AgentSessions }) {
  return (
    <p className="m-0 text-small text-text-secondary">
      {agent.note}
      {agent.doc && (
        <>
          {" "}
          <a
            href={agent.doc}
            target="_blank"
            rel="noopener noreferrer"
            className="text-accent underline-offset-2 hover:underline"
          >
            How {deploymentLabelFromAgentId(agent.harness)} finds skills
          </a>
        </>
      )}
    </p>
  );
}

function SkillRow({ skill, findings }: { skill: SkillSessionRollup; findings: Finding[] }) {
  const counts = [
    skill.described > 0 && `full description in ${skill.described}`,
    skill.truncated > 0 && `shortened in ${skill.truncated}`,
    skill.name_only > 0 && `name only in ${skill.name_only}`,
    skill.used > 0 && `used in ${skill.used}`,
    skill.missed > 0 && `possibly missed in ${skill.missed}`,
  ].filter(Boolean);
  return (
    <li
      aria-label={skill.name}
      className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-3 py-2"
    >
      <span
        className="min-w-0 truncate text-body text-text-primary"
        title={skill.path ?? undefined}
      >
        {skill.name}
      </span>
      <span className="text-right text-small text-text-secondary">
        {findings
          .filter((finding) => skill.flags.includes(finding.flag))
          .map((finding) => finding.label)
          .join(" · ")}
      </span>
      {(counts.length > 0 || skill.reach) && (
        <span className="col-span-2 text-caption text-text-tertiary tabular-nums">
          {counts.length > 0 && `Sessions: ${counts.join(" · ")}`}
          {counts.length > 0 && skill.reach && ". "}
          {skill.reach}
        </span>
      )}
    </li>
  );
}

/** One row per skill with a finding, in the order a person acts on the findings. */
function Findings({ findings, skills }: { findings: Finding[]; skills: SkillSessionRollup[] }) {
  const [open, setOpen] = useState(false);
  const rank = (skill: SkillSessionRollup) =>
    findings.findIndex((finding) => skill.flags.includes(finding.flag));
  const rows = skills.filter((skill) => rank(skill) >= 0).sort((a, b) => rank(a) - rank(b));
  if (rows.length === 0) return null;
  // At most 200 rows in the page; the CLI prints every one.
  const shown = rows.slice(0, open ? 200 : 8);
  return (
    <PageSection title={`Findings · ${rows.length}`}>
      <ul
        aria-label="Skills with findings"
        className="m-0 list-none divide-y divide-border-subtle border-y border-border-subtle p-0"
      >
        {shown.map((skill) => (
          <SkillRow key={skill.name} skill={skill} findings={findings} />
        ))}
      </ul>
      {open && rows.length > shown.length && (
        <p className="mt-2 text-caption text-text-tertiary">
          {rows.length - shown.length} more. Run skill-studio sessions to see every one.
        </p>
      )}
      {!open && rows.length > shown.length && (
        <Button size="sm" variant="ghost" className="mt-1 -ml-2.5" onClick={() => setOpen(true)}>
          Show all {rows.length}
        </Button>
      )}
    </PageSection>
  );
}

function formatTime(at: string | null): string {
  if (!at) return "Time unknown";
  const date = new Date(at);
  return Number.isNaN(date.getTime())
    ? at
    : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

function RecordedListing({
  session,
  descriptions,
}: {
  session: SessionSummary;
  descriptions: string[];
}) {
  const [open, setOpen] = useState(false);
  const [limit, setLimit] = useState(10);
  return (
    <details onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary className="cursor-pointer text-small">
        Skill list in this session · {session.entries.length.toLocaleString()}{" "}
        {session.entries.length === 1 ? "skill" : "skills"}
      </summary>
      {open && (
        <>
          <ul
            className="mt-1 list-none divide-y divide-border-subtle pl-0"
            aria-label="Recorded skills"
          >
            {session.entries.slice(0, limit).map((entry, position) => {
              const text =
                entry.description_index == null ? undefined : descriptions[entry.description_index];
              return (
                <li key={`${position}-${entry.skill}`} className="py-2">
                  <strong className="font-medium">{entry.skill}</strong>
                  {entry.locator && (
                    <p className="break-all text-caption text-text-tertiary">{entry.locator}</p>
                  )}
                  {(!text || entry.listed_as !== "described") && (
                    <span className="ml-2 text-caption text-text-tertiary">
                      {SESSION_LISTED_LABEL[entry.listed_as]}
                    </span>
                  )}
                  {text && (
                    <p className="mt-0.5 whitespace-pre-wrap select-text text-text-secondary">
                      {text}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
          {session.entries.length > limit && (
            <Button variant="ghost" size="sm" onClick={() => setLimit(limit + 25)}>
              Show more ({limit} of {session.entries.length})
            </Button>
          )}
        </>
      )}
    </details>
  );
}

function SessionRow({
  session,
  descriptions,
  guidance,
}: {
  session: SessionSummary;
  descriptions: string[];
  guidance: boolean;
}) {
  const folder = session.project_path?.split("/").filter(Boolean) ?? [];
  const parent = folder[folder.length - 2];
  const leaf = folder[folder.length - 1];
  const projectName =
    folder[folder.length - 4] === ".t3" && folder[folder.length - 3] === "worktrees"
      ? parent
      : (leaf ?? "Unknown folder");
  return (
    <li>
      <details className="group border-b border-border-subtle">
        <summary
          data-own-marker
          className="grid cursor-pointer grid-cols-[12px_minmax(0,1fr)_auto] items-center gap-3 rounded-sm px-1 py-2.5 hover:bg-bg-hover"
        >
          <ChevronRight
            aria-hidden="true"
            size={13}
            className="text-text-tertiary group-open:rotate-90"
          />
          <span className="min-w-0">
            <strong
              className="block truncate text-body font-medium"
              title={session.project_path ?? undefined}
            >
              {projectName}
              {session.agent_name && ` · ${session.agent_name}`}
            </strong>
            <span className="block truncate text-caption text-text-secondary">
              {guidance
                ? (session.id ?? "Saved session")
                : session.used.length
                  ? session.used.map((u) => u.skill).join(" · ")
                  : "No skills used"}
            </span>
          </span>
          <span className="text-right">
            <span className="block text-small tabular-nums">
              {guidance
                ? session.listing_recorded
                  ? `${session.listed.toLocaleString()} ${session.listed === 1 ? "skill" : "skills"} saved`
                  : "Guidance unavailable"
                : session.used.length > 0 &&
                  `${session.used.length} ${session.used.length === 1 ? "skill" : "skills"} used`}
            </span>
            <span className="text-caption text-text-tertiary">{formatTime(session.at)}</span>
          </span>
        </summary>
        <div className="mb-3 ml-1 flex flex-col gap-2 border-l-2 border-border-subtle pl-3 text-small">
          <p>
            {session.listing_recorded
              ? `Listed ${session.listed} skills · ${session.described} with a full description · ${session.name_only} name only`
              : "This session did not record its skill list."}
          </p>
          {session.used.map((u) => (
            <p key={u.skill}>
              {u.skill}{" "}
              <span className="text-text-tertiary">· {SESSION_LISTED_LABEL[u.listed_as]}</span>
            </p>
          ))}
          {session.listing_recorded && session.entries.length > 0 && (
            <RecordedListing session={session} descriptions={descriptions} />
          )}
          {session.missed.map((m) => (
            <p key={m.skill}>
              Possible match: {m.skill} · matched {m.words.join(", ")}.
            </p>
          ))}
          {session.prompt && (
            <details className="text-text-tertiary">
              <summary className="cursor-pointer text-caption">First message</summary>
              <p className="mt-1 select-text text-text-secondary">{session.prompt}</p>
            </details>
          )}
        </div>
      </details>
    </li>
  );
}

interface SessionsViewProps {
  initialHarness?: string;
  /** The use history behind Uses in 30 days; App passes the same snapshot as What it sees. */
  snapshot?: SkillSnapshot;
  /** The 30-day use history is being counted, as on a first launch. */
  usageRefreshing?: boolean;
}

export function SessionsView({
  initialHarness,
  snapshot,
  usageRefreshing = false,
}: SessionsViewProps) {
  const [selected, setSelected] = useState(initialHarness);
  const navigate = useAppStore((s) => s.setActiveView);
  const [count, setCount] = useState("100");
  const [onlyFindings, setOnlyFindings] = useState(false);
  const load = useSessionsReport(null, Number(count));

  const report = load?.data;
  const clients = useClientRows(report?.agents ?? []);
  const agent = clients.find((a) => a.harness === selected) ?? clients[0];
  const harness = agent?.harness ?? "";
  const skills = withoutUsedSkillsUnused(
    report?.skills.filter((s) => s.harness === harness) ?? [],
    snapshot,
  );
  const findings = findingsFor(agent?.sessions_with_listing ?? 0);
  const sessions = (report?.sessions.filter((s) => s.harness === harness) ?? []).sort((a, b) =>
    (b.at ?? "").localeCompare(a.at ?? ""),
  );
  const guidance = agent?.records === "saved_guidance";
  const name = deploymentLabelFromAgentId(harness);
  const shownSessions =
    onlyFindings && !guidance
      ? sessions.filter(
          (s) => s.missed.length > 0 || s.used.some((u) => u.listed_as !== "described"),
        )
      : sessions;
  const agents = clients.map(({ harness: id }) => ({
    value: id,
    label: deploymentLabelFromAgentId(id),
  }));

  return (
    <PageShell
      title="Agents"
      refreshing={load?.refreshing ?? true}
      toolbar={<AgentTabs tab="sessions" harness={harness} />}
      actions={
        <>
          <SelectControl
            value={harness}
            onValueChange={setSelected}
            items={agents}
            ariaLabel="Agent"
            disabled={!agents.length}
          />
          {agent && agent.records !== "uses_only" && (
            <SelectControl
              value={count}
              onValueChange={setCount}
              items={SESSION_COUNTS}
              ariaLabel="Sessions to read"
            />
          )}
        </>
      }
    >
      <div className="flex flex-col gap-8">
        {load?.error && (
          <p role="alert" className="m-0 text-small text-destructive-text">
            {load.error}
          </p>
        )}
        {!agent ? (
          report && (
            <p className="m-0 text-small text-text-secondary">
              None of your agents has session records.
            </p>
          )
        ) : (
          <>
            {agent.records === "listing_and_uses" && (
              <SummaryCard title={`What ${name} used`}>
                <SummaryMetric
                  value={`${agent.sessions_with_use.toLocaleString()} of ${agent.sessions_read.toLocaleString()}`}
                  unit={`recent ${agent.sessions_read === 1 ? "session" : "sessions"} used a skill`}
                />
                <p className="mt-2 text-small text-text-secondary tabular-nums">
                  {agent.sessions_with_miss.toLocaleString()} possibly missed one ·{" "}
                  {agent.sessions_with_listing.toLocaleString()} recorded the skill list
                </p>
              </SummaryCard>
            )}
            {guidance && (
              <SummaryCard title={`What ${name} saved`}>
                <SummaryMetric
                  value={`${agent.sessions_with_listing.toLocaleString()} of ${agent.sessions_read.toLocaleString()}`}
                  unit={`recent ${agent.sessions_read === 1 ? "session" : "sessions"} saved readable skill guidance`}
                />
              </SummaryCard>
            )}
            {agent.records === "uses_only" && (
              <div className="flex flex-col items-start gap-3">
                <AgentNote agent={agent} />
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => navigate({ kind: "listing", harness })}
                >
                  See what {name} sees
                </Button>
              </div>
            )}
            {snapshot && (
              <AgentUses
                snapshot={snapshot}
                harness={harness}
                sessionsRead={agent.sessions_read}
                sessionsWithUse={agent.sessions_with_use}
                historyCounting={usageRefreshing}
              />
            )}
            {agent.records === "listing_and_uses" && (
              <Findings findings={findings} skills={skills} />
            )}
            {agent.records === "uses_only" && (
              <Findings findings={[NOT_REACHABLE]} skills={skills} />
            )}
            {agent.records !== "uses_only" && (
              <PageSection
                title="Recent sessions"
                meta={
                  !guidance && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="-mr-2 rounded-sm px-2 text-text-secondary"
                      aria-pressed={onlyFindings}
                      onClick={() => setOnlyFindings((v) => !v)}
                    >
                      {onlyFindings
                        ? `Show all ${sessions.length}`
                        : "Show only sessions with findings"}
                    </Button>
                  )
                }
              >
                {shownSessions.length === 0 ? (
                  <p className="m-0 text-small text-text-secondary">
                    {onlyFindings && !guidance
                      ? "No session used a skill without its description or possibly missed one."
                      : guidance
                        ? "No readable session files in the last 30 days."
                        : "No sessions in the last 30 days."}
                  </p>
                ) : (
                  <ul className="m-0 list-none divide-y divide-border-subtle p-0">
                    {shownSessions.map((session) => (
                      <SessionRow
                        key={JSON.stringify([session.transcript, session.id])}
                        guidance={guidance}
                        session={session}
                        descriptions={agent.captured_descriptions ?? []}
                      />
                    ))}
                  </ul>
                )}
              </PageSection>
            )}
            {agent.records === "listing_and_uses" && (
              <details className="rounded-md border border-border-subtle px-3 py-2">
                <summary className="cursor-pointer text-small">What the words mean</summary>
                <div className="mt-3 mb-1 flex flex-col gap-3">
                  <Glossary findings={findings} />
                  <AgentNote agent={agent} />
                </div>
              </details>
            )}
          </>
        )}
      </div>
    </PageShell>
  );
}
