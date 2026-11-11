// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { EditorCard } from "./EditorCard";

afterEach(() => {
  cleanup();
  clearMocks();
});

it("a failed editor read says so in the card, or Settings shows 'Looking for installed editors…' forever", async () => {
  mockIPC((command) => {
    if (command === "get_editor_choices") throw new Error("fixture read failed");
    throw new Error(`Unexpected command: ${command}`);
  });
  render(<EditorCard />);

  expect(await screen.findByText("Couldn't read installed editors.")).toBeTruthy();
  expect(screen.queryByText("Looking for installed editors…")).toBeNull();
});
