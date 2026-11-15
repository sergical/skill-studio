// ============================================================================
// Skill Studio - Agent benchmark summary
// Reads the results file that tools/agent-bench writes and pairs each
// harness and model with itself, so a fast harness cannot flatter a method.
// ============================================================================

export type BenchTask = "usage" | "update" | "share";
export type BenchMethod = "raw" | "cli" | "mcp";
export type BenchMetric = "duration_s" | "cost_usd" | "correct_share" | "tool_calls";

export interface BenchStat {
  median: number;
  min: number;
  max: number;
}

export interface BenchCell {
  task: BenchTask;
  harness: string;
  model: string;
  label: string;
  method: BenchMethod;
  runs: number;
  errors: number;
  questions: number;
  correct_share: BenchStat | null;
  duration_s: BenchStat | null;
  cost_usd: BenchStat | null;
  cost_source: "harness" | "estimate";
  tool_calls: BenchStat | null;
}

export interface BenchData {
  generated: string;
  total_runs: number;
  total_cost_usd: number;
  tasks: Partial<Record<BenchTask, { runs: number; questions: string[] }>>;
  cells: BenchCell[];
}

export const HARNESS_NAMES: ReadonlyMap<string, string> = new Map([
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["opencode", "OpenCode"],
  ["pi", "pi"],
  ["cursor", "Cursor"],
]);

export function harnessName(id: string): string {
  return HARNESS_NAMES.get(id) ?? id;
}

export function rowKey(cell: Pick<BenchCell, "harness" | "model">): string {
  return `${cell.harness}/${cell.model}`;
}

export function cellsForTask(data: BenchData, task: BenchTask): BenchCell[] {
  return data.cells.filter((cell) => cell.task === task && cell.runs > cell.errors);
}

const MIN_SCORED_RUNS = 3;

export interface RacePair {
  key: string;
  harness: string;
  label: string;
  byHand: BenchCell;
  studio: BenchCell;
  speedup: number;
  callRatio: number;
  costRatio: number;
}

export interface RaceHeadline {
  speedup: RacePair;
  callRatio: RacePair;
  costRatio: RacePair;
  correctByHand: number;
  correctWithStudio: number;
}

const share = (cell: BenchCell) => cell.correct_share?.median ?? 0;
const seconds = (cell: BenchCell) => cell.duration_s?.median ?? Infinity;
const isScored = (cell: BenchCell) =>
  cell.runs - cell.errors >= MIN_SCORED_RUNS &&
  cell.duration_s !== null &&
  cell.tool_calls !== null &&
  cell.cost_usd !== null;

// Each harness and model against itself by hand, with its more correct Skill Studio method
// (the faster one on a tie). Pairs where Skill Studio gets fewer answers right are left out.
export function racePairs(data: BenchData, task: BenchTask): RacePair[] {
  const rows = new Map<string, Partial<Record<BenchMethod, BenchCell>>>();
  for (const cell of cellsForTask(data, task).filter(isScored)) {
    const row = rows.get(rowKey(cell)) ?? {};
    row[cell.method] = cell;
    rows.set(rowKey(cell), row);
  }
  const pairs: RacePair[] = [];
  for (const [key, row] of rows) {
    const byHand = row.raw;
    const studio = [row.mcp, row.cli]
      .filter((cell): cell is BenchCell => cell !== undefined)
      .sort((a, b) => share(b) - share(a) || seconds(a) - seconds(b))[0];
    if (!byHand || !studio || share(studio) < share(byHand)) continue;
    pairs.push({
      key,
      harness: byHand.harness,
      label: byHand.label,
      byHand,
      studio,
      speedup: byHand.duration_s!.median / studio.duration_s!.median,
      callRatio: byHand.tool_calls!.median / Math.max(studio.tool_calls!.median, 1),
      costRatio: byHand.cost_usd!.median / Math.max(studio.cost_usd!.median, 0.0001),
    });
  }
  return pairs.sort((a, b) => b.speedup - a.speedup);
}

const MIN_FEATURED_SPEEDUP = 1.25;

// One pair per harness, in harness order, when Skill Studio clearly wins it: the fastest pair
// with every answer right, or else the fastest pair.
export function featuredPairs(pairs: readonly RacePair[]): RacePair[] {
  const best = new Map<string, RacePair>();
  const ranked = [...pairs].sort(
    (a, b) => share(b.studio) - share(a.studio) || b.speedup - a.speedup,
  );
  for (const pair of ranked) {
    if (pair.speedup >= MIN_FEATURED_SPEEDUP && !best.has(pair.harness))
      best.set(pair.harness, pair);
  }
  const order = [...HARNESS_NAMES.keys()];
  return [...best.values()].sort((a, b) => order.indexOf(a.harness) - order.indexOf(b.harness));
}

export function raceHeadline(pairs: readonly RacePair[]): RaceHeadline | null {
  if (pairs.length === 0) return null;
  const mean = (values: number[]) => values.reduce((sum, v) => sum + v, 0) / values.length;
  const best = (ratio: (pair: RacePair) => number) =>
    pairs.reduce((top, pair) => (ratio(pair) > ratio(top) ? pair : top));
  return {
    speedup: best((p) => p.speedup),
    callRatio: best((p) => p.callRatio),
    costRatio: best((p) => p.costRatio),
    correctByHand: mean(pairs.map((p) => share(p.byHand))),
    correctWithStudio: mean(pairs.map((p) => share(p.studio))),
  };
}

export function formatMetric(metric: BenchMetric, value: number): string {
  switch (metric) {
    case "duration_s":
      return `${Math.round(value)} s`;
    case "cost_usd":
      return value < 0.1 ? `$${value.toFixed(3)}` : `$${value.toFixed(2)}`;
    case "correct_share":
      return `${Math.round(value * 100)}%`;
    case "tool_calls":
      return `${Math.round(value)}`;
  }
}

export function formatRatio(value: number): string {
  return `${value >= 10 ? Math.round(value) : value.toFixed(1)}×`;
}
