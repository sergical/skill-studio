// ============================================================================
// LearnView - What Skill Studio's words mean: uses, the skill list and its
// budget, who can invoke a skill, Broken and Warnings, and parking. Facts
// come from the shared catalog, issue map and labels in @skill-studio/lib, so
// a new agent or issue kind shows up here without a copy edit.
// ============================================================================

import type { ReactNode } from "react";
import {
  ATTENTION_LABEL,
  HARNESS_CATALOG,
  HEALTH_ISSUE_KIND_LABEL,
  HEALTH_ISSUE_KIND_ORDER,
  HEALTH_ISSUE_SEVERITY,
  LISTING_STATE_LABEL,
  joinNames,
  TRIGGERS,
} from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import type { HealthIssueKind } from "@skill-studio/lib";
import { HarnessIcon } from "../ui/HarnessIcon";
import { PageSection } from "../Shell/PageSection";
import { PageShell } from "../Shell/PageShell";
import { useAppStore } from "../../store/appStore";
type CatalogAgentId = (typeof HARNESS_CATALOG)[number]["id"];

/** How a person calls a skill by name, and the setting that keeps the model from calling it.
 * Source: docs/agent-skill-conventions.md, "Invocation control and per-agent off settings". */
const INVOKE_BY_AGENT = {
  "claude-code": { call: "/name", youOnly: ["disable-model-invocation: true"] },
  codex: {
    call: "$name",
    youOnly: ["agents/openai.yaml", "policy.allow_implicit_invocation: false"],
  },
  opencode: { call: "/name", youOnly: ["disable-model-invocation: true"] },
  pi: { call: "/skill:name", youOnly: ["disable-model-invocation: true"] },
  cursor: { call: "/name", youOnly: ["disable-model-invocation: true"] },
  "grok-build": { call: "/name", youOnly: ["disable-model-invocation: true"] },
} as const satisfies Record<CatalogAgentId, { call: string; youOnly: readonly string[] }>;

const ISSUE_MEANING = {
  "broken-symlink": "A link points at a folder that no longer exists.",
  "spec-violation":
    "The name or frontmatter breaks the agentskills.io spec, so the agent skips the skill.",
  "parked-but-reinstalled": "You parked it, then an installer put it back, so it loads again.",
  "linked-root":
    "An agent’s whole skills folder links to the Universal folder, so one skill can’t be turned off for that agent alone.",
  duplicate: "Two copies have different content, so two agents behave differently.",
  "spec-warning": "The frontmatter works for some agents and not for others.",
  "lock-only": ".skill-lock.json lists the skill, but it has no folder on disk.",
} as const satisfies Record<HealthIssueKind, string>;

const EXAMPLE_SKILL = "pdf-tools";
const EXAMPLE_DESCRIPTION = "Extract text and tables from PDF files and fill forms";

const LADDER: { label: string; line: string | null }[] = [
  { label: LISTING_STATE_LABEL.described, line: `- ${EXAMPLE_SKILL}: ${EXAMPLE_DESCRIPTION}` },
  { label: LISTING_STATE_LABEL.truncated, line: `- ${EXAMPLE_SKILL}: Extract text…` },
  { label: LISTING_STATE_LABEL.name_only, line: `- ${EXAMPLE_SKILL}` },
  { label: LISTING_STATE_LABEL.omitted, line: null },
];

const BUDGETED_AGENTS = HARNESS_CATALOG.filter((row) =>
  row.verifiedVisibility.some((source) => source.exposure.startsWith("Budgeted")),
);
const USAGE_READERS = HARNESS_CATALOG.filter((row) => row.usageReader);

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function Code({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-[3px] bg-bg-tertiary px-1 py-px font-mono text-small text-text-primary">
      {children}
    </code>
  );
}

function AgentName({ id, label }: { id: CatalogAgentId; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-text-primary">
      <HarnessIcon harness={id} size={14} />
      {label}
    </span>
  );
}

function SectionLink({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button variant="link" className="h-auto p-0 text-small" onClick={onClick}>
      {label}
    </Button>
  );
}

