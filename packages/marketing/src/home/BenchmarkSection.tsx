// ============================================================================
// Skill Studio - Agent benchmark section
// Renders the results file from tools/agent-bench: one headline number per
// task, then every harness and model as a dot-and-range chart or a table.
// ============================================================================
import { useState } from "react";
import * as stylex from "@stylexjs/stylex";

import rawData from "../data/agent-benchmark.json";
import { siteTokens } from "../SiteTheme.stylex";
import { GITHUB_REPOSITORY_URL } from "../site-links";
import { chartTokens } from "./BenchmarkChart.stylex";
import {
  HARNESS_NAMES,
  harnessName,
  cellsForTask,
  formatMetric,
  formatRatio,
  rowKey,
  summarizeTask,
  type BenchCell,
  type BenchData,
  type BenchMethod,
  type BenchMetric,
  type BenchStat,
  type BenchTask,
} from "./benchmark-summary";
import { homeSectionStyles } from "./HomeSection.stylex";

// SAFETY: `bench.py report` writes this file in the BenchData shape.
const data = rawData as BenchData;

const TASKS: ReadonlyArray<{ id: BenchTask; label: string; question: string }> = [
  {
    id: "usage",
    label: "Skill usage",
    question:
      "How many skills are there, which ran in the last 45 days, and how often? Four questions with known answers.",
  },
  {
    id: "update",
    label: "Pack update",
    question:
      "A skill pack has a new version. How many of its skills do you have, and which agent copies are still old?",
  },
  {
    id: "share",
    label: "Share a setup",
    question: "Which repos did your skills come from, and which skills did you write yourself?",
  },
];

const METRICS: ReadonlyArray<{ id: BenchMetric; label: string; lowerIsBetter: boolean }> = [
  { id: "duration_s", label: "Time", lowerIsBetter: true },
  { id: "cost_usd", label: "Cost", lowerIsBetter: true },
  { id: "correct_share", label: "Correct", lowerIsBetter: false },
  { id: "tool_calls", label: "Tool calls", lowerIsBetter: true },
];

const METHODS: ReadonlyArray<{ id: BenchMethod; label: string; long: string }> = [
  { id: "raw", label: "By hand", long: "The agent reads the skill folders and logs itself" },
  { id: "cli", label: "CLI", long: "The agent runs the skill-studio CLI" },
  { id: "mcp", label: "MCP", long: "The agent calls the Skill Studio MCP server" },
];

const HARNESS_ORDER = [...HARNESS_NAMES.keys()];
const METHODOLOGY_URL = `${GITHUB_REPOSITORY_URL}/blob/main/tools/agent-bench/METHODOLOGY.md`;
const HARNESS_URL = `${GITHUB_REPOSITORY_URL}/tree/main/tools/agent-bench`;
const RESULTS_URL = `${GITHUB_REPOSITORY_URL}/blob/main/packages/marketing/src/data/agent-benchmark.json`;

interface Row {
  key: string;
  harness: string;
  label: string;
  cells: Partial<Record<BenchMethod, BenchCell>>;
}

function rowsFor(task: BenchTask): Row[] {
  const rows = new Map<string, Row>();
  for (const cell of cellsForTask(data, task)) {
    const key = rowKey(cell);
    const row = rows.get(key) ?? { key, harness: cell.harness, label: cell.label, cells: {} };
    row.cells[cell.method] = cell;
    rows.set(key, row);
  }
  return [...rows.values()].sort(
    (a, b) =>
      HARNESS_ORDER.indexOf(a.harness) - HARNESS_ORDER.indexOf(b.harness) ||
      a.label.localeCompare(b.label),
  );
}

function niceMax(value: number): number {
  if (value <= 0) return 1;
  const raw = value / 4;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * power).find((s) => s >= raw)!;
  return step * 4;
}

function speedWords(speedup: number | null) {
  if (speedup === null) return { ratio: "–", word: "" };
  if (speedup >= 1.15) return { ratio: formatRatio(speedup), word: "faster" };
  if (speedup < 1) return { ratio: formatRatio(1 / speedup), word: "slower" };
  return { ratio: "", word: "About the same speed" };
}

