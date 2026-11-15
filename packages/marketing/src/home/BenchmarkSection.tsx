// ============================================================================
// Skill Studio - Agent benchmark section
// Renders the results file from tools/agent-bench as a race: each agent and
// model answers the same questions by hand and with Skill Studio, one tick per
// tool call, on a sped-up clock. Every run stays in the table below.
// ============================================================================
import { useLayoutEffect, useRef, useState, type RefObject } from "react";
import { Check, RotateCcw, X } from "lucide-react";
import * as stylex from "@stylexjs/stylex";

import rawData from "../data/agent-benchmark.json";
import { siteTokens } from "../SiteTheme.stylex";
import { GITHUB_REPOSITORY_URL } from "../site-links";
import { chartTokens } from "./BenchmarkChart.stylex";
import {
  HARNESS_NAMES,
  harnessName,
  cellsForTask,
  featuredPairs,
  formatMetric,
  formatRatio,
  raceHeadline,
  racePairs,
  rowKey,
  type BenchCell,
  type BenchData,
  type BenchMethod,
  type BenchMetric,
  type BenchTask,
  type RaceHeadline,
  type RacePair,
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

const PLAY_SECONDS = 5;
const WIN_RATIO = 1.25;
const HARNESS_ORDER = [...HARNESS_NAMES.keys()];
const METHODOLOGY_URL = `${GITHUB_REPOSITORY_URL}/blob/main/tools/agent-bench/METHODOLOGY.md`;
const HARNESS_URL = `${GITHUB_REPOSITORY_URL}/tree/main/tools/agent-bench`;
const RESULTS_URL = `${GITHUB_REPOSITORY_URL}/blob/main/packages/marketing/src/data/agent-benchmark.json`;

const RACES = TASKS.map((task) => {
  const pairs = racePairs(data, task.id);
  return { ...task, lanes: featuredPairs(pairs), headline: raceHeadline(pairs) };
}).filter((race) => race.lanes.length > 0);

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

// Seconds on the race clock. Starts finished, so the server render and a reader with reduced
// motion see the result; rewinds on mount and plays once the race scrolls into view.
function useRaceClock(target: RefObject<HTMLElement | null>, length: number) {
  const [elapsed, setElapsed] = useState(Infinity);

  useLayoutEffect(() => {
    const node = target.current;
    if (!node || prefersReducedMotion() || !("IntersectionObserver" in window)) return;
    setElapsed(0);
    let frame = 0;
    const play = () => {
      const speed = length / PLAY_SECONDS;
      const start = performance.now();
      const tick = (now: number) => {
        const seconds = ((now - start) / 1000) * speed;
        setElapsed(seconds);
        if (seconds < length) frame = requestAnimationFrame(tick);
      };
      frame = requestAnimationFrame(tick);
    };
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        play();
      },
      { threshold: 0.4 },
    );
    observer.observe(node);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [target, length]);

  return elapsed;
}

function correctCount(cell: BenchCell) {
  return Math.round((cell.correct_share?.median ?? 0) * cell.questions);
}

interface LaneProps {
  cell: BenchCell;
  elapsed: number;
  length: number;
}

function Lane({ cell, elapsed, length }: LaneProps) {
  const seconds = cell.duration_s!.median;
  const calls = Math.max(1, Math.round(cell.tool_calls!.median));
  const shown = Math.min(elapsed, seconds);
  const done = elapsed >= seconds;
  const right = correctCount(cell);
  const allRight = right === cell.questions;
  const tone =
    cell.method === "raw" ? laneTone.raw : cell.method === "cli" ? laneTone.cli : laneTone.mcp;
  const pct = (value: number) => (value / length) * 100;
  const label = `${METHOD_LABELS[cell.method]}: ${formatMetric("duration_s", seconds)}, ${calls} tool ${calls === 1 ? "call" : "calls"}, ${right} of ${cell.questions} answers right`;

  return (
    <div {...stylex.props(styles.lane)} role="img" aria-label={label}>
      <span {...stylex.props(styles.laneName)}>{METHOD_LABELS[cell.method]}</span>
      <div {...stylex.props(styles.track)}>
        <span aria-hidden="true" {...stylex.props(styles.rail)} />
        <span
          aria-hidden="true"
          {...stylex.props(styles.progress, tone, dynamic.width(`${pct(shown)}%`))}
        />
        {Array.from({ length: calls }, (_, i) => {
          const at = ((i + 0.5) / calls) * seconds;
          return (
            <span
              key={i}
              aria-hidden="true"
              {...stylex.props(
                styles.tick,
                tone,
                dynamic.left(`${pct(at)}%`),
                elapsed >= at ? styles.tickOn : null,
              )}
            />
          );
        })}
        <span
          aria-hidden="true"
          {...stylex.props(
            styles.result,
            dynamic.left(`${pct(shown)}%`),
            done && styles.resultDone,
          )}
        >
          <span {...stylex.props(styles.time)}>{Math.round(shown)} s</span>
          {done &&
            (allRight ? (
              <span {...stylex.props(styles.verdict, styles.verdictRight)}>
                <Check size={13} strokeWidth={3} />
              </span>
            ) : (
              <span {...stylex.props(styles.verdict, styles.verdictWrong)}>
                <X size={13} strokeWidth={3} />
                {right}/{cell.questions}
              </span>
            ))}
        </span>
      </div>
    </div>
  );
}

