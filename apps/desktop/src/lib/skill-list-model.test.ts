// ============================================================================
// skill-list-model tests
// Guards that grouping skills into the state buckets accounts for every skill
// exactly once, in priority order, before the virtualizer trims what's painted.
// ============================================================================

import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { attentionBySkill, collectDashboardIssues } from "@skill-studio/lib";
import { buildHarnessSnapshot } from "../dev/harness/skill-fixture";
import { ROW_GROUP_ORDER } from "../components/SkillList/skill-row-state";
import { groupSkillRows } from "./skill-list-model";

const snapshot = buildHarnessSnapshot(400);

describe("groupSkillRows", () => {
  it("puts every skill in exactly one state bucket, and rows lists the buckets most serious first", () => {
    fc.assert(
      fc.property(
        fc.subarray(snapshot.skills),
        fc.constantFrom("name", "used", "size"),
        (skills, sort) => {
          const attention = attentionBySkill(skills, collectDashboardIssues(skills));
          const { buckets, rows } = groupSkillRows(skills, sort, snapshot.invocations, attention);
          const bucketed = ROW_GROUP_ORDER.flatMap((group) => buckets[group]);
          expect(rows).toEqual(bucketed);
          expect([...bucketed].sort(byName)).toEqual([...skills].sort(byName));
        },
      ),
    );
  });
});

function byName(a: { name: string }, b: { name: string }): number {
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}
