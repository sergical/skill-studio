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

export interface TaskSummary {
  pairs: number;
  speedup: number | null;
  costRatio: number | null;
  slowerPairs: number;
  correctByHand: number | null;
  correctWithStudio: number | null;
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

function median(values: readonly number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

function ratio(byHand: BenchStat | null, withStudio: BenchStat | null): number | null {
  if (!byHand || !withStudio || withStudio.median <= 0) return null;
  return byHand.median / withStudio.median;
}

export function rowKey(cell: Pick<BenchCell, "harness" | "model">): string {
  return `${cell.harness}/${cell.model}`;
}

export function cellsForTask(data: BenchData, task: BenchTask): BenchCell[] {
  return data.cells.filter((cell) => cell.task === task && cell.runs > cell.errors);
}

export function summarizeTask(data: BenchData, task: BenchTask): TaskSummary {
  const cells = cellsForTask(data, task);
  const byHand = new Map(cells.filter((c) => c.method === "raw").map((c) => [rowKey(c), c]));
  const speedups: number[] = [];
  const costRatios: number[] = [];
  for (const cell of cells) {
    const raw = cell.method === "raw" ? undefined : byHand.get(rowKey(cell));
    if (!raw) continue;
    const speedup = ratio(raw.duration_s, cell.duration_s);
    if (speedup !== null) speedups.push(speedup);
    const cost = ratio(raw.cost_usd, cell.cost_usd);
    if (cost !== null) costRatios.push(cost);
  }
  const correct = (methods: BenchMethod[]) => {
    const shares = cells
      .filter((c) => methods.includes(c.method) && c.correct_share)
      .map((c) => c.correct_share!.median);
    return shares.length ? shares.reduce((sum, v) => sum + v, 0) / shares.length : null;
  };
  return {
    pairs: speedups.length,
    speedup: median(speedups),
    costRatio: median(costRatios),
    slowerPairs: speedups.filter((s) => s < 1).length,
    correctByHand: correct(["raw"]),
    correctWithStudio: correct(["cli", "mcp"]),
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
