import { expect, test } from "@playwright/test";
import { openApp, openSkillPage, openSkillsPage, skillRow } from "./app-harness";

const SKILL = "pdf-processing";

test("choosing User only marks the skill You run it on the Skills list and sticks after a rescan", async ({
  page,
}) => {
  await openApp(page);
  await openSkillPage(page, SKILL);
  const invocation = page.getByRole("group", { name: "Invocation for Global folder" });
  await expect(invocation.getByRole("button", { name: "Both" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await invocation.getByRole("button", { name: "User only" }).click();

  await expect(invocation.getByRole("button", { name: "User only" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByRole("navigation").getByRole("button", { name: "Sync installed skills" }).click();
  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL).getByRole("img", { name: "You run it" })).toBeVisible();
  await openSkillPage(page, SKILL);
  await expect(invocation.getByRole("button", { name: "User only" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(invocation.getByRole("button", { name: "Both" })).toHaveAttribute(
    "aria-pressed",
    "false",
  );
});
