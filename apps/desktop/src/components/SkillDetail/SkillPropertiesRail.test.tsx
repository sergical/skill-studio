// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { SkillPropertiesRail } from "./SkillPropertiesRail";
import { buildScopeGroups } from "./skill-location-status";
import { railHarnessEntries } from "./skill-properties-rail-model";

afterEach(cleanup);

it("a skill only in the Universal folder counts every agent that reads that folder, matching the rows its Agents popover lists", () => {
  const universalOnly = skill({
    name: "find-bugs",
    deployments: [
      deployment({ agent: "shared", scope: "global", path: "/fixture/.agents/skills/find-bugs" }),
    ],
  });
  const listed = railHarnessEntries(universalOnly, buildScopeGroups(universalOnly)).length;
  expect(
    listed,
    "the fixture must have several Universal readers to tell the counts apart",
  ).toBeGreaterThan(1);

  render(<SkillPropertiesRail skill={universalOnly} />);
  expect(screen.getByRole("button", { name: `Agents: ${listed} agents, edit` })).toBeTruthy();
});
