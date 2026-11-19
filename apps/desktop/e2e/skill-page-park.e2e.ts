import { expect, test } from "@playwright/test";
import {
  openApp,
  openSidebarPage,
  openSkillPage,
  openSkillsPage,
  skillRow,
  toasts,
} from "./app-harness";

const SKILL = "code-review";

test("Park for every agent turns every copy off and Activity lists it with no Restore; Turn on for every agent turns them back on", async ({
  page,
}) => {
  await openApp(page);
  await openSkillPage(page, SKILL);
  const more = page.getByRole("button", { name: "More actions", exact: true });
  const copies = page.getByRole("main").getByRole("switch");
  await expect(copies).toHaveCount(2);

  await more.click();
  await page.getByRole("menuitem", { name: "Park for every agent" }).click();

  await expect(toasts(page).getByText(`Parked ${SKILL}`)).toBeVisible();
  // The Universal folder moves to the parked folder and the Claude Code link goes with it, so one
  // copy is left, and it is off. Count first: `all()` on an empty list checks nothing.
  await expect(copies).toHaveCount(1);
  await expect(copies).not.toBeChecked();

  await openSidebarPage(page, "Activity");
  await expect(page.getByRole("main").getByText("Parked", { exact: true })).toBeVisible();
  await expect(page.getByRole("main").getByRole("button", { name: "Restore" })).toHaveCount(0);

  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(0);
  await openSkillPage(page, SKILL, "Parked");
  await more.click();
  await page.getByRole("menuitem", { name: "Turn on for every agent" }).click();

  await expect(toasts(page).getByText(`Turned on ${SKILL}`)).toBeVisible();
  await expect(copies).toHaveCount(2);
  for (const copy of await copies.all()) await expect(copy).toBeChecked();
  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(1);
});
