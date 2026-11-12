// @vitest-environment happy-dom

import userEvent from "@testing-library/user-event";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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

it("removes an added folder only after confirmation and leaves it tracked on cancel", async () => {
  let removals = 0;
  mockIPC((command) => {
    if (command === "list_project_folders")
      return removals ? [] : [{ path: "/project", source: "added", missing: false, matches: null }];
    if (command === "get_discovery_sources") return [];
    if (command === "remove_skill_project") {
      removals++;
      return [];
    }
    return null;
  });
  const user = userEvent.setup();
  render(<ProjectFoldersCard snapshot={undefined} />);
  await user.click(await screen.findByRole("button", { name: /1 folder/ }));
  await user.click(await screen.findByRole("button", { name: "Remove /project" }));
  expect(removals).toBe(0);
  const dialog = await screen.findByRole("alertdialog");
  expect(within(dialog).getByText(/does not delete the folder/)).toBeTruthy();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(removals).toBe(0);
  await user.click(screen.getByRole("button", { name: "Remove /project" }));
  await user.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }),
  );
  await waitFor(() => expect(removals).toBe(1));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
});

it("stops tracking a discovered folder only after confirmation and keeps it tracked on cancel", async () => {
  let stops = 0;
  mockIPC((command) => {
    if (command === "list_project_folders")
      return stops
        ? []
        : [{ path: "/project", source: "discovered", missing: false, matches: null }];
    if (command === "get_discovery_sources") return [];
    if (command === "unregister_skill_project") {
      stops++;
      return [];
    }
    return null;
  });
  const user = userEvent.setup();
  render(<ProjectFoldersCard snapshot={undefined} />);
  await user.click(await screen.findByRole("button", { name: /1 folder/ }));
  await user.click(await screen.findByRole("button", { name: "Stop tracking /project" }));
  const dialog = await screen.findByRole("alertdialog");
  expect(within(dialog).getByText(/skills stay on disk/)).toBeTruthy();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(stops).toBe(0);
  await user.click(screen.getByRole("button", { name: "Stop tracking /project" }));
  await user.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Stop tracking" }),
  );
  await waitFor(() => expect(stops).toBe(1));
});
