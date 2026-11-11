// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { AppErrorBoundary } from "./AppErrorBoundary";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function BrokenView(): never {
  throw new Error("snapshot field missing");
}

it("a render error offers Reload and names the error, or the user's only way out is quitting the app", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const reload = vi.fn();
  vi.spyOn(window, "location", "get").mockReturnValue({ ...window.location, reload });
  render(
    <AppErrorBoundary>
      <BrokenView />
    </AppErrorBoundary>,
  );

  expect(screen.getByRole("alert").textContent).toContain("snapshot field missing");
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Reload Skill Studio" }));
  await userEvent.setup().keyboard("{Enter}");
  expect(reload).toHaveBeenCalledOnce();
});
