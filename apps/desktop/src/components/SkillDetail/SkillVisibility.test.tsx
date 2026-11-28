// @vitest-environment happy-dom
import { cleanup, render, screen, within } from "@testing-library/react";
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

/** The Agents section's row for Codex. */
async function codexRow() {
  const list = await screen.findByRole("list", { name: "Agents" });
  return within(list)
    .getAllByRole("listitem")
    .find((item) => item.textContent?.startsWith("Codex"))!;
}

function showListing(report: ListingReport) {
  useAppStore.setState({
    readListing: async () => report,
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
  const row = await codexRow();
  expect(row.textContent).toContain("Full description");
  expect(row.textContent).toContain("Last session");
  // The estimate now differs from what the session showed; the opened row says so.
  expect(screen.getByText(/With the current settings/).textContent).toContain("Shortened");
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
  expect((await codexRow()).textContent).toContain("Name only");
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

it("describes the opened copy when two Codex copies of the skill are listed", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const first = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === first.path),
  )!;
  const second = deployment({
    agent: "Codex",
    scope: "global",
    path: `${HARNESS_HOME}/.agents/skills-second/${skill.name}`,
  });
  skill.deployments.push(second);
  Object.assign(first, { state: "name_only" });
  codex.entries.push({ ...structuredClone(first), path: second.path, state: "described" });
  showListing(report);
  render(<SkillVisibility skill={skill} deployment={second} listingScope="global" />);
  const row = await codexRow();
  expect(row.textContent).toContain("Full description");
  expect(row.textContent).not.toContain("Name only");
});

it("does not claim an agent with recent uses is missing from a list that failed to load", async () => {
  const snapshot = buildHarnessSnapshot();
  const skill = snapshot.skills[0]!;
  let read = false;
  useAppStore.setState({
    readListing: async () => {
      read = true;
      throw new Error("Skill list read failed");
    },
  });
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments[0]!}
      invocations={[
        {
          skill: skill.name,
          last_24_hours: 0,
          last_7_days: 3,
          last_14_days: 3,
          last_30_days: 3,
          last_used: null,
          by_project_30_days: {},
          by_day: {},
          by_harness_30_days: { codex: 3 },
          by_trigger_30_days: { user: 0, agent: 3, file_read: 0 },
          by_hour: [],
        },
      ]}
      listingScope="global"
    />,
  );
  await vi.waitFor(() => expect(read).toBe(true));
  await screen.findByText("Skill list read failed");
  const row = await codexRow();
  expect(row.textContent).toContain("3 uses");
  expect(row.textContent).not.toContain("Not in its list");
});

it("says the list was not read when a refresh fails after an earlier report", async () => {
  const snapshot = buildHarnessSnapshot();
  const skill = snapshot.skills[0]!;
  const cached = mockListingReport(snapshot, {});
  const paths = new Set(skill.deployments.map((copy) => copy.path));
  for (const row of cached.listings)
    row.entries = row.entries.filter((entry) => !entry.path || !paths.has(entry.path));
  let read = false;
  useAppStore.setState({
    listingReports: {
      [listingReportKey(currentListingRequest(null))]: { data: cached, refreshing: false },
    },
    readListing: async () => {
      read = true;
      throw new Error("Skill list read failed");
    },
  });
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments[0]!}
      invocations={[
        {
          skill: skill.name,
          last_24_hours: 0,
          last_7_days: 3,
          last_14_days: 3,
          last_30_days: 3,
          last_used: null,
          by_project_30_days: {},
          by_day: {},
          by_harness_30_days: { codex: 3 },
          by_trigger_30_days: { user: 0, agent: 3, file_read: 0 },
          by_hour: [],
        },
      ]}
      listingScope="global"
    />,
  );
  await vi.waitFor(() => expect(read).toBe(true));
  await screen.findByText("Skill list read failed");
  const row = await codexRow();
  expect(row.textContent).toContain("List not read");
  expect(row.textContent).not.toContain("Not in its list");
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
  });
  render(<SkillVisibility skill={skill} deployment={copy} listingScope="global" />);
  await vi.waitFor(() => expect(read).toBe(true));
  await vi.waitFor(() =>
    expect(useAppStore.getState().listingReports).not.toEqual(initial.listingReports),
  );
  expect(screen.queryByRole("region", { name: "Agents" })).toBeNull();
});

it("shows each agent's uses from the 30-day history, and no readable history for an agent with none", async () => {
  const snapshot = buildHarnessSnapshot();
  const report = mockListingReport(snapshot, {});
  const codex = report.listings.find((row) => row.harness === "codex")!;
  const entry = codex.entries.find((item) => item.path && item.scanned)!;
  const skill = snapshot.skills.find((item) =>
    item.deployments.some((copy) => copy.path === entry.path),
  )!;
  const other = snapshot.skills.find((item) => item.name !== skill.name)!;
  const stats = (name: string, byHarness: Record<string, number>, harness: string) => ({
    skill: name,
    last_24_hours: 0,
    last_7_days: 0,
    last_14_days: 0,
    last_30_days: 0,
    last_used: null,
    by_project_30_days: {},
    by_day: {},
    by_harness_30_days: byHarness,
    by_trigger_30_days: { user: 0, agent: 0, file_read: 0 },
    by_hour: [{ hour: 1, harness, trigger: "agent" as const, project_path: null, count: 1 }],
  });
  showListing(report);
  render(
    <SkillVisibility
      skill={skill}
      deployment={skill.deployments.find((item) => item.path === entry.path)!}
      invocations={[
        stats(skill.name, { codex: 4 }, "codex"),
        stats(other.name, { "claude-code": 2 }, "claude-code"),
      ]}
      listingScope="global"
    />,
  );
  expect((await codexRow()).textContent).toContain("4 uses");
  const list = screen.getByRole("list", { name: "Agents" });
  const rows = within(list).getAllByRole("listitem");
  const cursor = rows.find((item) => item.textContent?.startsWith("Cursor"));
  expect(cursor?.textContent).toContain("No readable history");
});
