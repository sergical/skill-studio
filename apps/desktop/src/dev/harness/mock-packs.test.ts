// ============================================================================
// Skill Studio - mock-packs tests
// The harness pack list must group the way `pack_list` in the core does.
// ============================================================================

import { describe, expect, it } from "vitest";
import { mockPackReport } from "./mock-packs";
import { buildHarnessSnapshot, deployment, HARNESS_HOME, skill } from "./skill-fixture";

describe("mockPackReport", () => {
  it("leaves out skills the scan marks manual, as pack_list does", () => {
    const snapshot = buildHarnessSnapshot();
    const manual = snapshot.skills.filter(
      (s) => s.source === "manual" && s.source_kind === "dotagents" && !s.parked,
    );
    expect(manual.length).toBeGreaterThan(0);

    const packs = mockPackReport(snapshot, false).packs;
    expect(packs.map((p) => p.source)).not.toContain("manual");
    const names = packs.flatMap((p) => p.skills.map((s) => s.name));
    for (const skill of manual) expect(names).not.toContain(skill.name);
    expect(packs.some((p) => p.kind === "plugin")).toBe(true);
  });

  it("leaves out a skills.sh install from a local folder, as pack_list does", () => {
    const snapshot = buildHarnessSnapshot();
    const local = skill({
      name: "local-one",
      source: "vendor/local-one",
      source_type: "local",
      source_kind: "skills-sh",
      deployments: [
        deployment({
          agent: "shared",
          scope: "global",
          path: `${HARNESS_HOME}/.agents/skills/local-one`,
          owner_kind: "skills-sh",
          content_hash: "local-one-v1",
        }),
      ],
    });
    const packs = mockPackReport({ ...snapshot, skills: [...snapshot.skills, local] }, false).packs;
    expect(packs.map((p) => p.source)).not.toContain("vendor/local-one");
  });
});
