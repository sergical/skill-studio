import { describe, expect, it } from "vitest";

import { benchSummary, type BenchCell, type BenchData } from "./benchmark-summary";

const stat = (median: number) => ({ median, min: median, max: median });

function cell(model: string, method: BenchCell["method"], seconds: number, toolCalls: number) {
  return {
    task: "usage",
    harness: "codex",
    model,
    label: model,
    method,
    runs: 3,
    errors: 0,
    fixture_changed: 0,
    questions: 2,
    per_question: { q1: 1, q2: 1 },
    cost_source: "harness",
    correct_share: stat(1),
    duration_s: stat(seconds),
    cost_usd: stat(0.1),
    tool_calls: stat(toolCalls),
  } satisfies BenchCell;
}

const data = (cells: BenchCell[]): BenchData => ({
  generated: "",
  total_runs: 0,
  total_cost_usd: 0,
  tasks: {},
  cells,
});

describe("agent benchmark summary", () => {
  it("takes the median of each side over the pairs", () => {
    const summary = benchSummary(
      data([
        cell("a", "raw", 90, 20),
        cell("a", "mcp", 10, 2),
        cell("b", "raw", 60, 10),
        cell("b", "mcp", 20, 3),
        cell("c", "raw", 30, 12),
        cell("c", "mcp", 40, 8),
      ]),
      "usage",
    );
    expect(summary).toMatchObject({
      pairs: 3,
      byHand: { seconds: 60, toolCalls: 12 },
      studio: { seconds: 20, toolCalls: 3 },
    });
  });

  it("takes the middle of the two middle pairs when the count is even", () => {
    const summary = benchSummary(
      data([
        cell("a", "raw", 90, 7),
        cell("a", "mcp", 10, 2),
        cell("b", "raw", 60, 8),
        cell("b", "mcp", 20, 5),
      ]),
      "usage",
    );
    expect(summary?.byHand).toMatchObject({ seconds: 75, toolCalls: 7.5 });
  });

  it("counts one answer per question and scored run, and the wrong ones", () => {
    const summary = benchSummary(
      data([
        { ...cell("a", "raw", 90, 20), per_question: { q1: 1, q2: 1 / 3 } },
        cell("a", "mcp", 10, 2),
      ]),
      "usage",
    );
    expect(summary?.byHand).toMatchObject({ answers: 6, wrong: 2 });
    expect(summary?.studio).toMatchObject({ answers: 6, wrong: 0 });
  });

  it("leaves out a pair when one side has fewer than three scored runs", () => {
    const summary = benchSummary(
      data([
        { ...cell("a", "raw", 900, 99), errors: 2 },
        cell("a", "mcp", 10, 2),
        cell("b", "raw", 60, 10),
        cell("b", "mcp", 20, 3),
      ]),
      "usage",
    );
    expect(summary).toMatchObject({ pairs: 1, byHand: { seconds: 60 }, studio: { seconds: 20 } });
  });

  it("does not score a run that changed the fixture", () => {
    const summary = benchSummary(
      data([
        { ...cell("a", "raw", 900, 99), errors: 1, fixture_changed: 1 },
        cell("a", "mcp", 10, 2),
        { ...cell("b", "raw", 60, 10), runs: 4, fixture_changed: 1 },
        cell("b", "mcp", 20, 3),
      ]),
      "usage",
    );
    expect(summary).toMatchObject({ pairs: 1, byHand: { seconds: 60, answers: 6 } });
  });

  it("compares by hand with the MCP server, not the CLI", () => {
    const summary = benchSummary(
      data([cell("a", "raw", 60, 10), cell("a", "cli", 5, 1), cell("a", "mcp", 20, 3)]),
      "usage",
    );
    expect(summary?.studio).toMatchObject({ seconds: 20, toolCalls: 3 });
  });

  it("has no summary when no pair has both sides", () => {
    expect(
      benchSummary(data([cell("a", "raw", 60, 10), cell("a", "cli", 5, 1)]), "usage"),
    ).toBeNull();
  });
});
