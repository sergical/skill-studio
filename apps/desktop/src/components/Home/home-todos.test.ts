import { describe, expect, it } from "vitest";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { homeTodos } from "./home-todos";

const skill = buildHarnessSnapshot().skills[0];
const issue = { kind: "broken-symlink" as const, skill, detail: "" };
const warning = { kind: "duplicate" as const, skill, detail: "" };

describe("homeTodos", () => {
  it("counts a skill with two broken links once, as the Skills filter does", () => {
    const [broken] = homeTodos(
      {
        broken: [issue, { ...issue, detail: "second link" }],
        warnings: [],
        updates: [],
        unused: [],
      },
      [],
      [],
      "Checked just now",
    );
    expect(broken.title).toBe("1 skill is broken");
  });

  it("lists warnings before updates, in the order Skills uses", () => {
    const ids = homeTodos(
      { broken: [], warnings: [warning], updates: [skill], unused: [] },
      [],
      [],
      "Checked just now",
    ).map((todo) => todo.id);
    expect(ids).toEqual(["warnings", "updates"]);
  });

  it("says no skill lost its description when Claude's model window is unknown", () => {
    const groups = { broken: [], warnings: [], updates: [], unused: [] };
    const claude = mockListingReport(buildHarnessSnapshot(), {}).listings.filter(
      (row) => row.harness === "claude-code",
    );
    expect(homeTodos(groups, claude, [], "").map((todo) => todo.id)).toEqual(["space-claude-code"]);
    const unknown = claude.map((row) => ({ ...row, uses_default_context_window: true }));
    expect(homeTodos(groups, unknown, [], "")).toEqual([]);
  });
});
