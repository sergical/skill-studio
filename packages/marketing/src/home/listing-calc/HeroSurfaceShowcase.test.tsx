// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { HeroSurfaceShowcase } from "./HeroSurfaceShowcase";

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("shows Copied only on the bar whose text was copied, not on the next tab's bar", async () => {
  const user = userEvent.setup();
  render(<HeroSurfaceShowcase theme="dark" onToggleTheme={() => {}} />);

  await user.click(screen.getByRole("tab", { name: /CLI/ }));
  await user.click(screen.getByRole("button", { name: /^Copy command/ }));
  expect(screen.getByRole("button", { name: "Copied command" })).toBeTruthy();

  await user.click(screen.getByRole("tab", { name: /MCP/ }));
  expect(screen.getByRole("button", { name: "Copy MCP setup prompt" })).toBeTruthy();
});
