// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { SkillEvent } from "@skill-studio/lib";
import { SkillHistorySection } from "./SkillHistorySection";

afterEach(() => {
  cleanup();
  clearMocks();
});

it("history shows the expired backup reason and hides Restore or offers a missing backup", async () => {
  const reason = "Backup expired. Undo is not available.";
  const expired: SkillEvent = {
    id: "expired",
    ts: "2026-09-01T00:00:00Z",
    kind: "update",
    skill: "alpha",
    status: "done",
    harness: null,
    scope: "global",
    project_path: null,
    restorable: false,
    force_restorable: false,
    reverted_by: null,
    restore_unavailable_reason: reason,
  };
  mockIPC((command) => (command === "list_skill_events" ? [expired] : null));
  render(<SkillHistorySection />);
  await screen.findByText(reason);
  expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
});
