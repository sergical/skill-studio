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
  const parked = page.getByRole("main").getByText("Parked", { exact: true });
  await expect(parked).toBeVisible();
  await expect(page.getByRole("main").getByRole("button", { name: "Restore" })).toHaveCount(0);
  // The Parked row has no button, so it takes focus itself and shows why it cannot be restored.
  const reason = page.getByRole("main").getByText("Unpark the skill to reverse this.");
  const reasonOpacity = () => reason.evaluate((el) => getComputedStyle(el.parentElement!).opacity);
  expect(await reasonOpacity()).toBe("0");
  await parked.locator("xpath=ancestor::div[@tabindex='0'][1]").focus();
  await expect.poll(reasonOpacity).toBe("1");

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
