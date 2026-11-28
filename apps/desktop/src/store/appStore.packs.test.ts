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
  useAppStore.getState().setHarnessDetection({ choice: null });
});

function mockPackAdd(calls: unknown[], detected: string[]) {
  mockIPC((command, args) => {
    if (command === "get_add_method_defaults") {
      return {
        dotagents_installed: true,
        has_skill_lock: true,
        installed_harnesses: detected,
        claude_reads_shared_folder: false,
      };
    }
    if (command !== "pack_add") return null;
    calls.push(args);
    return { dry_run: false, steps: [], summary: "" };
  });
}

it("adding a pinned pack's new skills sends the pack's pin, so they install from the ref the list checked", async () => {
  const calls: unknown[] = [];
  mockPackAdd(calls, []);

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

it("adding a pack installs for the agents the Add Skill sheet ticks, else a pack reaches other agents than one skill", async () => {
  const calls: unknown[] = [];
  mockPackAdd(calls, ["codex", "pi"]);
  useAppStore.getState().setHarnessDetection({
    choice: { kept: ["cursor"], search_project_folders: false, saved_at: "2026-11-17T00:00:00Z" },
  });

  await useAppStore
    .getState()
    .runPackAction(
      { kind: "add", source: "acme/skills", skills: ["new-skill"], refPin: null },
      false,
    );

  expect(calls).toEqual([
    {
      request: expect.objectContaining({ harnesses: ["cursor"] }),
    },
  ]);
});
