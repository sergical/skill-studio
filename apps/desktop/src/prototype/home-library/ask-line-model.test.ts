import { expect, it } from "vitest";
import type { HarnessListing, ListingEntry } from "@skill-studio/lib";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { agentCell, cutLine } from "./ask-line-model";

const report = mockListingReport(buildHarnessSnapshot(), {});
const listing = (harness: string) => {
  const row = report.listings.find((r) => r.harness === harness);
  if (!row) throw new Error(`no ${harness} row`);
  return row;
};
const entries = (row: HarnessListing, states: ListingEntry["state"][]) =>
  states.map((state, i) => ({ ...row.entries[0], name: `skill-${i}`, path: `/s/${i}`, state }));

it("counts only the skills an agent's list holds, and says what a count without an estimate is", () => {
  const codex = listing("codex");
  const cell = agentCell(
    { id: "codex", label: "Codex" },
    {
      ...codex,
      entries: entries(codex, ["described", "described", "omitted", "omitted", "omitted"]),
    },
    [],
    undefined,
  );
  expect(cell).toMatchObject({ sees: 2, seesUnit: "in skill list", uses: null });
  expect(cell.notListed).toHaveLength(3);

  const cursor = agentCell({ id: "cursor", label: "Cursor" }, listing("cursor"), [], undefined);
  expect(cursor.seesUnit).toBe("found in folders it reads");
  expect(cutLine(cursor)).toBeNull();
});

it("makes no cut claim or park advice when Claude's model window is unknown", () => {
  const claude = listing("claude-code");
  const cell = agentCell(
    { id: "claude-code", label: "Claude Code" },
    {
      ...claude,
      uses_default_context_window: true,
      entries: entries(claude, ["described", "name_only", "omitted"]),
    },
    [],
    undefined,
  );
  expect(cell).toMatchObject({ sees: 3, nameOnly: null, notListed: null });
  expect(cell.seesUnit).toBe("found, model window unknown");
  expect(cutLine(cell)).toBeNull();
});
