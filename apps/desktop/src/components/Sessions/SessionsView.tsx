// ============================================================================
// SessionsView - The Sessions place: for each agent's recent coding sessions,
// which skills the model saw with a description, by name only, or not at all,
// which skills each session used, and which it possibly missed. Per skill,
// the findings say what to do. Data from the core's `skill_sessions`; reads
// only.
// ============================================================================

import { useEffect, useState } from "react";
import { Button } from "@skill-studio/ui";
import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type {
  AgentSessions,
  SessionSummary,
  SkillSessionFlag,
  SkillSessionRollup,
  SkillSessionsReport,
} from "@skill-studio/lib";
import { PageShell } from "../Shell/PageShell";
import { SelectControl } from "../ui/SelectControl";
import { useAppStore } from "../../store/appStore";

/** The answer for one session count; a different count means loading. */
type Load =
  | { kind: "ready"; count: string; report: SkillSessionsReport }
  | { kind: "error"; count: string; message: string };

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
      "The model found these from the name alone. Raise the listing budget on Model listing, or shorten other descriptions, so they keep a description.",
  },
  {
    flag: "listed_never_used",
    title: "Listed in nearly every session, never used",
    advice:
      "Each one takes listing space that other skills need. Park the skill, turn off its plugin, or in Claude Code set skillOverrides to user-invocable-only so you can still type it.",
  },
  {
    flag: "not_reachable",
    title: "Installed, but this agent cannot reach it",
    advice:
      "The skill is not in a folder this agent reads. Install it for this agent if you want the model to use it.",
  },
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
          : "This agent does not record its listing."}
      </p>
      <p className="m-0">{agent.note}</p>
      {agent.doc && (
        <p className="m-0">
          How this agent finds skills: <span className="break-all">{agent.doc}</span>
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
        {counts.length > 0 ? `Sessions: ${counts.join(", ")}.` : "Not in any session read."}
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

function SessionRow({ session }: { session: SessionSummary }) {
  const project = session.project_path?.split("/").filter(Boolean).pop() ?? "unknown folder";
  return (
    <li className="flex flex-col gap-0.5 py-2">
      <span className="min-w-0 truncate text-body text-text-primary" title={session.prompt ?? ""}>
        {session.prompt ?? "No typed prompt"}
      </span>
      <span className="text-caption text-text-tertiary">
        {formatTime(session.at)} in {project}
        {session.listing_recorded
          ? `. Listed ${session.listed}: ${session.described} described, ${session.name_only} name only.`
          : ". No listing recorded."}
      </span>
      {session.used.length > 0 && (
        <span className="text-caption text-text-secondary">
          Used: {session.used.map((u) => `${u.skill} (${LISTED_LABEL[u.listed_as]})`).join(", ")}
        </span>
      )}
      {session.missed.map((m) => (
        <span key={m.skill} className="text-caption text-text-secondary">
          Possibly missed (guess): {m.skill}, {LISTED_LABEL[m.listed_as]}. Matched{" "}
          {m.words.join(", ")}.
        </span>
      ))}
    </li>
  );
}

export function SessionsView() {
  const [harness, setHarness] = useState("claude-code");
  const [count, setCount] = useState("100");
  const [answer, setAnswer] = useState<Load | null>(null);
  const [onlyFindings, setOnlyFindings] = useState(true);
  const readSkillSessions = useAppStore((s) => s.readSkillSessions);

  useEffect(() => {
    let live = true;
    readSkillSessions({ project_path: null, sessions: Number(count) })
      .then((report) => live && setAnswer({ kind: "ready", count, report }))
      .catch(
        (err) =>
          live &&
          setAnswer({
            kind: "error",
            count,
            message: err instanceof Error ? err.message : String(err),
          }),
      );
    return () => {
      live = false;
    };
  }, [readSkillSessions, count]);

  const load = answer?.count === count ? answer : null;
  const report = load?.kind === "ready" ? load.report : null;
  const agent = report?.agents.find((a) => a.harness === harness) ?? null;
  const skills = report?.skills.filter((s) => s.harness === harness) ?? [];
  const sessions = report?.sessions.filter((s) => s.harness === harness) ?? [];
  const shownSessions = onlyFindings
    ? sessions.filter((s) => s.missed.length > 0 || s.used.some((u) => u.listed_as !== "described"))
    : sessions;
  const agents = (report?.agents.map((a) => a.harness) ?? [harness]).map((id) => ({
    value: id,
    label: deploymentLabelFromAgentId(id),
  }));

  return (
    <PageShell
      title="Sessions"
      subtitle="Which skills your agents saw in each session, which they used, and which they may have missed."
      actions={
        <>
          <SelectControl
            value={harness}
            onValueChange={setHarness}
            items={agents}
            ariaLabel="Agent"
          />
          <SelectControl
            value={count}
            onValueChange={setCount}
            items={SESSION_COUNTS}
            ariaLabel="Sessions to read"
          />
        </>
      }
    >
      <div className="flex flex-col gap-5">
        <Glossary />
        {load === null ? (
          <p className="m-0 text-small text-text-secondary">Reading session history…</p>
        ) : load.kind === "error" ? (
          <p role="alert" className="m-0 text-small text-destructive-text">
            {load.message}
          </p>
        ) : !agent ? (
          <p className="m-0 text-small text-text-secondary">No data for this agent.</p>
        ) : (
          <>
            <AgentSummary agent={agent} />
            {FINDINGS.map((finding) => (
              <FindingSection key={finding.flag} finding={finding} skills={skills} />
            ))}
            {agent.records === "listing_and_uses" && (
              <section aria-label="Sessions" className="flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <h2 className="m-0 text-body font-medium text-text-primary">Sessions</h2>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="rounded-sm px-2 text-text-secondary"
                    aria-pressed={!onlyFindings}
                    onClick={() => setOnlyFindings((v) => !v)}
                  >
                    {onlyFindings
                      ? `Show all ${sessions.length}`
                      : "Show only sessions with findings"}
                  </Button>
                </div>
                {shownSessions.length === 0 ? (
                  <p className="m-0 text-small text-text-secondary">
                    {onlyFindings
                      ? "No session used a skill without its description or possibly missed one."
                      : "No sessions in the last 30 days."}
                  </p>
                ) : (
                  <ul className="m-0 list-none divide-y divide-border-subtle p-0">
                    {shownSessions.map((session) => (
                      <SessionRow key={session.transcript} session={session} />
                    ))}
                  </ul>
                )}
              </section>
            )}
          </>
        )}
      </div>
    </PageShell>
  );
}
