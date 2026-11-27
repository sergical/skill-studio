// PROTOTYPE (throwaway): four simple Home directions, each answering one question first.
// "Since last time" lists what changed since the last visit; "One line" is a status sentence
// and a small week; "Ask" hands the clean-up to your agent as ready prompts; "Focus" shows the
// one most important thing at a time.

import { useState } from "react";
import { ArrowRight, Copy } from "lucide-react";
import {
  HARNESS_CATALOG,
  deploymentLabelFromAgentId,
  formatCount,
  formatRelativeTime,
} from "@skill-studio/lib";
import type { HarnessListing, SkillListFilter, SkillSnapshot } from "@skill-studio/lib";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { Button } from "@skill-studio/ui";
import { useAppStore } from "../../store/appStore";
import { dailyUses } from "./library-model";
import type { LibrarySkill } from "./library-model";

export interface SimpleProps {
  snapshot: SkillSnapshot;
  skills: LibrarySkill[];
  agents: string[] | undefined;
  counts: { broken: string[]; warnings: string[]; updates: string[]; unused: string[] };
  open: (name: string) => void;
}

const plural = (n: number, one: string, many = `${one}s`) =>
  `${formatCount(n)} ${n === 1 ? one : many}`;
const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

function useGoToSkills() {
  const replace = useAppStore((s) => s.replaceSkillListFilter);
  const setActiveView = useAppStore((s) => s.setActiveView);
  return (patch: Partial<SkillListFilter>) => {
    replace(patch);
    setActiveView({ kind: "skills" });
  };
}

interface Item {
  id: string;
  tone: string;
  title: string;
  why: string;
  action: string;
  go: () => void;
  ask: string;
}

/** Everything worth acting on, most harmful first; every view below draws from this one list. */
function useItems({ skills, counts }: SimpleProps): Item[] {
  const go = useGoToSkills();
  const items: Item[] = [];
  if (counts.broken.length)
    items.push({
      id: "broken",
      tone: "bg-error",
      title: `${plural(counts.broken.length, "skill")} ${counts.broken.length === 1 ? "is" : "are"} broken`,
      why: counts.broken.slice(0, 3).join(", "),
      action: "Fix",
      go: () => go({ attention: "broken" }),
      ask: `Use Skill Studio to fix my broken skills: ${counts.broken.join(", ")}.`,
    });
  if (counts.updates.length)
    items.push({
      id: "updates",
      tone: "bg-text-primary",
      title: `${plural(counts.updates.length, "update")} ready`,
      why: counts.updates.slice(0, 3).join(", "),
      action: "Review",
      go: () => go({ attention: "update" }),
      ask: "Use Skill Studio to show what changed in my skill updates, then update them.",
    });
  const cut = skills.filter((s) => s.cut).length;
  if (cut)
    items.push({
      id: "cut",
      tone: "bg-warning",
      title: `${plural(cut, "skill")} only listed by name`,
      why: "Your agent rarely picks a skill it knows only by name.",
      action: "See which",
      go: () => go({ attention: "warning" }),
      ask: "Use Skill Studio to show which skills my agents list by name only, and what to park to make room.",
    });
  if (counts.unused.length)
    items.push({
      id: "unused",
      tone: "bg-text-quaternary",
      title: `${plural(counts.unused.length, "skill")} unused in 30 days`,
      why: "Each one is sent with every message. Parking keeps it a click away.",
      action: "Review",
      go: () => go({ usage: "unused-30d" }),
      ask: "Use Skill Studio to park the skills I have not used in 30 days, and tell me what it parked.",
    });
  return items;
}

function week(snapshot: SkillSnapshot, agents: string[] | undefined) {
  const days = dailyUses(snapshot, agents, 7);
  const top = [...days.bySkill.entries()]
    .map(([name, v]) => ({ name, count: sum(v) }))
    .sort((a, b) => b.count - a.count);
  return { days, total: sum(days.total), used: top.filter((t) => t.count).length, top: top[0] };
}

