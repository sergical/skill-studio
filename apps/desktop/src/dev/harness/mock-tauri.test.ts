// @vitest-environment happy-dom
// ============================================================================
// Skill Studio - mock-tauri tests
// The harness park must keep each copy's scope, as `park_layout.rs` does, or
// the e2e specs that park and unpark prove nothing about project copies.
// ============================================================================

import { invoke } from "@tauri-apps/api/core";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { installMockTauri } from "./mock-tauri";
import { buildHarnessSnapshot } from "./skill-fixture";

afterEach(() => {
  clearMocks();
});

it("parking commit's global and project copies keeps them apart, and unpark puts each back where it was", async () => {
  const harness = installMockTauri(buildHarnessSnapshot());
  const commit = () => harness.snapshot().skills.find((s) => s.name === "commit")!;
  const before = commit().deployments;
  const scopes = new Set(before.map((d) => d.scope));
  expect(scopes).toEqual(new Set(["global", "project"]));
  // An owner target names the skill, so park takes every folder of it.
  const target = { owner_id: "owner:v1/universal/commit" };

  await invoke("park_skill", { target });

  const parked = commit().deployments;
  expect(parked.every((d) => d.scope === "parked")).toBe(true);
  expect(new Set(parked.map((d) => d.id)).size).toBe(parked.length);
  expect(new Set(parked.map((d) => d.parked_origin?.scope))).toEqual(scopes);

  await invoke("unpark_skill", { target });

  const ids = (list: { id: string }[]) => list.map((d) => d.id).sort();
  expect(ids(commit().deployments)).toEqual(ids(before));
});
