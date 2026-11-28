// ============================================================================
// Skill Studio - Agent benchmark section
// Renders the results file from tools/agent-bench as three measures: time, tool
// calls and wrong answers, by hand against Skill Studio, one bar per side.
// Every run stays in the table below.
// ============================================================================
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import * as stylex from "@stylexjs/stylex";

import rawData from "../data/agent-benchmark.json";
import { siteTokens } from "../SiteTheme.stylex";
import { MCP_DOCS_URL } from "../site-links";
import { chartTokens } from "./BenchmarkChart.stylex";
import {
  HARNESS_NAMES,
  harnessName,
  benchSummary,
  cellsForTask,
  formatMetric,
  formatRatio,
  rowKey,
  type BenchCell,
  type BenchData,
  type BenchMethod,
  type BenchMetric,
  type BenchSummary,
  type BenchTask,
} from "./benchmark-summary";
import { homeSectionStyles } from "./HomeSection.stylex";

// SAFETY: `bench.py report` writes this file in the BenchData shape.
const data = rawData as BenchData;

const TASKS: ReadonlyArray<{ id: BenchTask; label: string; question: string }> = [
  {
    id: "usage",
    label: "Skill usage",
    question: "How many skills do I have, which did I use in the last 45 days, and how often?",
  },
  {
    id: "update",
    label: "Pack update",
    question: "A skill pack has a new version. Which of my agent copies are still old?",
  },
  {
    id: "share",
    label: "Share a setup",
    question: "Which repos did my skills come from, and which did I write myself?",
  },
];

const METRICS: ReadonlyArray<{ id: BenchMetric; label: string }> = [
  { id: "duration_s", label: "Time" },
  { id: "cost_usd", label: "Cost" },
  { id: "correct_share", label: "Correct" },
  { id: "tool_calls", label: "Tool calls" },
];

const METHOD_LABELS = {
  raw: "By hand",
  cli: "Skill Studio CLI",
  mcp: "Skill Studio MCP",
} satisfies Record<BenchMethod, string>;

// Time a full-width bar takes to grow. Every bar grows at that speed, so a shorter bar stops
// sooner: the Skill Studio bar lands first, as its answer did.
const GROW_MS = 1400;
const ROW_DELAY_MS = 120;
// Share of a measure on screen before its bars grow, so the short ones do not finish unseen.
const MEASURE_IN_VIEW = 0.5;
const WIN_RATIO = 1.25;
const MIN_BAR_PX = 3;
// Room for the value at the end of a full-width bar.
const VALUE_ROOM = 120;
const VALUE_ROOM_MOBILE = 96;
const HARNESS_ORDER = [...HARNESS_NAMES.keys()];

interface BarValue {
  value: number;
  /** Share of the track a finished bar fills. */
  share: number;
  /** The label for a value on the way up, and for the final value. */
  text: (value: number) => string;
}

interface Measure {
  id: string;
  big: string;
  word: string;
  byHand: BarValue;
  studio: BarValue;
}

// The ratio of the two numbers the reader sees, so the page agrees with itself.
function lead(byHand: number, studio: number, win: string, lose: string, tie: string) {
  const times = byHand / studio;
  if (times >= WIN_RATIO) return { big: formatRatio(times), word: win };
  if (times <= 1 / WIN_RATIO) return { big: formatRatio(1 / times), word: lose };
  return { big: "Same", word: `${tie}, within 25%` };
}

const secondsText = (value: number) => `${Math.round(value)} s`;

// A median over an even count of pairs can end in .5, and shows as it is. Any other fraction is
// a value on the way up, and shows whole.
const callsText = (value: number) => {
  const shown = Number.isInteger(value * 2) ? value : Math.round(value);
  return `${shown} ${shown === 1 ? "call" : "calls"}`;
};

