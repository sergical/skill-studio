// @vitest-environment happy-dom

import { useState } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { SplitSkillDialog } from "./SplitSkillDialog";

function Confirmation() {
  const [open, setOpen] = useState(true);
  return (
    open && (
      <SplitSkillDialog
        skillName="gamma"
        target={{ deployment_id: "dep:v1/global/universal/gamma" }}
        projectPath={null}
        readers={["codex"]}
        onClose={() => setOpen(false)}
      />
    )
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
});

it("splitting explains which copies update and how to undo updates or says copies never update", async () => {
  mockIPC(() => []);
  const user = userEvent.setup();
  render(<Confirmation />);
  await screen.findByRole("dialog");
  expect(
    screen.getByText(/Skill Studio updates unedited copies from the same source/),
  ).toBeTruthy();
  expect(screen.getByText(/Undo updates from Activity/)).toBeTruthy();
  await user.click(screen.getByRole("checkbox", { name: "Codex" }));
  expect(screen.getByRole("button", { name: "Split" }).hasAttribute("disabled")).toBe(true);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
});
