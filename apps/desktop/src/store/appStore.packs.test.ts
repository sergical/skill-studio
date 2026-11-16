// @vitest-environment happy-dom
// ============================================================================
// Skill Studio - appStore pack action tests
// The request each pack intent sends to the core's pack commands.
// ============================================================================

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { useAppStore } from "./appStore";

afterEach(() => {
  clearMocks();
});

it("adding a pinned pack's new skills sends the pack's pin, so they install from the ref the list checked", async () => {
  const calls: unknown[] = [];
  mockIPC((command, args) => {
    if (command !== "pack_add") return null;
    calls.push(args);
    return { dry_run: true, steps: [], summary: "" };
  });

  await useAppStore
    .getState()
    .runPackAction(
      { kind: "add", source: "acme/skills", skills: ["new-skill"], refPin: "develop" },
      true,
    );

  expect(calls).toEqual([
    {
      request: expect.objectContaining({
        source: "acme/skills",
        skills: ["new-skill"],
        ref_pin: "develop",
        dry_run: true,
      }),
    },
  ]);
});