function measuresFor({ byHand, studio }: BenchSummary): Measure[] {
  const seconds = { byHand: Math.round(byHand.seconds), studio: Math.round(studio.seconds) };
  const slowest = Math.max(seconds.byHand, seconds.studio);
  const mostCalls = Math.max(byHand.toolCalls, studio.toolCalls);
  const noneWrong = byHand.wrong === 0 && studio.wrong === 0;
  return [
    {
      id: "time",
      ...lead(seconds.byHand, seconds.studio, "faster", "slower", "time"),
      byHand: { value: seconds.byHand, share: seconds.byHand / slowest, text: secondsText },
      studio: { value: seconds.studio, share: seconds.studio / slowest, text: secondsText },
    },
    {
      id: "calls",
      ...lead(
        byHand.toolCalls,
        studio.toolCalls,
        "fewer tool calls",
        "more tool calls",
        "tool calls",
      ),
      byHand: { value: byHand.toolCalls, share: byHand.toolCalls / mostCalls, text: callsText },
      studio: { value: studio.toolCalls, share: studio.toolCalls / mostCalls, text: callsText },
    },
    {
      id: "wrong",
      big: noneWrong ? "0" : `${byHand.wrong} → ${studio.wrong}`,
      word: noneWrong ? "wrong answers, both ways" : "wrong answers",
      // A share of all answers, so 1 wrong answer of 24 does not fill the bar.
      byHand: {
        value: byHand.wrong,
        share: byHand.wrong / byHand.answers,
        text: (v) => `${Math.round(v)} of ${byHand.answers}`,
      },
      studio: {
        value: studio.wrong,
        share: studio.wrong / studio.answers,
        text: (v) => `${Math.round(v)} of ${studio.answers}`,
      },
    },
  ];
}

const SUMMARIES = TASKS.flatMap((task) => {
  const summary = benchSummary(data, task.id);
  return summary ? [{ ...task, pairs: summary.pairs, measures: measuresFor(summary) }] : [];
});

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

// Milliseconds since the bars of one measure began to grow. Starts finished, so the server render
// and a reader with reduced motion see the result; rewinds on mount and plays once the measure
// scrolls into view. Each measure has its own clock: on a phone they are a screen apart. Reduced
// motion turned on at any point ends the animation at once.
function useGrowClock(target: RefObject<HTMLElement | null>, total: number) {
  const [elapsed, setElapsed] = useState(Infinity);

  useLayoutEffect(() => {
    const node = target.current;
    const reduced = window.matchMedia(REDUCED_MOTION_QUERY);
    if (!node || reduced.matches || !("IntersectionObserver" in window)) return;
    setElapsed(0);
    let frame = 0;
    // `disconnect()` keeps entries already queued, so a queued callback could start the
    // bars again after reduced motion showed the final values.
    let done = false;
    const tick = (start: number) => (now: number) => {
      if (done) return;
      const ms = now - start;
      setElapsed(ms < total ? ms : Infinity);
      if (ms < total) frame = requestAnimationFrame(tick(start));
    };
    const observer = new IntersectionObserver(
      (entries) => {
        // The newest entry is the current state. `isIntersecting` is also true below the
        // threshold, when only an edge shows, so the ratio decides.
        if (done || entries[entries.length - 1].intersectionRatio < MEASURE_IN_VIEW) return;
        observer.disconnect();
        frame = requestAnimationFrame(tick(performance.now()));
      },
      { threshold: MEASURE_IN_VIEW },
    );
    const finish = () => {
      if (!reduced.matches) return;
      done = true;
      observer.disconnect();
      cancelAnimationFrame(frame);
      setElapsed(Infinity);
    };
    observer.observe(node);
    reduced.addEventListener("change", finish);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      reduced.removeEventListener("change", finish);
    };
  }, [target, total]);

  return elapsed;
}

interface BarLineProps {
  name: string;
  tone: "byHand" | "studio";
  bar: BarValue;
  /** Share of the track a full-width bar has reached. */
  grown: number;
}

