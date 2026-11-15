import { describe, expect, it } from "vitest";

import {
  featuredPairs,
  raceHeadline,
  racePairs,
  type BenchCell,
  type BenchData,
} from "./benchmark-summary";

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
    cost_source: "harness",
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

describe("agent benchmark race", () => {
  const right = (c: BenchCell, correct: number) => ({ ...c, correct_share: stat(correct) });

  it("races each harness and model against itself by hand", () => {
    const pairs = racePairs(
      data([
        cell("codex", "raw", 90, 0.3),
        cell("codex", "mcp", 30, 0.1),
        cell("claude", "raw", 20, 0.1),
        cell("claude", "mcp", 10, 0.05),
      ]),
      "usage",
    );
    expect(pairs.map((p) => [p.harness, p.speedup])).toEqual([
      ["codex", 3],
      ["claude", 2],
    ]);
  });

  it("picks the more correct Skill Studio method, then the faster one", () => {
    const [pair] = racePairs(
      data([
        cell("a", "raw", 90, 0.3),
        right(cell("a", "mcp", 20, 0.1), 0.5),
        right(cell("a", "cli", 40, 0.1), 0.75),
      ]),
      "usage",
    );
    expect(pair.studio.method).toBe("cli");
  });

  it("keeps a pair where Skill Studio gets fewer answers right out of the race and the ratios, but in the correct shares", () => {
    const pairs = racePairs(
      data([
        right(cell("a", "raw", 90, 0.3), 1),
        right(cell("a", "mcp", 5, 0.01), 0.5),
        right(cell("b", "raw", 20, 0.1), 1),
        right(cell("b", "mcp", 10, 0.1), 1),
      ]),
      "usage",
    );
    expect(featuredPairs(pairs).map((p) => p.key)).toEqual(["b/b-model"]);
    expect(raceHeadline(pairs)?.speedup.key).toBe("b/b-model");
    expect(raceHeadline(pairs)).toMatchObject({ correctByHand: 1, correctWithStudio: 0.75 });
  });

  it("features the fastest pair of each harness, in harness order", () => {
    const pairs = racePairs(
      data([
        cell("codex", "raw", 90, 0.3),
        cell("codex", "mcp", 30, 0.1),
        { ...cell("claude", "raw", 40, 0.1), model: "m1" },
        { ...cell("claude", "mcp", 20, 0.1), model: "m1" },
        { ...cell("claude", "raw", 80, 0.1), model: "m2" },
        { ...cell("claude", "mcp", 10, 0.1), model: "m2" },
      ]),
      "usage",
    );
    expect(featuredPairs(pairs).map((p) => p.key)).toEqual(["claude/m2", "codex/codex-model"]);
  });

  it("features a pair with every answer right over a faster pair with a miss", () => {
    const pairs = racePairs(
      data([
        { ...right(cell("a", "raw", 90, 0.1), 0.5), model: "fast" },
        { ...right(cell("a", "mcp", 10, 0.1), 0.75), model: "fast" },
        { ...right(cell("a", "raw", 40, 0.1), 1), model: "sure" },
        { ...right(cell("a", "mcp", 20, 0.1), 1), model: "sure" },
      ]),
      "usage",
    );
    expect(featuredPairs(pairs).map((p) => p.key)).toEqual(["a/sure"]);
  });

  it("features no pair that Skill Studio does not clearly win", () => {
    const pairs = racePairs(data([cell("a", "raw", 22, 0.1), cell("a", "mcp", 20, 0.1)]), "usage");
    expect(featuredPairs(pairs)).toEqual([]);
  });

  it("leads with the best pair and averages correct answers over every pair", () => {
    const headline = raceHeadline(
      racePairs(
        data([
          right(cell("a", "raw", 90, 0.3), 0.5),
          right(cell("a", "mcp", 10, 0.1), 1),
          right(cell("b", "raw", 20, 0.1), 1),
          right(cell("b", "mcp", 10, 0.1), 1),
        ]),
        "usage",
      ),
    );
    expect(headline?.speedup.speedup).toBe(9);
    expect(headline).toMatchObject({ correctByHand: 0.75, correctWithStudio: 1 });
  });
});
