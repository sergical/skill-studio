// @vitest-environment happy-dom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { mockSkillSessionsReport } from "../../dev/harness/mock-sessions";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { useAppStore } from "../../store/appStore";
import { SessionsView } from "./SessionsView";

const initial = useAppStore.getState();
afterEach(() => {
  cleanup();
  useAppStore.setState(initial, true);
});

it("names only the sessions that recorded a skill list when it calls a skill unused", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockSkillSessionsReport(snapshot);
  const claude = report.agents.find((agent) => agent.harness === "claude-code")!;
  Object.assign(claude, { sessions_read: 50, sessions_with_listing: 3 });
  const idle = report.skills.find((skill) => skill.flags.includes("listed_never_used"))!;
  useAppStore.setState({ readSkillSessions: async () => report });
  render(
    <SessionsView
      initialHarness="claude-code"
      snapshot={{
        ...snapshot,
        invocations: snapshot.invocations.filter((stats) => stats.skill !== idle.name),
      }}
    />,
  );
  const findings = await screen.findByRole("list", { name: "Skills with findings" });
  const row = within(findings).getByRole("listitem", { name: idle.name });
  expect(row.textContent).toContain("3 sessions with a skill list");
  expect(row.textContent).not.toContain("50");
});