function BarLine({ name, tone, bar, grown }: BarLineProps) {
  const shown = Math.min(grown, bar.share);
  const done = grown >= bar.share;
  const end = shown > 0 ? `max(${shown * 100}%, ${MIN_BAR_PX}px)` : "0px";
  return (
    <div {...stylex.props(styles.barLine)}>
      <span {...stylex.props(styles.barName)}>{name}</span>
      <span {...stylex.props(styles.track)}>
        <span {...stylex.props(styles.clip)}>
          <span
            {...stylex.props(
              styles.fill,
              barTone[tone],
              dynamic.transform(`translateX(calc(${end} - 100%))`),
            )}
          />
        </span>
        <span {...stylex.props(styles.valueWindow)}>
          <span {...stylex.props(styles.valueSlot, dynamic.transform(`translateX(${end})`))}>
            <span
              {...stylex.props(
                styles.value,
                bar.share === 0 && styles.valueFlush,
                done && tone === "studio" && styles.valueOn,
              )}
            >
              {bar.text(done ? bar.value : (bar.value * shown) / bar.share)}
            </span>
          </span>
        </span>
      </span>
    </div>
  );
}

interface MeasureRowProps {
  measure: Measure;
  row: number;
  /** The reader picked this task, so the big number changes in front of them. */
  swapped: boolean;
}

function MeasureRow({ measure, row, swapped }: MeasureRowProps) {
  const rowRef = useRef<HTMLLIElement>(null);
  const delay = row * ROW_DELAY_MS;
  const elapsed = useGrowClock(rowRef, delay + GROW_MS);
  const grown = Math.min(Math.max((elapsed - delay) / GROW_MS, 0), 1);
  const { byHand, studio } = measure;
  return (
    <li ref={rowRef} {...stylex.props(styles.measure)}>
      <p {...stylex.props(styles.lead)}>
        <span
          {...stylex.props(
            styles.big,
            swapped && styles.bigSwap,
            swapped && dynamic.delay(`${row * 60}ms`),
          )}
        >
          {measure.big}
        </span>
        <span {...stylex.props(styles.word)}>{measure.word}</span>
      </p>
      <div
        role="img"
        aria-label={`By hand: ${byHand.text(byHand.value)}. Skill Studio: ${studio.text(studio.value)}.`}
        {...stylex.props(styles.bars)}
      >
        <BarLine name="By hand" tone="byHand" bar={byHand} grown={grown} />
        <BarLine name="Skill Studio" tone="studio" bar={studio} grown={grown} />
      </div>
    </li>
  );
}

interface SegmentedProps<T extends string> {
  label: string;
  options: ReadonlyArray<{ id: T; label: string }>;
  value: T;
  onChange: (value: T) => void;
}

const STEP_KEYS = new Map([
  ["ArrowRight", 1],
  ["ArrowDown", 1],
  ["ArrowLeft", -1],
  ["ArrowUp", -1],
]);