function MiniWeek({ values }: { values: number[] }) {
  const max = Math.max(1, ...values);
  return (
    <span className="inline-flex h-8 items-end gap-1" aria-hidden>
      {values.map((v, i) => (
        <span
          key={i}
          className={`w-2.5 rounded-[2px] ${i === values.length - 1 ? "bg-data" : "bg-data/40"}`}
          style={{ height: `${v ? Math.max(12, (v / max) * 100) : 6}%` }}
        />
      ))}
    </span>
  );
}

function Row({ item, children }: { item: Item; children?: React.ReactNode }) {
  return (
    <li className="grid grid-cols-[0.5rem_minmax(0,1fr)_auto] items-center gap-3 border-b border-border-subtle py-3">
      <span className={`size-2 rounded-full ${item.tone}`} aria-hidden />
      <span className="min-w-0">
        <span className="block text-body font-medium">{item.title}</span>
        <span className="block truncate text-small text-text-secondary">{item.why}</span>
      </span>
      {children ?? (
        <Button size="sm" variant="outline" onClick={item.go}>
          {item.action}
        </Button>
      )}
    </li>
  );
}

// Read once per app launch, before this launch records itself, so a re-render or a second
// mount never turns "since last time" into "since just now".
const previousVisit = Number(localStorage.getItem("proto-last-visit")) || null;
localStorage.setItem("proto-last-visit", String(Date.now()));

function useLastVisit(fallback: number): number {
  return previousVisit ?? fallback;
}

/** 1. What changed since you last opened Home, and at most three things to act on. */
export function SinceVariant(props: SimpleProps) {
  const items = useItems(props);
  // The last visit is read once per mount and replaced by this one.
  const since = useLastVisit(Date.parse(props.snapshot.scanned_at) - 3 * 86_400_000);
  const w = week(props.snapshot, props.agents);
  const added = props.skills.filter((s) => {
    const at = Date.parse(s.skill.installed_at);
    return at > since && at <= Date.parse(props.snapshot.scanned_at);
  });
  const facts = [
    w.total ? `${plural(w.total, "use")} this week${w.top ? `, mostly ${w.top.name}` : ""}` : "",
    added.length ? `${plural(added.length, "new skill")}` : "",
    props.counts.broken.length ? `${formatCount(props.counts.broken.length)} broke` : "",
  ].filter(Boolean);
  return (
    <section aria-label="Since last time" className="flex max-w-2xl flex-col gap-4">
      <p className="text-small text-text-secondary">
        Since {formatRelativeTime(new Date(since).toISOString())}
      </p>
      <p className="text-[1.375rem] leading-snug font-semibold text-balance">
        {facts.length
          ? `${facts.join(" · ")}.`
          : "Nothing changed. Your skills are as you left them."}
      </p>
      {items.length > 0 && (
        <ul className="border-t border-border-subtle">
          {items.slice(0, 3).map((item) => (
            <Row key={item.id} item={item} />
          ))}
        </ul>
      )}
    </section>
  );
}

/** 2. One status sentence and the week as a small chart; everything else lives on other pages. */
export function OneLineVariant(props: SimpleProps) {
  const items = useItems(props);
  const w = week(props.snapshot, props.agents);
  const go = useGoToSkills();
  const hasHistory = props.snapshot.invocations.length > 0;
  return (
    <section
      aria-label="Status"
      className="flex items-center justify-between gap-6 rounded-lg border border-border-subtle px-5 py-4"
    >
      <div className="min-w-0">
        <p className="text-body">
          {hasHistory ? (
            <>
              <strong className="font-semibold">{plural(w.total, "use")}</strong> this week across{" "}
              {plural(w.used, "skill")}.
            </>
          ) : (
            "No readable skill history yet."
          )}{" "}
          {items[0] ? (
            <Button variant="link" className="h-auto p-0 text-body" onClick={items[0].go}>
              {items[0].title}
              {items.length > 1 ? ` and ${items.length - 1} more` : ""} →
            </Button>
          ) : (
            <span className="text-text-secondary">Everything is in order.</span>
          )}
        </p>
        <p className="mt-1 text-small text-text-secondary">
          {formatCount(props.skills.length)} skills ·{" "}
          <Button variant="link" className="h-auto p-0 text-small" onClick={() => go({})}>
            Open Skills
          </Button>
        </p>
      </div>
      {hasHistory && <MiniWeek values={w.days.total} />}
    </section>
  );
}

