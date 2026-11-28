// PROTOTYPE (throwaway): "Ask in one line". Each set-up agent with what it sees and what it
// cut, the week as a number and day bars, what needs the user as counts that open Skills, and
// one prompt, written for exactly those items, that the user copies into an agent with the
// Skill Studio MCP server.

import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Copy, RefreshCw } from "lucide-react";
import { HARNESS_CATALOG, formatCount } from "@skill-studio/lib";
import type { HarnessListing, SkillListFilter } from "@skill-studio/lib";
import { Button, cn } from "@skill-studio/ui";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import type { UpdateCheck } from "../../components/Home/useUpdateCheck";
import { useMcpConnections } from "../../hooks/useMcpConnections";
import { invokeErrorMessage } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";
import { agentCell, cutLine, names, plural } from "./ask-line-model";
import type { AgentCell } from "./ask-line-model";
import { dailyUses } from "./library-model";
import type { SimpleProps } from "./SimpleVariants";

const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);
const orList = (items: string[]) =>
  items.length > 1
    ? `${items.slice(0, -1).join(", ")} or ${items[items.length - 1]}`
    : items.join("");
const lowerFirst = (text: string) => text.charAt(0).toLowerCase() + text.slice(1);

interface Task {
  id: "broken" | "warnings" | "updates" | "unused" | `cut:${string}`;
  /** The prompt line, an instruction that starts with a verb. */
  line: string;
}

/** One prompt for the given tasks: a single task reads as one sentence, more as a list. */
function promptFor(tasks: Task[]): string {
  if (tasks.length === 1)
    return `Use the Skill Studio MCP tools to ${lowerFirst(tasks[0].line)} Tell me what you changed.`;
  return [
    "Use the Skill Studio MCP tools to sort out my skills:",
    ...tasks.map((task) => `- ${task.line}`),
    "Tell me what you changed.",
  ].join("\n");
}

/** Copies text and says so in place for 1.5s; a refused clipboard says so in a toast. */
function useCopy() {
  const addToast = useAppStore((s) => s.addToast);
  const [copied, setCopied] = useState<string | null>(null);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = (key: string, text: string) => {
    Promise.resolve()
      .then(() => navigator.clipboard.writeText(text))
      .then(() => {
        setCopied(key);
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setCopied(null), 1500);
      })
      .catch((cause: unknown) =>
        addToast({
          type: "error",
          title: "Couldn’t copy the prompt",
          message: invokeErrorMessage(cause),
        }),
      );
  };
  return { copied, copy };
}

function WeekBars({ values, labels }: { values: number[]; labels: string[] }) {
  const max = Math.max(1, ...values);
  return (
    <span className="flex flex-col gap-1" aria-hidden>
      <span className="flex h-8 items-end gap-1">
        {values.map((v, i) => (
          <span
            key={i}
            className={cn("w-3 rounded-[2px]", i === values.length - 1 ? "bg-data" : "bg-data/40")}
            style={{ height: `${v ? Math.max(12, (v / max) * 100) : 6}%` }}
          />
        ))}
      </span>
      <span className="flex gap-1 text-[10px] leading-none text-text-tertiary">
        {labels.map((label, i) => (
          <span
            key={i}
            className={cn("w-3 text-center", i === labels.length - 1 && "text-text-secondary")}
          >
            {label}
          </span>
        ))}
      </span>
    </span>
  );
}