function RaceRow({ pair, elapsed, length }: { pair: RacePair; elapsed: number; length: number }) {
  const won = elapsed >= pair.studio.duration_s!.median;
  return (
    <li {...stylex.props(styles.raceRow)}>
      <div {...stylex.props(styles.agent)}>
        <span {...stylex.props(styles.agentName)}>{harnessName(pair.harness)}</span>
        <span {...stylex.props(styles.agentModel)}>{pair.label}</span>
        <span {...stylex.props(styles.win, won && styles.winOn)}>
          {formatRatio(pair.speedup)} faster
        </span>
      </div>
      <div {...stylex.props(styles.lanes)}>
        <Lane cell={pair.byHand} elapsed={elapsed} length={length} />
        <Lane cell={pair.studio} elapsed={elapsed} length={length} />
      </div>
    </li>
  );
}

function Stat({ value, label, note }: { value: string; label: string; note: string }) {
  return (
    <div {...stylex.props(styles.stat)}>
      <dt {...stylex.props(styles.statLabel)}>{label}</dt>
      <dd {...stylex.props(styles.statValue)}>{value}</dd>
      <dd {...stylex.props(styles.statNote)}>{note}</dd>
    </div>
  );
}

const pairName = (pair: RacePair) => `Best: ${harnessName(pair.harness)}, ${pair.label}`;

function Headline({ headline }: { headline: RaceHeadline }) {
  const percent = (value: number) => `${Math.round(value * 100)}%`;
  const { speedup, callRatio, costRatio } = headline;
  return (
    <dl {...stylex.props(styles.stats)}>
      {speedup.speedup >= WIN_RATIO && (
        <Stat value={formatRatio(speedup.speedup)} label="faster" note={pairName(speedup)} />
      )}
      {callRatio.callRatio >= WIN_RATIO && (
        <Stat
          value={formatRatio(callRatio.callRatio)}
          label="fewer tool calls"
          note={pairName(callRatio)}
        />
      )}
      {headline.correctWithStudio > headline.correctByHand && (
        <Stat
          value={percent(headline.correctWithStudio)}
          label="answers right"
          note={`${percent(headline.correctByHand)} by hand`}
        />
      )}
      {costRatio.costRatio >= WIN_RATIO && (
        <Stat
          value={formatRatio(costRatio.costRatio)}
          label="lower cost"
          note={pairName(costRatio)}
        />
      )}
    </dl>
  );
}

interface SegmentedProps<T extends string> {
  label: string;
  options: ReadonlyArray<{ id: T; label: string }>;
  value: T;
  onChange: (value: T) => void;
}

