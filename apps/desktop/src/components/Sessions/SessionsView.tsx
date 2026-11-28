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
import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type {
  AgentSessions,
  SessionSummary,
  SkillSessionFlag,
  SkillSessionRollup,
} from "@skill-studio/lib";
import { useClientRows } from "../../hooks/useClientRows";
import { PageShell } from "../Shell/PageShell";
import { SelectControl } from "../ui/SelectControl";
import { useSessionsReport } from "../../hooks/useStoreReports";
import { useAppStore } from "../../store/appStore";

type ListedAs = SessionSummary["used"][number]["listed_as"];

const LISTED_LABEL = {
  described: "described",
  truncated: "shortened",
  name_only: "name only",
  not_listed: "not listed",
  unknown: "listing not recorded",
} satisfies Record<ListedAs, string>;

interface Finding {
  flag: SkillSessionFlag;
  title: string;
  advice: string;
}

const NOT_REACHABLE: Finding = {
  flag: "not_reachable",
  title: "Installed outside this client’s skill folders",
  advice: "Install the skill in a folder this client reads to make it available here.",
};

/** Findings in the order a person acts on them. */
const FINDINGS: Finding[] = [
  {
    flag: "possibly_missed",
    title: "Possibly missed",
    advice:
      "A guess: a short first prompt matched the skill's description, and the session used no skill. If the guess is right, reword the description so it names the task, or make sure the skill keeps its description.",
  },
  {
    flag: "used_while_name_only",
    title: "Used while listed by name only",
    advice:
      "The model found these from the name alone. Raise the listing budget on Client configuration, or shorten other descriptions, so they keep a description.",
  },
  {
    flag: "listed_never_used",
    title: "Listed in nearly every session, never used",
    advice:
      "Check whether you still want these skills available. Open a skill to see its locations and invocation options.",
  },
  NOT_REACHABLE,
  {
    flag: "not_described_now",
    title: "Not described in the current listing",
    advice:
      "The current settings show these by name only, leave them out, or hide them. The reason is next to each skill.",
  },
];

const SESSION_COUNTS = [
  { value: "50", label: "Newest 50 sessions" },
  { value: "100", label: "Newest 100 sessions" },
  { value: "200", label: "Newest 200 sessions" },
];

function Glossary() {
  return (
    <section
      aria-label="What the words mean"
      className="flex flex-col gap-1 rounded-md border border-border-subtle p-3 text-small text-text-secondary"
    >
      <p className="m-0">
        <span className="text-text-primary">Described</span>: the model sees the skill's name and
        description, so it can pick the skill from what you ask.
      </p>
      <p className="m-0">
        <span className="text-text-primary">Name only</span>: the model sees only the name and must
        guess from it. The skill can still run.
      </p>
      <p className="m-0">
        <span className="text-text-primary">Not listed</span>: the model does not see the skill. It
        can still read the SKILL.md file if it knows the path.
      </p>
    </section>
  );
}

function AgentSummary({ agent }: { agent: AgentSessions }) {
  return (
    <div className="flex flex-col gap-1 text-small text-text-secondary">
      <p className="m-0 text-body text-text-primary">
        {agent.records === "listing_and_uses"
          ? `${agent.sessions_read} sessions read: ${agent.sessions_with_use} used a skill, ${agent.sessions_with_miss} possibly missed one.`
          : agent.note}
      </p>
      <p className="m-0">
        {agent.records === "listing_and_uses"
          ? agent.note
          : "Explore the skills found in this client’s folders below."}
      </p>
      {agent.doc && (
        <p className="m-0">
          <a
            href={agent.doc}
            target="_blank"
            rel="noopener noreferrer"
            className="text-accent underline-offset-2 hover:underline"
          >
            How {deploymentLabelFromAgentId(agent.harness)} finds skills
          </a>
        </p>
      )}
    </div>
  );
}

function SkillRow({ skill }: { skill: SkillSessionRollup }) {
  const counts = [
    skill.described > 0 && `described in ${skill.described}`,
    skill.truncated > 0 && `shortened in ${skill.truncated}`,
    skill.name_only > 0 && `name only in ${skill.name_only}`,
    skill.used > 0 && `used in ${skill.used}`,
    skill.missed > 0 && `possibly missed in ${skill.missed}`,
  ].filter(Boolean);
  return (
    <li aria-label={skill.name} className="flex flex-col gap-0.5 py-1.5">
      <span
        className="min-w-0 truncate text-body text-text-primary"
        title={skill.path ?? undefined}
      >
        {skill.name}
      </span>
      <span className="text-caption text-text-tertiary">
        {counts.length > 0 && `Sessions: ${counts.join(", ")}.`}
        {skill.reach && ` ${skill.reach}`}
      </span>
    </li>
  );
}