function Segmented<T extends string>({ label, options, value, onChange }: SegmentedProps<T>) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const highlightRef = useRef<HTMLSpanElement>(null);
  const hasPositioned = useRef(false);
  const viaKeyboard = useRef(false);
  const activeIndex = options.findIndex((option) => option.id === value);

  useLayoutEffect(() => {
    const sync = (animate: boolean) => {
      const el = refs.current[activeIndex];
      const pill = highlightRef.current;
      if (!el || !pill) return;
      pill.style.transition = animate ? "" : "none";
      pill.style.width = `${el.offsetWidth}px`;
      pill.style.height = `${el.offsetHeight}px`;
      pill.style.transform = `translate(${el.offsetLeft}px, ${el.offsetTop}px)`;
      pill.style.opacity = "1";
    };
    const animate = hasPositioned.current && !viaKeyboard.current;
    viaKeyboard.current = false;
    sync(animate);
    hasPositioned.current = true;
    const onResize = () => sync(false);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [activeIndex]);

  const onKeyDown = (event: KeyboardEvent) => {
    const step = STEP_KEYS.get(event.key);
    if (!step) return;
    event.preventDefault();
    viaKeyboard.current = true;
    const next = (activeIndex + step + options.length) % options.length;
    onChange(options[next].id);
    refs.current[next]?.focus();
  };

  return (
    <div
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      {...stylex.props(styles.segmented)}
    >
      <span ref={highlightRef} aria-hidden="true" {...stylex.props(styles.segmentHighlight)} />
      {options.map((option, i) => (
        <button
          key={option.id}
          ref={(el) => {
            refs.current[i] = el;
          }}
          type="button"
          role="radio"
          aria-checked={i === activeIndex}
          tabIndex={i === activeIndex ? 0 : -1}
          onClick={() => onChange(option.id)}
          {...stylex.props(styles.segment, i === activeIndex && styles.segmentOn)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function cellText(cell: BenchCell | undefined, metric: BenchMetric): string {
  const stat = cell?.[metric];
  if (!stat) return "–";
  const median = formatMetric(metric, stat.median);
  const estimate = metric === "cost_usd" && cell.cost_source === "estimate" ? " est." : "";
  return stat.min === stat.max
    ? `${median}${estimate}`
    : `${median}${estimate} (${formatMetric(metric, stat.min)}–${formatMetric(metric, stat.max)})`;
}

function ResultsTable({ task }: { task: BenchTask }) {
  const cells = cellsForTask(data, task).sort(
    (a, b) =>
      HARNESS_ORDER.indexOf(a.harness) - HARNESS_ORDER.indexOf(b.harness) ||
      a.label.localeCompare(b.label) ||
      a.method.localeCompare(b.method),
  );
  return (
    <div {...stylex.props(styles.tableWrap)}>
      <table {...stylex.props(styles.table)}>
        <caption {...stylex.props(styles.visuallyHidden)}>
          Median and range of each harness, model and method
        </caption>
        <thead>
          <tr>
            {["Agent", "Model", "Method", ...METRICS.map((m) => m.label), "Runs"].map((h) => (
              <th key={h} scope="col" {...stylex.props(styles.th)}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {cells.map((cell) => (
            <tr key={`${rowKey(cell)}:${cell.method}`}>
              <td {...stylex.props(styles.td)}>{harnessName(cell.harness)}</td>
              <td {...stylex.props(styles.td)}>{cell.label}</td>
              <td {...stylex.props(styles.td)}>{METHOD_LABELS[cell.method]}</td>
              {METRICS.map((metric) => (
                <td key={metric.id} {...stylex.props(styles.td, styles.num)}>
                  {cellText(cell, metric.id)}
                </td>
              ))}
              <td {...stylex.props(styles.td, styles.num)}>{cell.runs}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function BenchmarkSection() {
  const [task, setTask] = useState<BenchTask>(SUMMARIES[0]?.id ?? "usage");
  const [picks, setPicks] = useState(0);
  const summary = SUMMARIES.find((s) => s.id === task) ?? SUMMARIES[0];
  const harnesses = new Set(data.cells.map((c) => c.harness)).size;
  const models = new Set(data.cells.map((c) => c.model)).size;
  const runDate = new Date(data.generated).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });

  const pick = (next: BenchTask) => {
    setTask(next);
    setPicks((count) => count + 1);
  };

  if (!summary) return null;

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        Three skill questions, by hand and with Skill Studio.
      </h2>

      {SUMMARIES.length > 1 && (
        <Segmented label="Task" options={SUMMARIES} value={task} onChange={pick} />
      )}
      <p {...stylex.props(styles.question)}>“{summary.question}”</p>

      {/* A new task mounts the measures again, so their bars grow from zero. */}
      <ul key={`${task}:${picks}`} {...stylex.props(styles.measures)}>
        {summary.measures.map((measure, row) => (
          <MeasureRow key={measure.id} measure={measure} row={row} swapped={picks > 0} />
        ))}
      </ul>
      <p {...stylex.props(styles.note)}>
        Medians over {summary.pairs} agent and model pairs, three runs each. Skill Studio here is
        its MCP server; the CLI is in “Every run”. The benchmark has three tasks: skill usage, a
        pack update and sharing a setup. It does not measure Model listing or Sessions.
      </p>

      <details {...stylex.props(styles.details)}>
        <summary {...stylex.props(styles.summary)}>Every run &amp; methodology</summary>
        <ResultsTable task={task} />
        <p {...stylex.props(styles.methodText)}>
          {data.total_runs} runs on {runDate}: {harnesses} agents, {models} models, three runs each.
          Every run got a fresh copy of a made-up home folder with known answers and identical
          prompts, scored automatically against the fixture ground truth. Your numbers will differ.
        </p>
      </details>

      <div {...stylex.props(styles.links)}>
        <a href="#download" {...stylex.props(homeSectionStyles.textLink)}>
          Get the Mac app <span aria-hidden="true">→</span>
        </a>
        <a href={MCP_DOCS_URL} {...stylex.props(homeSectionStyles.textLink)}>
          Set up MCP <span aria-hidden="true">→</span>
        </a>
      </div>
    </>
  );
}

const MOBILE = "@media (max-width: 600px)";
const TABLET = "@media (max-width: 900px)";
const HOVER = "@media (hover: hover) and (pointer: fine)";
const REDUCED_MOTION = "@media (prefers-reduced-motion: reduce)";

const swapIn = stylex.keyframes({
  from: { filter: "blur(4px)", opacity: 0, transform: "translateY(8px)" },
  to: { filter: "blur(0)", opacity: 1, transform: "none" },
});

const dynamic = stylex.create({
  delay: (animationDelay: string) => ({ animationDelay }),
  transform: (transform: string) => ({ transform }),
});

const barTone = stylex.create({
  byHand: { backgroundColor: chartTokens.byHand },
  studio: { backgroundColor: chartTokens.mcp },
});

const styles = stylex.create({
  title: { marginBottom: 20 },
  question: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.5,
    margin: "16px 0 0",
    maxWidth: "60ch",
    textWrap: "pretty",
    [MOBILE]: { fontSize: 16 },
  },
  measures: { display: "grid", listStyle: "none", margin: "36px 0 0", padding: 0 },
  measure: {
    alignItems: "center",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 48,
    gridTemplateColumns: "300px minmax(0, 1fr)",
    paddingBlock: 30,
    [TABLET]: { gap: 32, gridTemplateColumns: "210px minmax(0, 1fr)" },
    [MOBILE]: { gap: 16, gridTemplateColumns: "minmax(0, 1fr)", paddingBlock: 24 },
  },
  lead: { display: "grid", gap: 4, margin: 0 },
  big: {
    color: siteTokens.text,
    fontSize: "clamp(52px, 6vw, 76px)",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 720,
    letterSpacing: "-.05em",
    lineHeight: 1,
  },
  // Only after the reader picks a task. The first numbers are at rest when the page loads.
  bigSwap: {
    animationDuration: "280ms",
    animationFillMode: "backwards",
    animationName: { default: swapIn, [REDUCED_MOTION]: "none" },
    animationTimingFunction: "cubic-bezier(0.32, 0.72, 0, 1)",
  },
  word: { color: siteTokens.muted, fontSize: 17, fontWeight: 560, [MOBILE]: { fontSize: 16 } },
  bars: { display: "grid", gap: 12, [MOBILE]: { gap: 10 } },
  barLine: {
    alignItems: "center",
    display: "grid",
    gap: 16,
    gridTemplateColumns: "96px minmax(0, 1fr)",
    [MOBILE]: { gap: 10, gridTemplateColumns: "86px minmax(0, 1fr)" },
  },
  barName: { color: siteTokens.muted, fontSize: 14, fontWeight: 560, whiteSpace: "nowrap" },
  track: {
    display: "block",
    height: 26,
    marginRight: VALUE_ROOM,
    position: "relative",
    [MOBILE]: { height: 22, marginRight: VALUE_ROOM_MOBILE },
  },
  clip: { borderRadius: 5, inset: 0, overflow: "hidden", position: "absolute" },
  // A full-width bar that slides in from the left, so growing it never changes the layout.
  fill: { borderRadius: 5, inset: 0, position: "absolute", willChange: "transform" },
  // The track plus the value room. It clips the slot, which is as wide as the track and would
  // reach past the edge of the page at the end of a long bar.
  valueWindow: {
    bottom: 0,
    left: 0,
    overflow: "hidden",
    position: "absolute",
    right: -VALUE_ROOM,
    top: 0,
    [MOBILE]: { right: -VALUE_ROOM_MOBILE },
  },
  valueSlot: {
    alignItems: "center",
    bottom: 0,
    display: "flex",
    left: 0,
    position: "absolute",
    right: VALUE_ROOM,
    top: 0,
    willChange: "transform",
    [MOBILE]: { right: VALUE_ROOM_MOBILE },
  },
  value: {
    color: siteTokens.muted,
    fontSize: 20,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 640,
    letterSpacing: "-.02em",
    paddingLeft: 12,
    transition: "color 200ms ease-out",
    whiteSpace: "nowrap",
    [MOBILE]: { fontSize: 17, paddingLeft: 10 },
    [REDUCED_MOTION]: { transition: "none" },
  },
  valueFlush: { paddingLeft: 0, [MOBILE]: { paddingLeft: 0 } },
  valueOn: { color: siteTokens.text },
  note: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.6,
    margin: "4px 0 0",
    maxWidth: "64ch",
    textWrap: "pretty",
  },
  segmented: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "inline-flex",
    flexWrap: "wrap",
    gap: 2,
    padding: 3,
    position: "relative",
  },
  segmentHighlight: {
    backgroundColor: siteTokens.background,
    borderRadius: 7,
    boxShadow: `inset 0 0 0 1px ${siteTokens.border}`,
    left: 0,
    opacity: 0,
    pointerEvents: "none",
    position: "absolute",
    top: 0,
    transition: {
      default:
        "transform 240ms cubic-bezier(0.25, 1, 0.5, 1), width 240ms cubic-bezier(0.25, 1, 0.5, 1), height 240ms cubic-bezier(0.25, 1, 0.5, 1)",
      [REDUCED_MOTION]: "none",
    },
  },
  segment: {
    backgroundColor: "transparent",
    borderRadius: 7,
    borderWidth: 0,
    color: siteTokens.muted,
    cursor: "pointer",
    fontFamily: "inherit",
    fontSize: 13,
    fontWeight: 600,
    minHeight: 34,
    paddingInline: 12,
    position: "relative",
    transition: "color 150ms ease, transform 150ms cubic-bezier(0.25, 0.46, 0.45, 0.94)",
    [HOVER]: { ":hover": { color: siteTokens.text } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 1 },
    [MOBILE]: { minHeight: 40, paddingInline: 10 },
  },
  segmentOn: {
    color: siteTokens.text,
  },
  details: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    marginTop: 20,
    paddingTop: 16,
  },
  summary: {
    color: siteTokens.text,
    cursor: "pointer",
    fontSize: 14,
    fontWeight: 600,
    minHeight: 32,
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 2 },
  },
  tableWrap: { marginTop: 12, overflowX: "auto" },
  table: { borderCollapse: "collapse", fontSize: 13, minWidth: 760, width: "100%" },
  th: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    color: siteTokens.muted,
    fontWeight: 560,
    padding: "8px 10px 8px 0",
    textAlign: "left",
    whiteSpace: "nowrap",
  },
  td: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    color: siteTokens.text,
    padding: "8px 10px 8px 0",
    whiteSpace: "nowrap",
  },
  num: { fontVariantNumeric: "tabular-nums" },
  methodText: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.6,
    margin: "14px 0 0",
    maxWidth: "64ch",
    textWrap: "pretty",
  },
  links: { display: "flex", flexWrap: "wrap", gap: "0 20px", marginTop: 8 },
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
});
