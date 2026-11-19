// @vitest-environment happy-dom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { TrackedProjects } from "@skill-studio/lib";
import { loadStartupTrackedProjects } from "./legacy-project-paths";

const SAVED: TrackedProjects = { added: ["/work/saved"], excluded: [] };

let calls: { command: string; args: unknown }[] = [];

function backend(importFails: boolean) {
  mockIPC((command, args) => {
    calls.push({ command, args });
    if (command === "import_tracked_projects") {
      if (importFails) throw "fixture: disk full";
      return SAVED;
    }
    if (command === "get_tracked_projects") return SAVED;
    throw new Error(`unexpected ${command}`);
  });
}

beforeEach(() => {
  calls = [];
  localStorage.clear();
});

afterEach(() => {
  clearMocks();
});

it("imports an old machine's localStorage project lists into the backend, then clears them", async () => {
  // The format the old app wrote: one path per line, possibly with blank lines.
  localStorage.setItem("project-paths", "/work/one\n\n/work/two\n");
  localStorage.setItem("excluded-project-paths", "");
  backend(false);

  expect(await loadStartupTrackedProjects()).toEqual(SAVED);

  expect(calls).toEqual([
    {
      command: "import_tracked_projects",
      args: { added: ["/work/one", "/work/two"], excluded: [] },
    },
  ]);
  expect(localStorage.getItem("project-paths")).toBeNull();
  expect(localStorage.getItem("excluded-project-paths")).toBeNull();
});

it("imports an excluded-only old list too, since either key alone is a legacy machine", async () => {
  localStorage.setItem("excluded-project-paths", "/work/hidden");
  backend(false);

  await loadStartupTrackedProjects();

  expect(calls).toEqual([
    { command: "import_tracked_projects", args: { added: [], excluded: ["/work/hidden"] } },
  ]);
});

it("keeps the old lists when the import fails, so the next launch can try again", async () => {
  localStorage.setItem("project-paths", "/work/one");
  localStorage.setItem("excluded-project-paths", "/work/hidden");
  backend(true);

  await expect(loadStartupTrackedProjects()).rejects.toThrow("fixture: disk full");

  expect(localStorage.getItem("project-paths")).toBe("/work/one");
  expect(localStorage.getItem("excluded-project-paths")).toBe("/work/hidden");
});

it("reads the saved list and imports nothing on a machine with no old lists", async () => {
  backend(false);

  expect(await loadStartupTrackedProjects()).toEqual(SAVED);

  expect(calls.map((call) => call.command)).toEqual(["get_tracked_projects"]);
});