/** Learn's sections. */
export function LearnView() {
  const setActiveView = useAppStore((state) => state.setActiveView);

  const brokenKinds = HEALTH_ISSUE_KIND_ORDER.filter(
    (kind) => HEALTH_ISSUE_SEVERITY[kind] === "error",
  );
  const warningKinds = HEALTH_ISSUE_KIND_ORDER.filter(
    (kind) => HEALTH_ISSUE_SEVERITY[kind] === "warning",
  );

  return (
    <PageShell title="Learn">
      <div className="flex max-w-[70ch] flex-col gap-8 text-body leading-[1.55] text-text-secondary select-text">
        <PageSection
          title="Uses"
          meta={
            <SectionLink
              label="Open Activity"
              onClick={() => setActiveView({ kind: "activity" })}
            />
          }
        >
          <div className="flex flex-col gap-3">
            <p className="m-0">
              A use is the model running a skill in a recorded session. Skill Studio reads the last
              30 days of history from{" "}
              <span className="inline-flex flex-wrap items-center gap-x-2.5 align-middle">
                {USAGE_READERS.map((row) => (
                  <AgentName key={row.id} id={row.id} label={row.label} />
                ))}
              </span>
              .
            </p>
            <dl className="m-0 grid grid-cols-[max-content_minmax(0,1fr)] gap-x-4 gap-y-1.5">
              {TRIGGERS.map((trigger) => (
                <div key={trigger.id} className="contents">
                  <dt className="font-medium text-text-primary">{trigger.label}</dt>
                  <dd className="m-0">{trigger.hint}</dd>
                </div>
              ))}
            </dl>
            <p className="m-0">
              A skill is “Not used in the last 30 days” when no history Skill Studio can read
              records a use. An agent whose history it can’t read adds no uses.
            </p>
          </div>
        </PageSection>

        <PageSection
          title="Skill list and budget"
          meta={
            <SectionLink
              label="Open What it sees"
              onClick={() => setActiveView({ kind: "listing" })}
            />
          }
        >
          <div className="flex flex-col gap-3">
            <p className="m-0">
              On every turn, an agent puts each skill’s name and description in its skill list. The
              rest of SKILL.md loads only when the skill runs.{" "}
              {joinNames(BUDGETED_AGENTS.map((row) => row.label))} cap the list with a budget; as it
              fills, descriptions step down:
            </p>
            <figure className="m-0 overflow-hidden rounded-md border border-border-subtle">
              <figcaption className="sr-only">One skill at each step of a skill list</figcaption>
              {LADDER.map((step) => (
                <div
                  key={step.label}
                  className="grid grid-cols-[120px_minmax(0,1fr)] items-baseline gap-3 border-b border-border-subtle px-3 py-2 last:border-b-0"
                >
                  <span className="text-small font-medium text-text-primary">{step.label}</span>
                  {step.line == null ? (
                    <span className="text-small text-text-tertiary">no line</span>
                  ) : (
                    <span className="font-mono text-small break-words text-text-primary">
                      {step.line}
                    </span>
                  )}
                </div>
              ))}
            </figure>
            <p className="m-0">
              {LISTING_STATE_LABEL.hidden} skills are out of the list and cost nothing: “You only”,
              turned off in the agent’s settings, or parked.
            </p>
          </div>
        </PageSection>

        <PageSection title="Who can invoke a skill">
          <div className="flex flex-col gap-3">
            <p className="m-0">
              By default a skill is “You or the model”: you call it by name, and the model calls it
              when its description fits the task. “You only” keeps the model from calling it.
            </p>
            <table className="w-full border-collapse text-small">
              <thead>
                <tr className="text-left text-text-tertiary">
                  <th scope="col" className="border-b border-border-subtle py-1.5 pr-4 font-medium">
                    Agent
                  </th>
                  <th scope="col" className="border-b border-border-subtle py-1.5 pr-4 font-medium">
                    Call it by name
                  </th>
                  <th scope="col" className="border-b border-border-subtle py-1.5 font-medium">
                    Make it “You only”
                  </th>
                </tr>
              </thead>
              <tbody>
                {HARNESS_CATALOG.map((row) => {
                  const invoke = INVOKE_BY_AGENT[row.id];
                  return (
                    <tr key={row.id} className="align-top">
                      <td className="border-b border-border-subtle py-1.5 pr-4">
                        <AgentName id={row.id} label={row.label} />
                      </td>
                      <td className="border-b border-border-subtle py-1.5 pr-4">
                        <Code>{invoke.call}</Code>
                      </td>
                      <td className="border-b border-border-subtle py-1.5">
                        {invoke.youOnly.length === 1 ? (
                          <Code>{invoke.youOnly[0]}</Code>
                        ) : (
                          <>
                            <Code>{invoke.youOnly[0]}</Code> → <Code>{invoke.youOnly[1]}</Code>
                          </>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            <p className="m-0">
              A SKILL.md setting applies to every agent that reads that folder; Codex’s applies to
              Codex only. “Model only” exists only in Claude Code:{" "}
              <Code>user-invocable: false</Code>.
            </p>
          </div>
        </PageSection>

        <PageSection title={`${ATTENTION_LABEL.broken} and ${ATTENTION_LABEL.warning}`}>
          <div className="flex flex-col gap-4">
            <IssueGroup
              title={ATTENTION_LABEL.broken}
              lead="An agent does not load the skill."
              dotClassName="bg-error"
              kinds={brokenKinds}
            />
            <IssueGroup
              title={ATTENTION_LABEL.warning}
              lead="Agents still load it, but something drifted."
              dotClassName="bg-warning"
              kinds={warningKinds}
            />
          </div>
        </PageSection>

        <PageSection title="Parking">
          <div className="flex flex-col gap-3">
            <p className="m-0">
              Park takes a global skill away from every agent at once and keeps it, so you can bring
              it back. Skill Studio moves its folder and removes Claude Code’s link to it:
            </p>
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-small text-text-primary">
              <span>~/.agents/skills/{EXAMPLE_SKILL}</span>
              <span aria-hidden="true" className="text-text-tertiary">
                →
              </span>
              <span className="sr-only">moves to</span>
              <span>~/.agents/skills-parked/{EXAMPLE_SKILL}</span>
            </div>
            <p className="m-0">
              Unpark puts both back. A parked skill leaves every skill list, so it costs no budget.
            </p>
          </div>
        </PageSection>
      </div>
    </PageShell>
  );
}

function IssueGroup({
  title,
  lead,
  dotClassName,
  kinds,
}: {
  title: string;
  lead: string;
  dotClassName: string;
  kinds: HealthIssueKind[];
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <span aria-hidden="true" className={`size-2 rounded-full ${dotClassName}`} />
        <h3 className="m-0 text-body font-semibold text-text-primary">{title}</h3>
        <span className="text-text-secondary">· {lead}</span>
      </div>
      <dl className="m-0 grid grid-cols-[minmax(0,14rem)_minmax(0,1fr)] gap-x-4 gap-y-1.5 pl-4">
        {kinds.map((kind) => (
          <div key={kind} className="contents">
            <dt className="font-medium text-text-primary">
              {capitalize(HEALTH_ISSUE_KIND_LABEL[kind].singular)}
            </dt>
            <dd className="m-0">{ISSUE_MEANING[kind]}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
