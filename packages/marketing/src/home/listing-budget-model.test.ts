import { expect, it } from "vitest";

import {
  allocateClaudeDescriptions,
  allocateCodexDescriptions,
  claudeBudgetChars,
  codexListingBudget,
} from "./listing-budget-model";

it("matches fixed listing vectors from the released CLI and Rust allocators", () => {
  // Released `listing --home <fixture>`: three 80-character skills, a 200k
  // Claude fallback at 0.01%, and two Codex context settings.
  expect(claudeBudgetChars(200_000, 4, 0.01)).toBe(80);
  for (const [window, limit] of [
    [272_000, 5_440],
    [500_000, 10_000],
  ] as const) {
    expect(codexListingBudget(window, null)).toEqual({ limit, unit: "tokens" });
  }
  expect(codexListingBudget(null, null)).toEqual({ limit: 8_000, unit: "characters" });
  expect(codexListingBudget(272_000, 14_000)).toEqual({ limit: 10_000, unit: "tokens" });
  expect(claudeBudgetChars(1_000_000, 3, undefined, 150_000)).toBe(100_000);
  expect(claudeBudgetChars(1_000_000, 3, 1, 150_000)).toBe(30_000);

  // listing_visibility.rs: fixed lines survive, and usage order can skip a
  // long line and then use the remaining room on a shorter one.
  const claudeLine = (fullWidth: number, score = 0, fixed = false) => ({
    nameWidth: 5,
    fullWidth,
    score,
    fixed,
  });
  for (const { lines, budget, kept } of [
    { lines: [claudeLine(20), claudeLine(30)], budget: 51, kept: [true, true] },
    {
      lines: [claudeLine(40), claudeLine(40, 3), claudeLine(10)],
      budget: 60,
      kept: [false, true, true],
    },
    { lines: [claudeLine(50, 0, true), claudeLine(40, 9)], budget: 10, kept: [true, false] },
  ]) {
    expect(allocateClaudeDescriptions(lines, budget)).toEqual(kept);
  }

  const codexLine = (name: string, description: string) => ({
    name,
    description,
    locator: "/s/SKILL.md",
  });
  for (const { lines, budget, kept } of [
    {
      lines: [codexLine("a", "x".repeat(80)), codexLine("b", "y".repeat(80))],
      budget: 172,
      kept: [60, 60],
    },
    {
      lines: [codexLine("a", "x"), codexLine("b", "y"), codexLine("c", "z")],
      budget: 50,
      kept: [0, 0, null],
    },
  ]) {
    expect(allocateCodexDescriptions(lines, { limit: budget, unit: "characters" }).kept).toEqual(
      kept,
    );
  }
});
