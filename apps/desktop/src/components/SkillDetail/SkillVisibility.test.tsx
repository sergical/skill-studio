// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { listingReportKey, useAppStore } from "../../store/appStore";
import { currentListingRequest } from "../../hooks/useStoreReports";
import { SkillVisibility } from "./SkillVisibility";

const initial = useAppStore.getState();
afterEach(() => {
  cleanup();
  useAppStore.setState(initial, true);
});

it("refreshes a cached listing and shows where the current description was cut", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const entry = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === entry.path),
  )!;
  const deployment = skill.deployments.find((item) => item.path === entry.path)!;
  const cached = structuredClone(report);
  Object.assign(entry, {
    state: "truncated",
    description_preview: { shown: "Review staged changes...", remainder: " before a release." },
  });
  useAppStore.setState({
    listingReports: {
      [listingReportKey(currentListingRequest(null))]: { data: cached, refreshing: false },
    },
    readListing: async () => report,
    readSkillSessions: async () => ({ skills: [], sessions: [], agents: [], history_days: 30 }),
  });
  render(
    <SkillVisibility
      skill={skill}
      deployment={deployment}
      listingScope="global"
      initialClient="codex"
    />,
  );
  expect(await screen.findByText(/Review staged changes\.\.\./)).toBeTruthy();
  expect(screen.getByText("Beyond the cutoff").parentElement?.textContent).toContain(
    "before a release.",
  );
});