function Segmented<T extends string>({ label, options, value, onChange }: SegmentedProps<T>) {
  return (
    <div role="group" aria-label={label} {...stylex.props(styles.segmented)}>
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={value === option.id}
          onClick={() => onChange(option.id)}
          {...stylex.props(styles.segment, value === option.id && styles.segmentOn)}
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

const raceLength = (lanes: readonly RacePair[]) =>
  Math.max(...lanes.map((p) => Math.max(p.byHand.duration_s!.median, p.studio.duration_s!.median)));

function Race({ lanes }: { lanes: readonly RacePair[] }) {
  const raceRef = useRef<HTMLOListElement>(null);
  const length = raceLength(lanes);
  const elapsed = useRaceClock(raceRef, length);
  return (
    <ol ref={raceRef} {...stylex.props(styles.race)}>
      {lanes.map((pair) => (
        <RaceRow key={pair.key} pair={pair} elapsed={elapsed} length={length} />
      ))}
    </ol>
  );
}

export function BenchmarkSection() {
  const [task, setTask] = useState<BenchTask>(RACES[0]?.id ?? "usage");
  const [runId, setRunId] = useState(0);
  const race = RACES.find((r) => r.id === task) ?? RACES[0];
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
    setRunId((r) => r + 1);
  };

  if (!race) return null;

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        Same agent. Same question. Skill Studio answers first.
      </h2>

      {RACES.length > 1 && <Segmented label="Task" options={RACES} value={task} onChange={pick} />}
      <p {...stylex.props(styles.question)}>“{race.question}”</p>

      {race.headline && <Headline headline={race.headline} />}

      <div {...stylex.props(styles.raceHead)}>
        <ul {...stylex.props(styles.key)} aria-label="Legend">
          <li {...stylex.props(styles.keyItem)}>
            <span aria-hidden="true" {...stylex.props(styles.keyTick)} />
            One tool call
          </li>
          <li {...stylex.props(styles.keyItem)}>
            <Check size={13} strokeWidth={3} aria-hidden="true" />
            All answers right
          </li>
        </ul>
        <button
          type="button"
          onClick={() => setRunId((r) => r + 1)}
          {...stylex.props(styles.replay)}
        >
          <RotateCcw size={13} aria-hidden="true" />
          Replay
        </button>
      </div>

      <Race key={`${task}:${runId}`} lanes={race.lanes} />

      <details {...stylex.props(styles.details)}>
        <summary {...stylex.props(styles.summary)}>Every run</summary>
        <ResultsTable task={task} />
        <p {...stylex.props(styles.methodText)}>
          {data.total_runs} runs on {runDate}: {harnesses} agents, {models} models, three runs each.
          Every run got a fresh copy of a made-up home folder with known answers. The race shows
          medians on a clock sped up {Math.round(raceLength(race.lanes) / PLAY_SECONDS)}×. Your
          numbers will differ.
        </p>
      </details>

      <div {...stylex.props(styles.links)}>
        <a href={METHODOLOGY_URL} {...stylex.props(homeSectionStyles.textLink)}>
          How we tested <span aria-hidden="true">→</span>
        </a>
        <a href={HARNESS_URL} {...stylex.props(homeSectionStyles.textLink)}>
          Run it yourself <span aria-hidden="true">→</span>
        </a>
        <a href={RESULTS_URL} {...stylex.props(homeSectionStyles.textLink)}>
          All results <span aria-hidden="true">→</span>
        </a>
      </div>
    </>
  );
}

const MOBILE = "@media (max-width: 600px)";

const dynamic = stylex.create({
  left: (left: string) => ({ left }),
  width: (width: string) => ({ width }),
});