export function AskLineVariant(
  props: SimpleProps & { listings: HarnessListing[]; updateCheck: UpdateCheck },
) {
  const { snapshot, counts, updateCheck } = props;
  const navigate = useAppStore((s) => s.setActiveView);
  const replaceFilter = useAppStore((s) => s.replaceSkillListFilter);
  const goToSkills = (patch: Partial<SkillListFilter>) => {
    replaceFilter(patch);
    navigate({ kind: "skills" });
  };
  const { connected } = useMcpConnections();
  const { copied, copy } = useCopy();
  const [open, setOpen] = useState(false);
  const previewId = useId();

  const days = dailyUses(snapshot, props.agents, 7);
  const total = sum(days.total);
  const hasHistory = snapshot.invocations.length > 0;
  const kept = new Set(props.agents ?? props.listings.map((r) => r.harness));
  const setUp = HARNESS_CATALOG.filter((row) => kept.has(row.id));
  const missing = HARNESS_CATALOG.filter((row) => !kept.has(row.id));
  const agents: AgentCell[] = setUp.map((row) =>
    agentCell(
      row,
      props.listings.find((r) => r.harness === row.id),
      snapshot.invocations,
      days.byAgent.get(row.id),
    ),
  );
  // One agent's uses would repeat the week's total, so the cells show uses only for two or more.
  const usesPerAgent = agents.length > 1 && hasHistory;

  const tasks: Task[] = [];
  const chips: { id: Task["id"]; count: number; word: string; tone: string; go: () => void }[] = [];
  if (counts.broken.length) {
    tasks.push({
      id: "broken",
      line: `Fix the ${plural(counts.broken.length, "broken skill")}: ${names(counts.broken)}.`,
    });
    chips.push({
      id: "broken",
      count: counts.broken.length,
      word: "broken",
      tone: "text-error",
      go: () => goToSkills({ attention: "broken" }),
    });
  }
  if (counts.warnings.length) {
    tasks.push({
      id: "warnings",
      line: `Check the ${plural(counts.warnings.length, "skill")} with warnings and fix what you can: ${names(counts.warnings)}.`,
    });
    chips.push({
      id: "warnings",
      count: counts.warnings.length,
      word: counts.warnings.length === 1 ? "warning" : "warnings",
      tone: "text-warning",
      go: () => goToSkills({ attention: "warning" }),
    });
  }
  if (counts.updates.length) {
    tasks.push({
      id: "updates",
      line: `Show me what changed in the ${plural(counts.updates.length, "skill")} with updates, then update the ones I approve: ${names(counts.updates)}.`,
    });
    chips.push({
      id: "updates",
      count: counts.updates.length,
      word: counts.updates.length === 1 ? "update" : "updates",
      tone: "text-text-primary",
      go: () => goToSkills({ attention: "update" }),
    });
  }
  for (const agent of agents) {
    const line = cutLine(agent);
    if (line) tasks.push({ id: `cut:${agent.id}`, line });
  }
  if (counts.unused.length) {
    tasks.push({
      id: "unused",
      line: `Park the ${plural(counts.unused.length, "skill")} I have not used in 30 days, after I confirm the list: ${names(counts.unused)}.`,
    });
    chips.push({
      id: "unused",
      count: counts.unused.length,
      word: "unused",
      tone: "text-text-primary",
      go: () => goToSkills({ usage: "unused-30d" }),
    });
  }
  const prompt = tasks.length ? promptFor(tasks) : "";
  // Only a read that came back can say no agent has the server; until then, say nothing.
  const noServer = connected !== null && !setUp.some((row) => connected.has(row.id));

  return (
    <section aria-label="Your skills at a glance" className="flex max-w-4xl flex-col gap-6">
      <ul aria-label="Agents" className="-mx-3 flex flex-wrap gap-1">
        {agents.map((agent) => {
          const icon = harnessIdFromLabel(agent.label);
          const cut = [
            agent.nameOnly?.length ? `${formatCount(agent.nameOnly.length)} name only` : "",
            agent.notListed?.length ? `${formatCount(agent.notListed.length)} not listed` : "",
          ].filter(Boolean);
          return (
            <li key={agent.id} className="flex w-36">
              <Button
                variant="ghost"
                className="h-auto min-w-0 flex-1 flex-col items-stretch justify-start gap-0.5 rounded-md px-3 py-2 text-left font-normal whitespace-normal"
                onClick={() => navigate({ kind: "listing", scope: "global", harness: agent.id })}
              >
                <span className="flex min-w-0 items-center gap-1.5 text-caption text-text-secondary">
                  {icon && <HarnessIcon harness={icon} size={12} />}
                  <span className="truncate">{agent.label}</span>
                  <ChevronRight
                    aria-hidden
                    className="-ml-0.5 size-3 shrink-0 opacity-40 group-hover/button:opacity-100 group-focus-visible/button:opacity-100"
                  />
                </span>
                <span className="text-caption text-text-tertiary tabular-nums">
                  <span className="text-body font-semibold text-text-primary">
                    {agent.sees === null ? "–" : formatCount(agent.sees)}
                  </span>{" "}
                  {agent.seesUnit}
                </span>
                {usesPerAgent && (
                  <span className="truncate text-caption text-text-tertiary tabular-nums">
                    {agent.uses === null
                      ? "No readable history"
                      : `${plural(agent.uses, "use")} this week`}
                  </span>
                )}
                {cut.length > 0 && (
                  <span className="truncate text-caption text-warning tabular-nums">
                    {cut.join(" · ")}
                  </span>
                )}
              </Button>
            </li>
          );
        })}
        {missing.length > 0 && (
          <li
            className="flex items-start gap-1.5 px-3 py-2 text-caption text-text-tertiary"
            title={`Not set up: ${missing.map((row) => row.label).join(", ")}`}
          >
            {missing.map((row) => {
              const icon = harnessIdFromLabel(row.label);
              return (
                icon && (
                  <span key={row.id} className="opacity-50" aria-hidden>
                    <HarnessIcon harness={icon} size={12} />
                  </span>
                )
              );
            })}
            <span className="whitespace-nowrap">{formatCount(missing.length)} not set up</span>
            <span className="sr-only">: {missing.map((row) => row.label).join(", ")}</span>
          </li>
        )}
      </ul>

      <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-5">
        {hasHistory ? (
          <Button
            variant="ghost"
            className="-mx-2 -my-1.5 h-auto items-end gap-4 px-2 py-1.5 font-normal"
            onClick={() => navigate({ kind: "activity" })}
          >
            <span className="flex flex-col items-start">
              <span className="text-caption text-text-tertiary">Uses this week</span>
              <span className="text-display leading-none font-semibold tabular-nums">
                {formatCount(total)}
              </span>
            </span>
            <WeekBars values={days.total} labels={days.labels} />
            <span className="sr-only">
              {days.labels.map((label, i) => `${label} ${days.total[i]}`).join(", ")}. Open Activity
            </span>
          </Button>
        ) : (
          <div className="max-w-xs">
            <p className="text-caption text-text-tertiary">Uses this week</p>
            <p className="mt-1 text-body">No readable history yet</p>
            <p className="mt-0.5 text-small text-text-secondary">
              Uses show here once a {orList(setUp.map((row) => row.label)) || "agent"} session runs
              a skill.
            </p>
          </div>
        )}

        <div className="flex flex-col items-start gap-2 sm:items-end">
          <div className="flex flex-wrap items-center gap-1.5">
            {tasks.length === 0 && (
              <span className="text-small text-text-secondary">Nothing needs you</span>
            )}
            {chips.map((chip) => (
              <Button key={chip.id} size="sm" variant="outline" onClick={chip.go}>
                <span className={cn("font-semibold tabular-nums", chip.tone)}>
                  {formatCount(chip.count)}
                </span>{" "}
                {chip.word}
              </Button>
            ))}
            {prompt && (
              <span className="ml-1 flex">
                <Button
                  size="sm"
                  className="rounded-r-none"
                  onClick={() => copy("all", prompt)}
                  aria-label={copied === "all" ? "Prompt copied" : "Copy prompt for your agent"}
                >
                  {copied === "all" ? <Check aria-hidden /> : <Copy aria-hidden />}
                  {copied === "all" ? "Copied" : "Copy prompt"}
                </Button>
                <Button
                  size="icon-sm"
                  className="rounded-l-none border-l border-l-black/20"
                  aria-label="Show the prompt"
                  aria-expanded={open}
                  aria-controls={previewId}
                  onClick={() => setOpen((v) => !v)}
                >
                  <ChevronDown
                    aria-hidden
                    className={cn(
                      "transition-transform duration-150 motion-reduce:transition-none",
                      open && "rotate-180",
                    )}
                  />
                </Button>
              </span>
            )}
            {prompt && noServer && (
              <Button
                size="sm"
                variant="outline"
                title="No set-up agent has the Skill Studio MCP server yet, and the prompt needs it."
                onClick={() => navigate({ kind: "settings" })}
              >
                Connect the MCP server
              </Button>
            )}
          </div>
          <span className="flex items-center gap-1 text-caption text-text-tertiary">
            <span className={updateCheck.failed ? "text-error" : undefined}>
              {updateCheck.caption}
            </span>
            <Button
              variant="ghost"
              size="xs"
              className="-mr-2 text-text-secondary"
              disabled={updateCheck.pending}
              onClick={updateCheck.check}
            >
              <RefreshCw
                aria-hidden
                className={updateCheck.pending ? "motion-safe:animate-spin" : undefined}
              />
              {updateCheck.pending ? "Checking…" : "Check for updates"}
            </Button>
          </span>
        </div>
      </div>

      <span role="status" className="sr-only">
        {copied ? "Prompt copied" : ""}
      </span>
      {updateCheck.checkError && (
        <span role="alert" className="sr-only">
          {updateCheck.checkError}
        </span>
      )}
      {prompt && open && (
        <div
          id={previewId}
          className="flex flex-col gap-1 rounded-md border border-border-subtle bg-bg-secondary py-2.5 pr-1.5 pl-3 font-mono text-small leading-relaxed select-text"
        >
          {tasks.length > 1 && <p>Use the Skill Studio MCP tools to sort out my skills:</p>}
          <ul aria-label="Prompt lines" className="flex flex-col gap-1">
            {tasks.map((task) => (
              <li key={task.id} className="flex items-start gap-2">
                <span
                  className={cn("min-w-0 flex-1", tasks.length > 1 && "pl-[2ch] -indent-[2ch]")}
                >
                  {tasks.length > 1 ? `- ${task.line}` : promptFor([task])}
                </span>
                {tasks.length > 1 && (
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    className="text-text-tertiary"
                    aria-label={
                      copied === task.id
                        ? "Copied"
                        : `Copy only this line as a prompt: ${task.line}`
                    }
                    title="Copy only this line as a prompt"
                    onClick={() => copy(task.id, promptFor([task]))}
                  >
                    {copied === task.id ? <Check aria-hidden /> : <Copy aria-hidden />}
                  </Button>
                )}
              </li>
            ))}
          </ul>
          {tasks.length > 1 && <p>Tell me what you changed.</p>}
        </div>
      )}
    </section>
  );
}
