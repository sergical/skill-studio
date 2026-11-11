// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { ProjectFoldersCard } from "./ProjectFoldersCard";

afterEach(() => {
  cleanup();
  clearMocks();
});

it("a failed folder list says so in the card, or Settings shows 'Looking for project folders…' forever", async () => {
  mockIPC((command) => {
    if (command === "list_project_folders") throw new Error("fixture list failed");
    if (command === "get_discovery_sources") return [];
    throw new Error(`Unexpected command: ${command}`);
  });
  render(<ProjectFoldersCard snapshot={undefined} />);

  expect(await screen.findByText("Couldn't list project folders.")).toBeTruthy();
  expect(screen.queryByText("Looking for project folders…")).toBeNull();
});

it("a discovery settings failure names settings instead of the folder list", async () => {
  mockIPC((command) => {
    if (command === "list_project_folders") return [];
    if (command === "get_discovery_sources") throw new Error("settings failed");
    return null;
  });
  render(<ProjectFoldersCard snapshot={undefined} />);
  expect(await screen.findByText("Couldn't read discovery settings.")).toBeTruthy();
  expect(screen.queryByText("Couldn't list project folders.")).toBeNull();
});