const laneTone = stylex.create({
  raw: { backgroundColor: chartTokens.byHand },
  cli: { backgroundColor: chartTokens.cli },
  mcp: { backgroundColor: chartTokens.mcp },
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
  stats: {
    display: "grid",
    gap: 24,
    gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))",
    margin: "36px 0 0",
  },
  stat: {
    borderTopColor: siteTokens.text,
    borderTopStyle: "solid",
    borderTopWidth: 2,
    display: "grid",
    gridTemplateAreas: '"value" "label" "note"',
    paddingTop: 14,
  },
  statLabel: { color: siteTokens.text, fontSize: 15, fontWeight: 620, gridArea: "label" },
  statValue: {
    color: siteTokens.text,
    fontSize: "clamp(48px, 6vw, 72px)",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 720,
    gridArea: "value",
    letterSpacing: "-.045em",
    lineHeight: 1.05,
    margin: 0,
  },
  statNote: { color: siteTokens.muted, fontSize: 12, gridArea: "note", margin: "4px 0 0" },
  raceHead: {
    alignItems: "center",
    display: "flex",
    gap: 16,
    justifyContent: "space-between",
    marginTop: 48,
  },
  key: {
    color: siteTokens.muted,
    display: "flex",
    flexWrap: "wrap",
    fontSize: 13,
    gap: "6px 18px",
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  keyItem: { alignItems: "center", display: "flex", gap: 7 },
  keyTick: { backgroundColor: siteTokens.muted, borderRadius: 1, height: 12, width: 2 },
  replay: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    fontFamily: "inherit",
    fontSize: 13,
    fontWeight: 600,
    gap: 6,
    minHeight: 36,
    paddingInline: 14,
    transition: "border-color 150ms ease-out",
    ":hover": { borderColor: siteTokens.muted },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 2 },
  },
  race: {
    display: "flex",
    flexDirection: "column",
    listStyle: "none",
    margin: "16px 0 0",
    padding: 0,
  },
  raceRow: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 24,
    gridTemplateColumns: "180px minmax(0, 1fr)",
    paddingBlock: 18,
    [MOBILE]: { gap: 10, gridTemplateColumns: "1fr", paddingBlock: 16 },
  },
  agent: {
    alignContent: "start",
    display: "grid",
    gap: 2,
    [MOBILE]: { alignItems: "baseline", display: "flex", flexWrap: "wrap", gap: "2px 8px" },
  },
  agentName: { color: siteTokens.text, fontSize: 15, fontWeight: 640 },
  agentModel: { color: siteTokens.muted, fontSize: 13 },
  win: {
    color: siteTokens.text,
    fontSize: 13,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 680,
    marginTop: 6,
    opacity: 0,
    transform: "translateY(4px)",
    transition: "opacity 240ms ease-out, transform 240ms ease-out",
    [MOBILE]: { marginTop: 0, marginLeft: "auto" },
  },
  winOn: { opacity: 1, transform: "none" },
  lanes: { display: "grid", gap: 10 },
  lane: {
    alignItems: "center",
    display: "grid",
    gap: 14,
    gridTemplateColumns: "120px minmax(0, 1fr)",
    [MOBILE]: { gap: 8, gridTemplateColumns: "76px minmax(0, 1fr)" },
  },
  laneName: { color: siteTokens.muted, fontSize: 12, fontWeight: 560, lineHeight: 1.2 },
  // The right padding keeps the time and verdict inside the row at the end of the slowest lane.
  track: { height: 22, marginRight: 132, position: "relative", [MOBILE]: { marginRight: 104 } },
  rail: {
    backgroundColor: chartTokens.grid,
    height: 2,
    left: 0,
    marginTop: -1,
    position: "absolute",
    right: 0,
    top: "50%",
  },
  progress: {
    borderRadius: 2,
    height: 2,
    left: 0,
    marginTop: -1,
    position: "absolute",
    top: "50%",
  },
  tick: {
    borderRadius: 1,
    height: 14,
    marginLeft: -1,
    marginTop: -7,
    opacity: 0,
    position: "absolute",
    top: "50%",
    transform: "scaleY(.3)",
    transition: "opacity 120ms ease-out, transform 160ms ease-out",
    width: 2,
  },
  tickOn: { opacity: 1, transform: "none" },
  result: {
    alignItems: "center",
    color: siteTokens.muted,
    display: "flex",
    gap: 6,
    marginLeft: 10,
    position: "absolute",
    top: "50%",
    transform: "translateY(-50%)",
    whiteSpace: "nowrap",
  },
  resultDone: { color: siteTokens.text },
  time: { fontSize: 13, fontVariantNumeric: "tabular-nums", fontWeight: 640 },
  verdict: {
    alignItems: "center",
    borderRadius: 999,
    display: "inline-flex",
    fontSize: 11,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 700,
    gap: 2,
    height: 18,
    justifyContent: "center",
    minWidth: 18,
    paddingInline: 3,
  },
  verdictRight: { backgroundColor: siteTokens.text, color: siteTokens.background },
  verdictWrong: {
    borderColor: siteTokens.muted,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    paddingInline: "3px 6px",
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
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    ":hover": { color: siteTokens.text },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 1 },
    [MOBILE]: { minHeight: 40, paddingInline: 10 },
  },
  segmentOn: {
    backgroundColor: siteTokens.background,
    boxShadow: `inset 0 0 0 1px ${siteTokens.border}`,
    color: siteTokens.text,
  },
  details: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    marginTop: 8,
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
