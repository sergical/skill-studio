// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Toaster } from "sonner";
import { afterEach, expect, it } from "vitest";
import { AppVersionCard } from "./AppVersionCard";

afterEach(async () => {
  cleanup();
  await Promise.resolve();
  clearMocks();
});

it("shows a failed initial status read in the card", async () => {
  mockIPC(
    (command) => {
      if (command === "app_version") return { version: "1.0", commit: "dev", notes: null };
      if (command === "get_update_status") throw new Error("Status unavailable");
      return null;
    },
    { shouldMockEvents: true },
  );
  render(<AppVersionCard />);
  expect(await screen.findByText("Status unavailable")).toBeTruthy();
});

it("shows a failed update check and lets the user retry", async () => {
  let checks = 0;
  mockIPC(
    (command) => {
      if (command === "app_version") return { version: "1.0", commit: "dev", notes: null };
      if (command === "get_update_status") return { status: "up-to-date" };
      if (command === "check_for_update") {
        if (++checks === 1) throw new Error("Update unavailable");
        return { status: "up-to-date" };
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  const user = userEvent.setup();
  render(<AppVersionCard />);
  const check = await screen.findByRole("button", { name: "Check for updates" });
  await user.click(check);
  expect(await screen.findByText("Update unavailable")).toBeTruthy();
  await user.click(check);
  await waitFor(() => expect(screen.getByText("Up to date")).toBeTruthy());
  expect(checks).toBe(2);
});

it("a failed update listener says so and keeps the update status, so Restart to update stays", async () => {
  mockIPC((command) => {
    if (command === "app_version") return { version: "1.0", commit: "dev", notes: null };
    if (command === "get_update_status") return { status: "ready-to-install", version: "2.0" };
    if (command === "plugin:event|listen") throw new Error("Listener unavailable");
    return null;
  });
  render(
    <>
      <AppVersionCard />
      <Toaster />
    </>,
  );
  expect(await screen.findByText("Couldn't follow update progress")).toBeTruthy();
  expect(screen.getByText("Listener unavailable")).toBeTruthy();
  expect(await screen.findByRole("button", { name: /Restart/ })).toBeTruthy();
});
