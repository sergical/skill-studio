import { describe, expect, it } from "vitest";

import { summarizeTask, type BenchCell, type BenchData } from "./benchmark-summary";

const stat = (median: number) => ({ median, min: median, max: median });

function cell(harness: string, method: BenchCell["method"], seconds: number, cost: number) {
  return {
    task: "usage",
    harness,
    model: `${harness}-model`,
    label: harness,
    method,
    runs: 3,
    errors: 0,
    questions: 4,
    correct_share: stat(0.75),
    duration_s: stat(seconds),
    cost_usd: stat(cost),
    tool_calls: stat(5),
  } satisfies BenchCell;
}

const data = (cells: BenchCell[]): BenchData => ({
  generated: "",
  total_runs: 0,
  total_cost_usd: 0,
  tasks: {},
  cells,
});

describe("agent benchmark headline", () => {
  it("compares each harness with itself, so a fast harness cannot lift the other method", () => {
    // A cross-harness median would compare the fast harness's tools with the slow one's raw run.
    const summary = summarizeTask(
      data([
        cell("fast", "raw", 10, 0.1),
        cell("fast", "mcp", 10, 0.1),
        cell("slow", "raw", 100, 1),
        cell("slow", "mcp", 100, 1),
      ]),
      "usage",
    );
    expect(summary.speedup).toBe(1);
    expect(summary.costRatio).toBe(1);
  });

  it("counts the pairs where Skill Studio was slower, so the page can say so", () => {
    const summary = summarizeTask(
      data([cell("a", "raw", 90, 0.3), cell("a", "cli", 30, 0.1), cell("a", "mcp", 120, 0.4)]),
      "usage",
    );
    expect(summary.pairs).toBe(2);
    expect(summary.slowerPairs).toBe(1);
  });

  it("skips a harness that has no by-hand run instead of inventing a ratio", () => {
    const summary = summarizeTask(data([cell("a", "mcp", 30, 0.1)]), "usage");
    expect(summary.pairs).toBe(0);
    expect(summary.speedup).toBeNull();
  });

  it("leaves out cells where every run failed", () => {
    const failed = { ...cell("a", "cli", 1, 0.01), errors: 3 };
    const summary = summarizeTask(data([cell("a", "raw", 60, 0.2), failed]), "usage");
    expect(summary.pairs).toBe(0);
  });
});