function FindingSection({ finding, skills }: { finding: Finding; skills: SkillSessionRollup[] }) {
  const [open, setOpen] = useState(false);
  const rows = skills.filter((s) => s.flags.includes(finding.flag));
  if (rows.length === 0) return null;
  // At most 200 rows in the page; the CLI prints every one.
  const shown = rows.slice(0, open ? 200 : 8);
  return (
    <section aria-label={finding.title} className="flex flex-col gap-1">
      <h2 className="m-0 text-body font-medium text-text-primary">
        {finding.title} <span className="text-text-tertiary">({rows.length})</span>
      </h2>
      <p className="m-0 text-small text-text-secondary">{finding.advice}</p>
      <ul className="m-0 list-none divide-y divide-border-subtle p-0">
        {shown.map((skill) => (
          <SkillRow key={skill.name} skill={skill} />
        ))}
      </ul>
      {open && rows.length > shown.length && (
        <p className="m-0 text-caption text-text-tertiary">
          {rows.length - shown.length} more. Run skill-studio sessions to see every one.
        </p>
      )}
      {!open && rows.length > shown.length && (
        <Button
          variant="ghost"
          size="sm"
          className="self-start rounded-sm px-2 text-text-secondary"
          onClick={() => setOpen(true)}
        >
          Show all {rows.length}
        </Button>
      )}
    </section>
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
        Recorded listing · {session.entries.length.toLocaleString()}{" "}
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
                      {LISTED_LABEL[entry.listed_as]}
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
        <summary className="grid cursor-pointer list-none grid-cols-[12px_minmax(0,1fr)_auto] [&::-webkit-details-marker]:hidden items-center gap-3 rounded-sm px-1 py-2.5 hover:bg-bg-hover">
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
                  : "No skill use recorded"}
            </span>
          </span>
          <span className="text-right">
            <span className="block text-small tabular-nums">
              {guidance
                ? session.listing_recorded
                  ? `${session.listed.toLocaleString()} ${session.listed === 1 ? "skill" : "skills"} saved`
                  : "Guidance unavailable"
                : `${session.used.length} ${session.used.length === 1 ? "skill" : "skills"} loaded`}
            </span>
            <span className="text-caption text-text-tertiary">{formatTime(session.at)}</span>
          </span>
        </summary>
        <div className="mb-3 ml-1 flex flex-col gap-2 border-l-2 border-accent pl-3 text-small">
          <p>
            {session.listing_recorded
              ? `${session.listed} skills listed · ${session.described} with a description · ${session.name_only} by name only`
              : "Listing unavailable for this session."}
          </p>
          {session.used.map((u) => (
            <p key={u.skill}>
              {u.skill} <span className="text-text-tertiary">· {LISTED_LABEL[u.listed_as]}</span>
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

export function SessionsView() {
  const [selected, setSelected] = useState<string>();
  const navigate = useAppStore((s) => s.setActiveView);
  const [count, setCount] = useState("100");
  const [onlyFindings, setOnlyFindings] = useState(false);
  const load = useSessionsReport(null, Number(count));

  const report = load?.data;
  const clients = useClientRows(report?.agents ?? []);
  const agent = clients.find((a) => a.harness === selected) ?? clients[0];
  const harness = agent?.harness ?? "";
  const skills = report?.skills.filter((s) => s.harness === harness) ?? [];
  const sessions = (report?.sessions.filter((s) => s.harness === harness) ?? []).sort((a, b) =>
    (b.at ?? "").localeCompare(a.at ?? ""),
  );
  const guidance = agent?.records === "saved_guidance";
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
      title="Sessions"
      refreshing={load?.refreshing ?? true}
      subtitle="What your client recorded about skill use."
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
      <div className="flex flex-col gap-5">
        {load?.error && (
          <p role="alert" className="m-0 text-small text-destructive-text">
            {load.error}
          </p>
        )}
        {!agent ? (
          report && (
            <p className="m-0 text-small text-text-secondary">
              No selected client has a session report.
            </p>
          )
        ) : (
          <>
            {agent.records === "listing_and_uses" && (
              <section className="rounded-lg border border-border-subtle bg-bg-secondary p-4">
                <h2 className="text-heading-lg font-semibold">Skills in recent work</h2>
                <p className="mt-3 flex items-baseline gap-2">
                  <strong className="text-display leading-none tabular-nums">
                    {agent.sessions_with_use}
                  </strong>
                  <span className="text-body text-text-secondary">
                    of {agent.sessions_read} recent {deploymentLabelFromAgentId(harness)} sessions
                    loaded a skill
                  </span>
                </p>
                <p className="mt-2 text-small text-text-tertiary">
                  {agent.sessions_with_listing} recorded which skills were listed.
                </p>
              </section>
            )}
            {guidance && (
              <section className="space-y-2">
                <h2 className="text-heading-lg font-semibold">Saved skill guidance</h2>
                <p>
                  {agent.sessions_with_listing.toLocaleString()} of{" "}
                  {agent.sessions_read.toLocaleString()} recent sessions have readable guidance.
                </p>
                <p className="text-small text-text-secondary">{agent.note}</p>
              </section>
            )}
            {agent.records !== "uses_only" && (
              <section aria-label="Sessions" className="flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <h2 className="mr-auto text-heading font-semibold">Recent sessions</h2>
                  {!guidance && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="rounded-sm px-2 text-text-secondary"
                      onClick={() => setOnlyFindings((v) => !v)}
                    >
                      {onlyFindings
                        ? `Show all ${sessions.length}`
                        : "Show only sessions with findings"}
                    </Button>
                  )}
                </div>
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
              </section>
            )}
            {agent.records !== "listing_and_uses" && (
              <section className="space-y-3">
                {!guidance && <AgentSummary agent={agent} />}
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => navigate({ kind: "listing", harness })}
                >
                  See skills for {deploymentLabelFromAgentId(harness)}
                </Button>
                <FindingSection finding={NOT_REACHABLE} skills={skills} />
              </section>
            )}
            {agent.records === "listing_and_uses" && (
              <details className="rounded-md border border-border-subtle px-3 py-2">
                <summary className="cursor-pointer text-small">
                  Findings and listing definitions
                </summary>
                <div className="mt-3 flex flex-col gap-4">
                  <AgentSummary agent={agent} />
                  <Glossary />
                  {FINDINGS.map((finding) => (
                    <FindingSection key={finding.flag} finding={finding} skills={skills} />
                  ))}
                </div>
              </details>
            )}
          </>
        )}
      </div>
    </PageShell>
  );
}
