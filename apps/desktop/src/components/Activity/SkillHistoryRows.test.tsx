// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { SkillEvent } from "@skill-studio/lib";
import { SkillHistorySection } from "./SkillHistorySection";

const kinds = [
  ["install", "Installed"],
  ["remove", "Removed"],
  ["update", "Updated"],
  ["park", "Parked"],
  ["unpark", "Unparked"],
  ["harness_disable", "Turned off"],
  ["harness_enable", "Turned on"],
  ["move_aside_disable", "Moved aside"],
  ["move_aside_restore", "Moved back"],
  ["invocation_change", "Invocation changed"],
  ["fork", "Forked"],
  ["unlink_harness", "Agent link removed"],
  ["relink_harness", "Agent link restored"],
  ["explode_shared_dir", "Whole-folder link converted"],
  ["materialize_then_disable", "Agent copy turned off"],
  ["reconcile_remove_stale_link", "Stale link removed"],
  ["repair_remove_link", "Broken link removed"],
  ["repair_relink_link", "Link repaired"],
  ["make_independent_copy", "Independent copy created"],
  ["repair_skill_frontmatter", "Frontmatter repaired"],
  ["restore", "Restored"],
  ["quarantine_prune", "Quarantine pruned"],
  ["split", "Split into agent copies"],
] as const;
function event(kind: string): SkillEvent {
  return {
    id: kind,
    ts: "2026-11-01T00:00:00Z",
    kind,
    skill: `skill-${kind}`,
    status: "done",
    restorable: false,
    force_restorable: false,
    harness: null,
    project_path: null,
    reverted_by: null,
    scope: "global",
    backup_path: "/fixture/backup",
  };
}
afterEach(() => {
  cleanup();
  clearMocks();
});

it("Activity names every action in the past tense with an action icon or exposes raw event names", async () => {
  const revealed: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "list_skill_events") return kinds.map(([kind]) => event(kind));
    if (command === "open_skill_path") {
      revealed.push(args);
      return;
    }
    throw new Error(`Unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<SkillHistorySection />);
  await screen.findByText("skill-install");
  for (const [, label] of kinds) {
    const row = screen.getByText(label).parentElement;
    expect(row?.querySelectorAll("svg").length, label).toBe(1);
    expect(row?.querySelector(".lucide-history"), label).toBeNull();
  }
  await user.click(screen.getAllByRole("button", { name: "Reveal in Finder" })[0]);
  expect(revealed).toEqual([{ path: "/fixture/backup", mode: "reveal" }]);
});

it("Activity explains how to reverse non-restorable actions or leaves their rows without a reason", async () => {
  mockIPC((command) =>
    command === "list_skill_events"
      ? ["park", "unpark", "quarantine_prune", "harness_disable"].map(event)
      : undefined,
  );
  const user = userEvent.setup();
  render(<SkillHistorySection />);
  expect(await screen.findByText("Unpark the skill to reverse this.")).toBeTruthy();
  expect(screen.getByText("Park the skill to reverse this.")).toBeTruthy();
  expect(screen.getByText("Pruned backups cannot be restored.")).toBeTruthy();
  expect(screen.getByText("This event has no saved change to restore.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  await user.click(screen.getAllByRole("button", { name: "Reveal in Finder" })[0]);
});

it("a failed Park row explains unavailable restore without promising Unpark or gives an action for a move that failed", async () => {
  mockIPC((command) => {
    if (command === "list_skill_events") return [{ ...event("park"), status: "failed" }];
    if (command === "open_skill_path") return;
    throw new Error(`Unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<SkillHistorySection />);
  expect(await screen.findByText("This event has no saved change to restore.")).toBeTruthy();
  expect(screen.queryByText("Unpark the skill to reverse this.")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Reveal in Finder" }));
});

it("Activity shows retained backup paths and unknown legacy sizes or hides old recovery bytes", async () => {
  mockIPC((command) =>
    command === "list_skill_events"
      ? [
          {
            ...event("park"),
            retained_copies: [
              { path: "/fixture/.park-quarantine-old", bytes: null },
              { path: "/fixture/.park-quarantine-new", bytes: 256 },
            ],
          },
        ]
      : undefined,
  );
  render(<SkillHistorySection />);
  expect(await screen.findByText(/\/fixture\/\.park-quarantine-old/)).toBeTruthy();
  expect(screen.getByText(/unknown bytes/)).toBeTruthy();
  expect(screen.getByText(/256 bytes/)).toBeTruthy();
});
