// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { HarnessListing, SkillInvocationStats } from "@skill-studio/lib";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { DontFit } from "./ListingView";

afterEach(cleanup);

const snapshot = buildHarnessSnapshot();
const report = mockListingReport(snapshot, {});
const row = (harness: string): HarnessListing => {
  const found = report.listings.find((r) => r.harness === harness);
  if (!found) throw new Error(`no ${harness} row`);
  const base = found.entries[0];
  return {
    ...found,
    entries: [
      { ...base, name: "alpha", path: "/alpha", state: "name_only" },
      { ...base, name: "beta", path: "/beta", state: "omitted" },
    ],
  };
};
const stats = (skill: string, byAgent: Record<string, number>): SkillInvocationStats => ({
  ...snapshot.invocations[0],
  skill,
  by_harness_30_days: byAgent,
  by_hour: [],
});
const dontFit = (listing: HarnessListing, invocations: SkillInvocationStats[]) =>
  render(<DontFit row={listing} invocations={invocations} open={() => {}} canOpen={() => false} />);

it("shows each skill's uses by this agent, and no readable history when the agent has none", () => {
  const { rerender } = dontFit(row("codex"), [
    stats("alpha", { "claude-code": 9, codex: 2 }),
    stats("beta", { "claude-code": 5 }),
  ]);
  const list = () => screen.getByRole("list", { name: "Skills that don’t fit" });
  expect(list().textContent).toContain("2 uses");
  expect(list().textContent).toContain("0 uses");
  expect(list().textContent).not.toContain("9 uses");

  rerender(
    <DontFit
      row={row("codex")}
      invocations={[stats("alpha", { "claude-code": 9 })]}
      open={() => {}}
      canOpen={() => false}
    />,
  );
  expect(screen.getByText("No readable history")).toBeTruthy();
  expect(list().textContent).not.toMatch(/\d+ uses?/);
});

it("lists nothing as not fitting when Claude's model window is unknown", () => {
  const { container } = dontFit({ ...row("claude-code"), uses_default_context_window: true }, []);
  expect(container.textContent).toBe("");
});

it("previews on hover the same prompt that Copy agent prompt copies", async () => {
  const written: string[] = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: async (text: string) => void written.push(text) },
  });
  dontFit(row("codex"), []);
  const copy = screen.getByRole("button", { name: "Copy agent prompt" });
  fireEvent.click(copy);
  await vi.waitFor(() => expect(written).toHaveLength(1));
  expect(written[0]).toContain("alpha, beta");
  expect(copy.getAttribute("title")).toBe(written[0]);
});
