import { expect, test, type Page } from "@playwright/test";
import { openApp, openSidebarPage } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openSidebarPage(page, "Settings");
});

/** The agents Activity's "Filter by agent" menu offers, after All agents. */
async function activityAgentChoices(page: Page): Promise<string[]> {
  await openSidebarPage(page, "Activity");
  await page.getByRole("button", { name: "Filter by agent" }).click();
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitemradio", { name: /^All agents/ })).toBeVisible();
  const choices = await menu.getByRole("menuitemradio").allInnerTexts();
  await page.keyboard.press("Escape");
  return choices.slice(1).map((choice) => choice.replace(/[\d,]+$/, "").trim());
}

test("turning off Find projects for an agent takes it out of Activity's agent filter, and turning it on puts it back", async ({
  page,
}) => {
  const codex = page.getByRole("switch", { name: "Find projects in Codex history" });
  await expect(codex).toBeChecked();

  await codex.click();

  await expect(codex).not.toBeChecked();
  const without = await activityAgentChoices(page);
  expect(without).not.toContain("Codex");
  expect(without).toContain("Claude Code");

  await openSidebarPage(page, "Settings");
  await expect(codex).not.toBeChecked();
  await codex.click();
  await expect(codex).toBeChecked();
  expect(await activityAgentChoices(page)).toContain("Codex");
});

test("the Telemetry switch keeps its new state after leaving Settings", async ({ page }) => {
  const telemetry = page.getByRole("switch", { name: "Telemetry" });
  await expect(telemetry).not.toBeChecked();

  await telemetry.click();
  await expect(telemetry).toBeChecked();

  await openSidebarPage(page, "Activity");
  await openSidebarPage(page, "Settings");
  await expect(telemetry).toBeChecked();
});
