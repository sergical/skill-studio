// @vitest-environment happy-dom

import { useState } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { universalDeployment } from "../../dev/harness/scanned-deployment";
import { skill } from "../../dev/harness/skill-fixture";
import { buildScopeGroups } from "./skill-location-status";
import { turnOffActionFor } from "./skill-agent-off-model";
import { TurnOffForAgentDialog } from "./TurnOffForAgentDialog";

function Confirmation() {
  const [open, setOpen] = useState(true);
  const [group] = buildScopeGroups(
    skill({
      name: "find-bugs",
      deployments: [universalDeployment({ universalPath: "/home/.agents/skills/find-bugs" })],
    }),
  );
  const row = group.rows.find((item) => item.harness === "codex");
  const request = row && turnOffActionFor(group, row);
  if (!request) throw new Error("Fixture has no Codex reader");
  return (
    open && (
      <TurnOffForAgentDialog
        skillName="find-bugs"
        request={request}
        onClose={() => setOpen(false)}
        onOffEverywhere={() => setOpen(false)}
        onTurnedOff={() => setOpen(false)}
      />
    )
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
});

it("turning off one agent explains manual copies before confirmation or hides the later update limit", async () => {
  mockIPC(() => ({ refusal: null, git_tracked: false, project: null }));
  const user = userEvent.setup();
  render(<Confirmation />);
  await screen.findByRole("button", { name: "Turn off for Codex" });
  expect(
    screen.getByText(
      /After you turn the Codex copy back on, it is a manual copy and Skill Studio will not update it/,
    ),
  ).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("the one-agent confirmation closes with Escape or traps keyboard users", async () => {
  mockIPC(() => ({ refusal: null, git_tracked: false, project: null }));
  const user = userEvent.setup();
  render(<Confirmation />);
  await screen.findByRole("button", { name: "Turn off for Codex" });
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
});