/** 3. The clean-up as prompts for your agent, which can run Skill Studio itself. */
export function AskVariant(props: SimpleProps) {
  const items = useItems(props);
  const addToast = useAppStore((s) => s.addToast);
  const agent = deploymentLabelFromAgentId(props.agents?.[0] ?? "claude-code");
  return (
    <section aria-label="Ask your agent" className="flex max-w-2xl flex-col gap-4">
      <p className="text-[1.375rem] leading-snug font-semibold">
        {items.length
          ? `${plural(items.length, "thing")} your agent can sort out`
          : "Nothing to sort out."}
      </p>
      <p className="-mt-2 text-small text-text-secondary">
        {agent} can run Skill Studio through its MCP server. Copy a prompt and paste it into a
        session.
      </p>
      {items.length > 0 && (
        <ul className="border-t border-border-subtle">
          {items.map((item) => (
            <Row key={item.id} item={{ ...item, why: `“${item.ask}”` }}>
              <span className="flex gap-1">
                <Button size="sm" variant="ghost" onClick={item.go}>
                  {item.action}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    void navigator.clipboard?.writeText(item.ask);
                    addToast({ type: "success", title: "Prompt copied" });
                  }}
                >
                  <Copy aria-hidden /> Copy prompt
                </Button>
              </span>
            </Row>
          ))}
        </ul>
      )}
    </section>
  );
}

/** 4. The single most important thing, with its reason and one action; Next shows the following one. */
export function FocusVariant(props: SimpleProps) {
  const items = useItems(props);
  const [index, setIndex] = useState(0);
  const w = week(props.snapshot, props.agents);
  const item = items[index % Math.max(1, items.length)];
  if (!item)
    return (
      <section aria-label="Focus" className="rounded-xl border border-border-subtle px-8 py-10">
        <p className="text-[1.375rem] font-semibold">Nothing needs you.</p>
        <p className="mt-1 text-small text-text-secondary">
          {w.total
            ? `${plural(w.total, "use")} this week across ${plural(w.used, "skill")}.`
            : "No readable skill history yet."}
        </p>
      </section>
    );
  return (
    <section
      aria-label="Focus"
      className="flex flex-col gap-5 rounded-xl border border-border-subtle px-8 py-8"
    >
      <p className="flex items-center gap-2 text-small text-text-secondary tabular-nums">
        <span className={`size-2 rounded-full ${item.tone}`} aria-hidden />
        {(index % items.length) + 1} of {items.length}
      </p>
      <div>
        <p className="text-[1.75rem] leading-tight font-semibold">{item.title}</p>
        <p className="mt-2 max-w-xl text-body text-text-secondary">{item.why}</p>
      </div>
      <div className="flex gap-2">
        <Button onClick={item.go}>
          {item.action} <ArrowRight aria-hidden />
        </Button>
        {items.length > 1 && (
          <Button variant="ghost" onClick={() => setIndex((i) => i + 1)}>
            Next
          </Button>
        )}
      </div>
    </section>
  );
}

/** 6. Ask in one line: every supported agent as a small visual (skills it lists, uses this
 * week), the week as bars, open items as counts, and the agent prompt one click away. */
