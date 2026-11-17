// @vitest-environment happy-dom
// ============================================================================
// Skill Studio marketing demo - Packs page tests
// The mock's Packs page must show the plan, skip reasons and labels the app shows.
// ============================================================================

import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";

import { ProductMock } from "./ProductMock";

afterEach(cleanup);

async function openPacks() {
  const user = userEvent.setup();
  render(<ProductMock theme="dark" onToggleTheme={() => {}} />);
  await user.click(screen.getByRole("button", { name: "Packs" }));
  return user;
}

describe("ProductMock Packs page", () => {
  it("plans an update, skips the edited skill with its reason, and Confirm updates the rest", async () => {
    const user = await openPacks();
    await user.click(screen.getByRole("button", { name: "Check upstream" }));
    const row = screen.getByRole("listitem", { name: "sergical/skills" });
    expect(row.textContent).toContain("4 skills, 1 edited locally, 2 behind upstream");
    await user.click(within(row).getByRole("button", { name: "Update" }));

    const dialog = screen.getByRole("alertdialog", { name: "Update sergical/skills?" });
    expect(within(dialog).getByRole("status").textContent).toBe(
      "Would update 3 skills; 1 skipped.",
    );
    expect(dialog.textContent).toContain(
      "has local edits, which the owner tool would overwrite or delete",
    );

    await user.click(within(dialog).getByRole("button", { name: "Update 3 skills" }));
    const done = screen.getByRole("alertdialog", { name: "Done" });
    expect(within(done).getByRole("status").textContent).toBe(
      "Updated 3 skills; 1 skipped; 0 failed.",
    );
    expect(within(done).getAllByText("Updated")).toHaveLength(3);
    await user.click(within(done).getByRole("button", { name: "Close" }));
    // The edited skill was skipped, so it is still behind.
    expect(screen.getByRole("listitem", { name: "sergical/skills" }).textContent).toContain(
      "4 skills, 1 edited locally, 1 behind upstream",
    );
  });

  it("removes a pack's clean skills and keeps the edited one with its own upstream state", async () => {
    const user = await openPacks();
    await user.click(screen.getByRole("button", { name: "Check upstream" }));
    const row = screen.getByRole("listitem", { name: "sergical/skills" });
    await user.click(within(row).getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("alertdialog", { name: "Remove sergical/skills?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove 3 skills" }));
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.getByRole("listitem", { name: "sergical/skills" }).textContent).toContain(
      "1 skill, 1 edited locally, 1 behind upstream, new upstream: add-customer-story, new-branch, prepare-videos",
    );
    expect(screen.getByRole("button", { name: "Add 3 new" })).toBeTruthy();
  });

  it("skips skills that are already installed when the same pack is added again", async () => {
    const user = await openPacks();
    const addTwice = async (expected: string) => {
      await user.type(
        screen.getByRole("textbox", { name: "Pack source" }),
        "acme-labs/review-skills",
      );
      await user.click(screen.getByRole("button", { name: "Add pack" }));
      const dialog = screen.getByRole("alertdialog", {
        name: "Add skills from acme-labs/review-skills?",
      });
      expect(within(dialog).getByRole("status").textContent).toBe(expected);
      return dialog;
    };
    const first = await addTwice("Would install 2 skills; 0 skipped.");
    await user.click(within(first).getByRole("button", { name: "Add 2 skills" }));
    await user.click(screen.getByRole("button", { name: "Close" }));

    const second = await addTwice("Would install 0 skills; 2 skipped.");
    expect(within(second).getAllByText("already installed here")).toHaveLength(2);
    expect(within(second).getByRole("button", { name: "Add" }).hasAttribute("disabled")).toBe(true);
  });

  it("turns off Update and Remove for a plugin pack and names the agent command", async () => {
    await openPacks();
    const row = screen.getByRole("listitem", { name: "figma@claude-plugins-official" });
    const update = within(row).getByRole("button", { name: "Update" });
    expect(update.hasAttribute("disabled")).toBe(true);
    expect(update.parentElement?.getAttribute("title")).toBe(
      "A plugin. The agent updates it with claude plugin update figma@claude-plugins-official.",
    );
    expect(within(row).getByRole("button", { name: "Remove" }).hasAttribute("disabled")).toBe(true);
  });

  it("offers a pack's new upstream skills only after Check upstream", async () => {
    const user = await openPacks();
    const row = screen.getByRole("listitem", { name: "getsentry/skills" });
    expect(within(row).queryByRole("button", { name: "Add 1 new" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Check upstream" }));
    await user.click(within(row).getByRole("button", { name: "Add 1 new" }));
    const dialog = screen.getByRole("alertdialog", { name: "Add skills from getsentry/skills?" });
    expect(within(dialog).getByRole("status").textContent).toBe(
      "Would install 1 skill; 0 skipped.",
    );
    await user.click(within(dialog).getByRole("button", { name: "Add 1 skill" }));
    await user.click(screen.getByRole("button", { name: "Close" }));
    // The core installs pack additions through skills.sh, the pack's own kind here.
    expect(screen.getAllByRole("listitem", { name: "getsentry/skills" })).toHaveLength(1);
    expect(screen.getByRole("listitem", { name: "getsentry/skills" }).textContent).toContain(
      "3 skills, up to date",
    );
  });
});
