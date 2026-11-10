// @vitest-environment happy-dom
import { useState } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { Drawer } from "@skill-studio/ui";
import type { SkillWithStatus, SkillDetails } from "@skill-studio/lib";
import { SkillDetailPanel } from "./SkillDetailPanel";

const skill: SkillWithStatus = {
  id: "owner/repo/sample",
  name: "sample",
  installs: 0,
  author: null,
  description: null,
  tags: null,
  is_installed: false,
  top_source: "owner/repo",
};
function Details({ selected = skill }: { selected?: SkillWithStatus }) {
  const [open, setOpen] = useState(true);
  return (
    <Drawer open={open} onOpenChange={setOpen}>
      <SkillDetailPanel
        skill={selected}
        onClose={() => setOpen(false)}
        onInstallStart={() => {}}
        onInstallPaused={() => {}}
        onInstallComplete={() => {}}
        onUpdateComplete={() => {}}
        onRemoveComplete={() => {}}
      />
    </Drawer>
  );
}
afterEach(() => {
  cleanup();
  clearMocks();
});
it.each([
  { key: "{Enter}", label: "Enter" },
  { key: " ", label: "Space" },
])(
  "Browse shows a details error and Retry works with $label or a failed server leaves an empty panel",
  async ({ key }) => {
    let attempts = 0;
    mockIPC((command) => {
      if (command === "get_skill_details") {
        attempts += 1;
        if (attempts === 1) throw new Error("The server is unavailable");
        return {
          id: skill.id,
          slug: skill.name,
          source: "owner/repo",
          installs: 0,
          hash: "revision",
          skill_md: "# Sample skill body",
        };
      }
      if (command === "get_harnesses_choice") return null;
      if (command === "get_add_method_defaults")
        return {
          dotagents_installed: false,
          has_skill_lock: false,
          installed_harnesses: [],
          claude_reads_shared_folder: false,
        };
      throw new Error(`Unexpected ${command}`);
    });
    const user = userEvent.setup();
    render(<Details />);
    expect((await screen.findByRole("alert")).textContent).toContain("The server is unavailable");
    const retry = screen.getByRole("button", { name: "Retry" });
    for (let i = 0; i < 20 && document.activeElement !== retry; i += 1) await user.tab();
    expect(document.activeElement).toBe(retry);
    await user.keyboard(key);
    expect(await screen.findByText("# Sample skill body")).toBeTruthy();
    expect(attempts).toBe(2);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  },
);

it("changing the Browse selection clears its old source while loading or offers the wrong repository", async () => {
  let finish: ((details: SkillDetails) => void) | undefined;
  mockIPC((command, args) => {
    if (command === "get_skill_details") {
      if (args && "skillId" in args && args.skillId === skill.id)
        return {
          id: skill.id,
          slug: skill.name,
          source: "owner/repo",
          installs: 0,
          hash: "one",
          skill_md: "First skill body",
        };
      return new Promise<SkillDetails>((resolve) => {
        finish = resolve;
      });
    }
    if (command === "get_harnesses_choice") return null;
    if (command === "get_add_method_defaults")
      return {
        dotagents_installed: false,
        has_skill_lock: false,
        installed_harnesses: [],
        claude_reads_shared_folder: false,
      };
    throw new Error(`Unexpected ${command}`);
  });
  const user = userEvent.setup();
  const view = render(<Details />);
  expect(await screen.findByText("First skill body")).toBeTruthy();
  view.rerender(
    <Details selected={{ ...skill, id: "second/repo/next", name: "next", top_source: null }} />,
  );
  expect(await screen.findByText("Loading content…")).toBeTruthy();
  expect(screen.queryByText("owner/repo")).toBeNull();
  finish?.({
    id: "second/repo/next",
    slug: "next",
    source: "second/repo",
    installs: 0,
    hash: "two",
    skill_md: "Second skill body",
  });
  expect(await screen.findByText("Second skill body")).toBeTruthy();
  expect(screen.getByText("second/repo")).toBeTruthy();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
