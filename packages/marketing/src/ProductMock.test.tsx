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
  it("plans an update, skips the edited skill with its reason, and Confirm shows what ran", async () => {
    const user = await openPacks();
    const row = screen.getByRole("listitem", { name: "sergical/skills" });
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
  });
});
