import { expect, test } from "@playwright/test";
import { openApp, openSkillPage } from "./app-harness";

// Global Universal folder, a Claude Code link to it, and Codex and OpenCode copies of their own.
const SKILL = "test-flake-hunt";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openSkillPage(page, SKILL);
});

test("Space on an agent's own copy parks it and keeps focus on its switch as the row moves, and Space again turns it back on", async ({
  page,
}) => {
  const codex = page.getByRole("switch", { name: "Global · Codex copy" });
  const openCode = page.getByRole("switch", { name: "Global · OpenCode copy" });
  await codex.focus();

  await page.keyboard.press("Space");

  await expect(codex).not.toBeChecked();
  await expect(codex).toBeFocused();
  await expect(openCode).toBeChecked();

  await page.keyboard.press("Space");

  await expect(codex).toBeChecked();
  await expect(codex).toBeFocused();
});

test("Cancel on Turn off for an agent that reads the Universal folder leaves its switch on and enabled", async ({
  page,
}) => {
  const claude = page.getByRole("switch", { name: "Global · Claude Code" });
  await claude.focus();

  await page.keyboard.press("Space");
  const dialog = page.getByRole("dialog", { name: `Turn off ${SKILL} for Claude Code?` });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();

  await expect(dialog).toHaveCount(0);
  await expect(claude).toBeChecked();
  await expect(claude).toBeEnabled();
});
