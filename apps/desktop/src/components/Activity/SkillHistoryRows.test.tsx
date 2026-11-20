// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { SkillEvent } from "@skill-studio/lib";
import { SkillHistorySection } from "./SkillHistorySection";

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

it("Reveal in Finder on an Activity row opens that event's backup folder", async () => {
  const revealed: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "list_skill_events") return [event("install")];
    if (command === "open_skill_path") {
      revealed.push(args);
      return;
    }
    throw new Error(`Unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<SkillHistorySection />);
  await screen.findByText("skill-install");
  await user.click(screen.getByRole("button", { name: "Reveal in Finder" }));
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
