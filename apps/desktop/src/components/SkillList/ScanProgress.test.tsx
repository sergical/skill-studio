// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { BackgroundStatus } from "@skill-studio/lib";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { SkillsView } from "./SkillsView";
import { SkillActivityView } from "../Activity/SkillActivityView";

const saved = buildHarnessSnapshot();

const scanning: BackgroundStatus = {
  revision: 1,
  tasks: [
    {
      worker: "scan",
      running: true,
      last_started_at: "2026-01-01T00:00:00Z",
      last_finished_at: null,
      last_error: null,
    },
  ],
};

describe("Inventory and Activity refresh", () => {
  afterEach(cleanup);
  it("inventory_shows_an_in_flight_note_with_its_saved_list_or_hides_a_running_scan", () => {
    render(
      <SkillsView snapshot={saved} active onSelectSkill={() => {}} backgroundStatus={scanning} />,
    );
    expect(screen.getByText(/Still scanning/).getAttribute("role")).toBe("status");
    expect(
      screen.getAllByRole("status").some((element) => element.textContent?.includes("skills")),
    ).toBe(true);
  });
  it("activity_shows_an_in_flight_note_with_its_saved_history_or_hides_a_running_scan", () => {
    render(
      <SkillActivityView snapshot={saved} onSelectSkill={() => {}} backgroundStatus={scanning} />,
    );
    expect(screen.getByText(/Still scanning/).getAttribute("role")).toBe("status");
    expect(screen.getAllByText(saved.invocations[0].skill).length).toBeGreaterThan(0);
  });
});
