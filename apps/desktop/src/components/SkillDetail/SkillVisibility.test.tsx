// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { InstalledSkill, ListingReport } from "@skill-studio/lib";
import { buildHarnessSnapshot, deployment, HARNESS_HOME } from "../../dev/harness/skill-fixture";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { listingReportKey, useAppStore } from "../../store/appStore";
import { currentListingRequest } from "../Listing/useSkillVisibility";
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
  // The harness estimate puts Codex in its shortened tier, which shares about 48 characters.
  expect(screen.getByText(/Codex keeps about/).textContent).toContain("48 characters");
});

function showListing(report: ListingReport) {
  useAppStore.setState({
    readListing: async () => report,
    readSkillSessions: async () => ({ skills: [], sessions: [], agents: [], history_days: 30 }),
  });
}

it("leads with what the last Codex session showed, not the estimate", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const entry = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === entry.path),
  )!;
  codex.observed_session = {
    transcript: "/rollout.jsonl",
    at: null,
    model: "gpt-5",
    listed: 1,
    described: 1,
  };
  Object.assign(entry, {
    state: "truncated",
    shown_description_chars: 12,
    full_description_chars: 30,
    description_preview: { shown: "Review staged", remainder: " changes first." },
    observed: "described",
    observed_description_chars: 30,
  });
  showListing(report);
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments.find((item) => item.path === entry.path)!}
      listingScope="global"
      initialClient="codex"
    />,
  );
  expect(await screen.findByText("Full description")).toBeTruthy();
  expect(screen.getByText(/Review staged changes first\./)).toBeTruthy();
  expect(screen.queryByText("Beyond the cutoff")).toBeNull();
});

it("shows no description for a skill the last session listed by name only", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const entry = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === entry.path),
  )!;
  codex.observed_session = {
    transcript: "/rollout.jsonl",
    at: null,
    model: "gpt-5",
    listed: 1,
    described: 0,
  };
  Object.assign(entry, {
    state: "described",
    full_description_chars: 30,
    description_preview: { shown: "Review staged changes first.", remainder: "" },
    observed: "name_only",
    observed_description_chars: null,
  });
  showListing(report);
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments.find((item) => item.path === entry.path)!}
      listingScope="global"
      initialClient="codex"
    />,
  );
  expect(await screen.findByText("Name only")).toBeTruthy();
  expect(screen.queryByText(/Review staged changes first\./)).toBeNull();
});

it("explains a Codex cut when the whole list fits but one description is over its fixed cap", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const entry = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === entry.path),
  )!;
  codex.description_char_limit = 1024;
  codex.thresholds = { ...codex.thresholds!, tier: "all_described" };
  Object.assign(entry, { state: "truncated", shown_description_chars: 1024 });
  showListing(report);
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments.find((item) => item.path === entry.path)!}
      listingScope="global"
      initialClient="codex"
    />,
  );
  expect((await screen.findByText(/Per-skill limit/)).textContent).toContain("1,024 characters");
  expect(screen.queryByText("skillListingMaxDescChars")).toBeNull();
});

function onlyCopy(name: string, copy: InstalledSkill["deployments"][number]) {
  const snapshot = buildHarnessSnapshot();
  const skill = snapshot.skills.find((item) => item.name === name)!;
  skill.deployments = [copy];
  return { snapshot, skill, copy };
}

it("shows nothing when no agent lists this skill", async () => {
  const { snapshot, skill, copy } = onlyCopy(
    "incident-postmortem",
    deployment({
      agent: "OpenCode",
      scope: "global",
      path: `${HARNESS_HOME}/.config/opencode/skills/incident-postmortem`,
    }),
  );
  const report = mockListingReport(snapshot, {});
  let read = false;
  useAppStore.setState({
    readListing: async () => {
      read = true;
      return report;
    },
    readSkillSessions: async () => ({ skills: [], sessions: [], agents: [], history_days: 30 }),
  });
  render(<SkillVisibility skill={skill} deployment={copy} listingScope="global" />);
  await vi.waitFor(() => expect(read).toBe(true));
  await vi.waitFor(() =>
    expect(useAppStore.getState().listingReports).not.toEqual(initial.listingReports),
  );
  expect(screen.queryByRole("region", { name: "What agents see of this skill" })).toBeNull();
});