function statFor(cell: BenchCell | undefined, metric: BenchMetric): BenchStat | null {
  return cell ? cell[metric] : null;
}

function markLabel(row: Row, method: BenchMethod, metric: BenchMetric): string {
  const cell = row.cells[method]!;
  const stat = statFor(cell, metric)!;
  const methodLabel = METHODS.find((m) => m.id === method)!.label;
  const range =
    stat.min === stat.max
      ? ""
      : ` (${formatMetric(metric, stat.min)} to ${formatMetric(metric, stat.max)})`;
  const estimate =
    metric === "cost_usd" && cell.cost_source === "estimate" ? ", estimated from tokens" : "";
  return `${harnessName(row.harness)}, ${row.label}, ${methodLabel}: ${formatMetric(metric, stat.median)}${range}${estimate}, ${cell.runs} runs`;
}

function Mark({ method }: { method: BenchMethod }) {
  return <span aria-hidden="true" {...stylex.props(styles.mark, markStyles[method])} />;
}

function Legend() {
  return (
    <ul {...stylex.props(styles.legend)} aria-label="Legend">
      {METHODS.map((method) => (
        <li key={method.id} {...stylex.props(styles.legendItem)}>
          <Mark method={method.id} />
          {method.label}
        </li>
      ))}
    </ul>
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

const LANE_Y = { raw: 12, cli: 28, mcp: 44 } satisfies Record<BenchMethod, number>;

function Chart({ rows, metric }: { rows: Row[]; metric: BenchMetric }) {
  const [hover, setHover] = useState<string | null>(null);
  const values = rows.flatMap((row) =>
    METHODS.map((m) => statFor(row.cells[m.id], metric)?.max ?? 0),
  );
  const max = metric === "correct_share" ? 1 : niceMax(Math.max(...values));
  const ticks = [0, 1, 2, 3, 4].map((i) => (max / 4) * i);
  const x = (value: number) => `${(value / max) * 100}%`;

  return (
    <div {...stylex.props(styles.chart)}>
      {rows.map((row) => (
        <div key={row.key} {...stylex.props(styles.row)}>
          <div {...stylex.props(styles.rowLabel)}>
            <span {...stylex.props(styles.rowHarness)}>{harnessName(row.harness)}</span>
            <span {...stylex.props(styles.rowModel)}>{row.label}</span>
          </div>
          <div {...stylex.props(styles.track)}>
            {ticks.map((tick) => (
              <span
                key={tick}
                aria-hidden="true"
                {...stylex.props(styles.gridLine, dynamic.left(x(tick)))}
              />
            ))}
            {METHODS.map(({ id }) => {
              const stat = statFor(row.cells[id], metric);
              if (!stat) return null;
              const markKey = `${row.key}:${id}`;
              const left = (stat.median / max) * 100;
              return (
                <div key={id}>
                  {stat.max > stat.min && (
                    <span
                      aria-hidden="true"
                      {...stylex.props(
                        styles.range,
                        rangeStyles[id],
                        dynamic.range(x(stat.min), x(stat.max - stat.min), LANE_Y[id]),
                      )}
                    />
                  )}
                  <button
                    type="button"
                    aria-label={markLabel(row, id, metric)}
                    onMouseEnter={() => setHover(markKey)}
                    onMouseLeave={() => setHover(null)}
                    onFocus={() => setHover(markKey)}
                    onBlur={() => setHover(null)}
                    {...stylex.props(styles.hit, dynamic.point(`${left}%`, LANE_Y[id]))}
                  >
                    <Mark method={id} />
                  </button>
                  {hover === markKey && (
                    <span
                      role="tooltip"
                      {...stylex.props(
                        styles.tooltip,
                        dynamic.point(`${left}%`, LANE_Y[id]),
                        left > 66 ? styles.tooltipLeft : left < 34 ? styles.tooltipRight : null,
                      )}
                    >
                      {markLabel(row, id, metric)}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      ))}
      <div {...stylex.props(styles.row, styles.axisRow)} aria-hidden="true">
        <span />
        <div {...stylex.props(styles.axis)}>
          {ticks.map((tick, index) => (
            <span
              key={tick}
              {...stylex.props(
                styles.tick,
                dynamic.left(x(tick)),
                index === 0 && styles.tickFirst,
                index === ticks.length - 1 && styles.tickLast,
              )}
            >
              {formatMetric(metric, tick)}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

function cellText(cell: BenchCell | undefined, metric: BenchMetric): string {
  const stat = statFor(cell, metric);
  if (!stat) return "–";
  const median = formatMetric(metric, stat.median);
  const estimate = metric === "cost_usd" && cell!.cost_source === "estimate" ? " est." : "";
  return stat.min === stat.max
    ? `${median}${estimate}`
    : `${median}${estimate} (${formatMetric(metric, stat.min)}–${formatMetric(metric, stat.max)})`;
}

function ResultsTable({ rows }: { rows: Row[] }) {
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
          {rows.flatMap((row) =>
            METHODS.filter((m) => row.cells[m.id]).map((method) => (
              <tr key={`${row.key}:${method.id}`}>
                <td {...stylex.props(styles.td)}>{harnessName(row.harness)}</td>
                <td {...stylex.props(styles.td)}>{row.label}</td>
                <td {...stylex.props(styles.td)}>{method.label}</td>
                {METRICS.map((metric) => (
                  <td key={metric.id} {...stylex.props(styles.td, styles.num)}>
                    {cellText(row.cells[method.id], metric.id)}
                  </td>
                ))}
                <td {...stylex.props(styles.td, styles.num)}>{row.cells[method.id]!.runs}</td>
              </tr>
            )),
          )}
        </tbody>
      </table>
    </div>
  );
}

const percent = (value: number | null) => (value === null ? "–" : `${Math.round(value * 100)}%`);

export function BenchmarkSection() {
  const tasks = TASKS.filter((t) => data.tasks[t.id]);
  const [task, setTask] = useState<BenchTask>(tasks[0]?.id ?? "usage");
  const [metric, setMetric] = useState<BenchMetric>("duration_s");
  const [view, setView] = useState<"chart" | "table">("chart");
  const rows = rowsFor(task);
  const summary = summarizeTask(data, task);
  const taskMeta = tasks.find((t) => t.id === task)!;
  const speed = speedWords(summary.speedup);
  const noResult = data.cells
    .filter((c) => c.task === task && !c.duration_s)
    .map(
      (c) =>
        `${harnessName(c.harness)} ${c.label} ${METHODS.find((m) => m.id === c.method)!.label.toLowerCase()}`,
    );
  const harnesses = new Set(data.cells.map((c) => c.harness)).size;
  const models = new Set(data.cells.map((c) => c.model)).size;
  const estimated = [
    ...new Set(
      data.cells.filter((c) => c.cost_source === "estimate").map((c) => harnessName(c.harness)),
    ),
  ];
  const runDate = new Date(data.generated).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        Ask your agent. With and without Skill Studio.
      </h2>
      <p {...stylex.props(styles.lede)}>
        We gave {harnesses} coding agents a made-up home folder with known answers, and asked about
        its skills. Each agent tried three ways: by hand, with the CLI, and with the MCP server.
      </p>

      <Segmented label="Task" options={tasks} value={task} onChange={setTask} />
      <p {...stylex.props(styles.question)}>{taskMeta.question}</p>

      <div {...stylex.props(styles.headline)}>
        <div>
          <p {...stylex.props(styles.hero)}>
            {speed.ratio}
            <span {...stylex.props(styles.heroWord)}>{speed.word}</span>
          </p>
          <p {...stylex.props(styles.heroCaption)}>
            Median time to a final answer with Skill Studio, compared with the same agent and model
            by hand. {summary.pairs} pairs.
          </p>
        </div>
        <dl {...stylex.props(styles.facts)}>
          <div {...stylex.props(styles.fact)}>
            <dt {...stylex.props(styles.factLabel)}>Cost</dt>
            <dd {...stylex.props(styles.factValue)}>
              {summary.costRatio === null
                ? "–"
                : summary.costRatio >= 1
                  ? `${formatRatio(summary.costRatio)} lower`
                  : `${formatRatio(1 / summary.costRatio)} higher`}
            </dd>
          </div>
          <div {...stylex.props(styles.fact)}>
            <dt {...stylex.props(styles.factLabel)}>Correct answers</dt>
            <dd {...stylex.props(styles.factValue)}>
              {percent(summary.correctByHand)} by hand, {percent(summary.correctWithStudio)} with
              Skill Studio
            </dd>
          </div>
          <div {...stylex.props(styles.fact)}>
            <dt {...stylex.props(styles.factLabel)}>Slower with Skill Studio</dt>
            <dd {...stylex.props(styles.factValue)}>
              {summary.slowerPairs} of {summary.pairs} pairs
            </dd>
          </div>
        </dl>
      </div>

      <div {...stylex.props(styles.controls)}>
        <Segmented label="Measure" options={METRICS} value={metric} onChange={setMetric} />
        <Legend />
        <Segmented
          label="View"
          options={[
            { id: "chart", label: "Chart" },
            { id: "table", label: "Table" },
          ]}
          value={view}
          onChange={setView}
        />
      </div>

      <figure {...stylex.props(styles.figure)}>
        {view === "chart" ? <Chart rows={rows} metric={metric} /> : <ResultsTable rows={rows} />}
        <figcaption {...stylex.props(styles.figcaption)}>
          Dot: median of the runs. Line: lowest to highest run.{" "}
          {METRICS.find((m) => m.id === metric)!.lowerIsBetter
            ? "Further left is better."
            : "Further right is better."}
          {noResult.length > 0 &&
            ` No result for ${noResult.join(", ")}: every run hit the 15-minute limit or failed.`}
        </figcaption>
      </figure>

      <div {...stylex.props(styles.method)}>
        <p {...stylex.props(styles.methodText)}>
          {data.total_runs} runs on {runDate}, ${data.total_cost_usd.toFixed(2)} of model use
          {estimated.length > 0 && ` (for ${estimated.join(" and ")}, estimated from tokens)`},{" "}
          {harnesses} agents and {models} models. Each run got a fresh copy of a made-up home
          folder, and a run that changed a file failed. Your numbers will differ.
        </p>
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
      </div>
    </>
  );
}

const MOBILE = "@media (max-width: 600px)";

const dynamic = stylex.create({
  left: (left: string) => ({ left }),
  point: (left: string, top: number) => ({ left, top }),
  range: (left: string, width: string, top: number) => ({ left, top, width }),
});

const markStyles = stylex.create({
  raw: {
    backgroundColor: siteTokens.background,
    borderColor: chartTokens.byHand,
    borderRadius: 999,
    borderStyle: "solid",
    borderWidth: 2,
  },
  cli: { backgroundColor: chartTokens.cli, borderRadius: 999 },
  mcp: { backgroundColor: chartTokens.mcp, borderRadius: 2, transform: "rotate(45deg) scale(.86)" },
});

const rangeStyles = stylex.create({
  raw: { backgroundColor: chartTokens.byHand },
  cli: { backgroundColor: chartTokens.cli },
  mcp: { backgroundColor: chartTokens.mcp },
});

const styles = stylex.create({
  title: { marginBottom: 14 },
  lede: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.6,
    margin: "0 0 32px",
    maxWidth: "60ch",
    textWrap: "pretty",
    [MOBILE]: { fontSize: 16 },
  },
  question: {
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.5,
    margin: "14px 0 0",
    maxWidth: "70ch",
    minHeight: "3em",
    textWrap: "pretty",
  },
  headline: {
    alignItems: "end",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    gap: 32,
    gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1.1fr)",
    paddingBlock: "24px 28px",
    "@media (max-width: 860px)": { gridTemplateColumns: "1fr", gap: 20 },
  },
  hero: {
    alignItems: "baseline",
    color: siteTokens.text,
    display: "flex",
    fontSize: "clamp(56px, 8vw, 96px)",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 720,
    gap: 14,
    letterSpacing: "-.045em",
    lineHeight: 1,
    margin: 0,
  },
  heroWord: { color: siteTokens.muted, fontSize: 22, fontWeight: 600, letterSpacing: "-.02em" },
  heroCaption: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.5,
    margin: "12px 0 0",
    maxWidth: "44ch",
    textWrap: "pretty",
  },
  facts: { display: "grid", gap: 10, margin: 0 },
  fact: {
    alignItems: "baseline",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 16,
    gridTemplateColumns: "minmax(0, 11em) minmax(0, 1fr)",
    paddingTop: 10,
    [MOBILE]: { gap: 2, gridTemplateColumns: "1fr" },
  },
  factLabel: { color: siteTokens.muted, fontSize: 13 },
  factValue: {
    color: siteTokens.text,
    fontSize: 15,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 600,
    margin: 0,
  },
  controls: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: "12px 20px",
    justifyContent: "space-between",
    paddingTop: 24,
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
  legend: {
    color: siteTokens.text,
    display: "flex",
    flexWrap: "wrap",
    fontSize: 13,
    gap: 16,
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  legendItem: { alignItems: "center", display: "flex", gap: 7 },
  mark: { display: "block", flexShrink: 0, height: 12, width: 12 },
  figure: { margin: "20px 0 0" },
  figcaption: { color: siteTokens.muted, fontSize: 12, marginTop: 12 },
  chart: { display: "flex", flexDirection: "column" },
  row: {
    alignItems: "center",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 20,
    gridTemplateColumns: "180px minmax(0, 1fr)",
    paddingBlock: 6,
    [MOBILE]: { gap: 2, gridTemplateColumns: "1fr", paddingBlock: "10px 4px" },
  },
  rowLabel: {
    display: "flex",
    flexDirection: "column",
    gap: 1,
    minWidth: 0,
    [MOBILE]: { flexDirection: "row", gap: 8 },
  },
  rowHarness: { color: siteTokens.text, fontSize: 14, fontWeight: 620 },
  rowModel: { color: siteTokens.muted, fontSize: 13 },
  track: { height: 56, marginInline: 8, position: "relative" },
  gridLine: {
    backgroundColor: chartTokens.grid,
    bottom: 0,
    position: "absolute",
    top: 0,
    width: 1,
  },
  range: {
    borderRadius: 2,
    height: 2,
    marginTop: -1,
    opacity: 0.55,
    position: "absolute",
  },
  hit: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 999,
    borderWidth: 0,
    cursor: "default",
    display: "flex",
    height: 24,
    justifyContent: "center",
    padding: 0,
    position: "absolute",
    transform: "translate(-50%, -50%)",
    width: 24,
    zIndex: 1,
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 0 },
  },
  tooltip: {
    backgroundColor: siteTokens.text,
    borderRadius: 8,
    boxShadow: "0 8px 24px oklch(0 0 0 / .25)",
    color: siteTokens.background,
    fontSize: 12,
    fontWeight: 560,
    lineHeight: 1.4,
    maxWidth: 260,
    padding: "7px 10px",
    pointerEvents: "none",
    position: "absolute",
    transform: "translate(-50%, calc(-100% - 12px))",
    width: "max-content",
    zIndex: 2,
  },
  tooltipLeft: { transform: "translate(calc(-100% + 12px), calc(-100% - 12px))" },
  tooltipRight: { transform: "translate(-12px, calc(-100% - 12px))" },
  axisRow: { paddingBlock: 0, [MOBILE]: { paddingBlock: 0 } },
  axis: { height: 24, marginInline: 8, position: "relative" },
  tick: {
    color: siteTokens.muted,
    fontSize: 11,
    fontVariantNumeric: "tabular-nums",
    position: "absolute",
    top: 6,
    transform: "translateX(-50%)",
    whiteSpace: "nowrap",
  },
  tickFirst: { transform: "none" },
  tickLast: { transform: "translateX(-100%)" },
  tableWrap: { overflowX: "auto" },
  table: { borderCollapse: "collapse", fontSize: 13, minWidth: 720, width: "100%" },
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
  method: {
    alignItems: "start",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 24,
    gridTemplateColumns: "minmax(0, 1fr) auto",
    marginTop: 28,
    paddingTop: 20,
    "@media (max-width: 760px)": { gap: 8, gridTemplateColumns: "1fr" },
  },
  methodText: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.6,
    margin: 0,
    maxWidth: "64ch",
    textWrap: "pretty",
  },
  links: { display: "flex", flexWrap: "wrap", gap: "0 20px" },
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