export function AskLineVariant(props: SimpleProps & { listings: HarnessListing[] }) {
  const items = useItems(props);
  const addToast = useAppStore((s) => s.addToast);
  const [showPrompt, setShowPrompt] = useState(false);
  const w = week(props.snapshot, props.agents);
  const kept = new Set(props.agents ?? props.listings.map((r) => r.harness));
  const hasHistory = props.snapshot.invocations.length > 0;
  const missing = HARNESS_CATALOG.filter((row) => !kept.has(row.id));
  const prompt = items.length
    ? `Use Skill Studio to sort out my skills: fix what is broken, review updates before applying them, and park what I have not used in 30 days. Tell me what you changed.`
    : "";
  return (
    <section aria-label="Your skills at a glance" className="flex flex-col gap-5">
      <ul
        aria-label="Agents"
        className="flex divide-x divide-border-subtle rounded-lg border border-border-subtle"
      >
        {HARNESS_CATALOG.filter((row) => kept.has(row.id)).map((row) => {
          const listing = props.listings.find((r) => r.harness === row.id);
          const entries = (listing?.entries ?? []).filter((e) => e.state !== "hidden");
          const full = entries.filter((e) => e.state === "described").length;
          const uses = sum(w.days.byAgent.get(row.id) ?? []);
          const icon = harnessIdFromLabel(row.label);
          return (
            <li key={row.id} className="min-w-0 flex-1 px-2.5 py-2.5" title={row.label}>
              <p className="flex items-center gap-1.5 text-caption text-text-secondary">
                {icon && <HarnessIcon harness={icon} size={12} />}
                <span className="truncate">{row.label}</span>
              </p>
              <p className="mt-1 text-caption text-text-tertiary tabular-nums">
                <span className="text-body font-semibold text-text-primary">
                  {formatCount(entries.length)}
                </span>{" "}
                skills
              </p>
              <span
                role="img"
                className="mt-2 flex h-1 overflow-hidden rounded-full bg-bg-tertiary"
                aria-label={`${full} of ${entries.length} described in full`}
              >
                <span className="bg-data" style={{ flex: full }} />
                <span className="bg-warning" style={{ flex: entries.length - full }} />
              </span>
              {row.usageReader && hasHistory && (
                <p className="mt-1.5 truncate text-caption text-text-tertiary tabular-nums">
                  {plural(uses, "use")}
                </p>
              )}
            </li>
          );
        })}
        {missing.length > 0 && (
          <li
            className="flex shrink-0 items-center gap-1.5 px-3 text-caption text-text-tertiary"
            title={`Not set up: ${missing.map((row) => row.label).join(", ")}`}
          >
            {missing.map((row) => {
              const icon = harnessIdFromLabel(row.label);
              return (
                icon && (
                  <span key={row.id} className="opacity-50">
                    <HarnessIcon harness={icon} size={12} />
                  </span>
                )
              );
            })}
            <span>{missing.length} not set up</span>
          </li>
        )}
      </ul>
      <div className="flex flex-wrap items-end justify-between gap-6">
        <div className="flex items-end gap-4">
          <div>
            <p className="text-caption text-text-tertiary">Uses this week</p>
            {hasHistory ? (
              <p className="text-[2rem] leading-none font-semibold tabular-nums">
                {formatCount(w.total)}
              </p>
            ) : (
              <p className="mt-1 text-small text-text-secondary">No readable history yet</p>
            )}
          </div>
          {hasHistory && <MiniWeek values={w.days.total} />}
        </div>
        <ul aria-label="Open items" className="flex flex-wrap gap-1.5">
          {items.map((item) => (
            <li key={item.id}>
              <Button size="sm" variant="outline" onClick={item.go} title={item.title}>
                <span className={`size-2 rounded-full ${item.tone}`} aria-hidden />
                <span className="font-semibold tabular-nums">
                  {formatCount(Number(item.title.split(" ")[0].replace(/,/g, "")) || 0)}
                </span>
                {{ broken: "broken", updates: "updates", cut: "cut", unused: "unused" }[item.id]}
              </Button>
            </li>
          ))}
          {items.length > 0 && (
            <li>
              <Button size="sm" aria-expanded={showPrompt} onClick={() => setShowPrompt((v) => !v)}>
                Ask your agent
              </Button>
            </li>
          )}
        </ul>
      </div>
      {showPrompt && (
        <div className="flex items-start gap-3 rounded-lg border border-border-subtle bg-bg-secondary p-3">
          <p className="flex-1 font-mono text-small leading-relaxed select-text">{prompt}</p>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              void navigator.clipboard?.writeText(prompt);
              addToast({ type: "success", title: "Prompt copied" });
            }}
          >
            <Copy aria-hidden /> Copy
          </Button>
        </div>
      )}
    </section>
  );
}
