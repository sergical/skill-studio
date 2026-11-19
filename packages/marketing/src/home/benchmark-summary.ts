// ============================================================================
// Skill Studio - Agent benchmark summary
// Reads the results file that tools/agent-bench writes and compares each
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
  /** Runs that changed the fixture. They are not scored, as runs with an error are not. */
  fixture_changed: number;
  questions: number;
  /** Share of the scored runs that got each question right, by question id. */
  per_question: Partial<Record<string, number | null>>;
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

export interface BenchSide {
  /** Median over the pairs of each pair's median. */
  seconds: number;
  toolCalls: number;
  /** One answer per question and scored run, over all pairs. */
  answers: number;
  wrong: number;
}

export interface BenchSummary {
  pairs: number;
  byHand: BenchSide;
  studio: BenchSide;
}

// A run can both fail and change the fixture, so this can count too few scored runs, never too many.
const scoredRuns = (cell: BenchCell) => cell.runs - cell.errors - cell.fixture_changed;
const isScored = (cell: BenchCell) =>
  scoredRuns(cell) >= MIN_SCORED_RUNS && cell.duration_s !== null && cell.tool_calls !== null;

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

function side(cells: BenchCell[]): BenchSide {
  let answers = 0;
  let wrong = 0;
  for (const cell of cells) {
    const runs = scoredRuns(cell);
    for (const share of Object.values(cell.per_question)) {
      answers += runs;
      wrong += runs - Math.round((share ?? 0) * runs);
    }
  }
  return {
    seconds: median(cells.map((cell) => cell.duration_s!.median)),
    toolCalls: median(cells.map((cell) => cell.tool_calls!.median)),
    answers,
    wrong,
  };
}

// Each harness and model by hand against itself with the Skill Studio MCP server. A pair counts
// only when both sides have three scored runs, so both sides cover the same agents and models.
export function benchSummary(data: BenchData, task: BenchTask): BenchSummary | null {
  const rows = new Map<string, Partial<Record<BenchMethod, BenchCell>>>();
  for (const cell of cellsForTask(data, task).filter(isScored)) {
    const row = rows.get(rowKey(cell)) ?? {};
    row[cell.method] = cell;
    rows.set(rowKey(cell), row);
  }
  const byHand: BenchCell[] = [];
  const studio: BenchCell[] = [];
  for (const row of rows.values()) {
    if (!row.raw || !row.mcp) continue;
    byHand.push(row.raw);
    studio.push(row.mcp);
  }
  if (byHand.length === 0) return null;
  return { pairs: byHand.length, byHand: side(byHand), studio: side(studio) };
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
