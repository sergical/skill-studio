import { expect, test, type Page } from "@playwright/test";
import type { InstalledSkill } from "@skill-studio/lib";
import { deployment, HARNESS_HOME } from "../src/dev/harness/skill-fixture";
import {
  openApp,
  openSkillsPage as openFilteredSkills,
  reinstallParkedSkill,
  skillRow,
} from "./app-harness";

/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

async function openSkillsPage(page: Page) {
  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  await expect(page.getByRole("grid", { name: "Skills" })).toBeVisible();
}

test("X on a focused Skills row selects its skill and shows the selection bar with its name, and Escape clears it", async ({
  page,
}) => {
  await openSkillsPage(page);
  const row = page.getByRole("grid", { name: "Skills" }).locator(SKILL_ROW).first();
  const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  await row.focus();

  await page.keyboard.press("x");
  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("checkbox", { name: `Select ${name}` })).toBeChecked();
  await expect(page.getByRole("button", { name: "Open", exact: true })).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(row).toHaveAttribute("aria-selected", "false");
  await expect(page.getByRole("button", { name: "Open", exact: true })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Skills", level: 1 })).toBeVisible();
});

test("X after a click on a row's own button selects that row, not the first row", async ({
  page,
}) => {
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const first = grid.locator(SKILL_ROW).first();
  const row = grid
    .locator(SKILL_ROW)
    .filter({ has: page.getByRole("button", { name: "Update", exact: true }) })
    .last();
  const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  const button = page
    .getByRole("checkbox", { name: `Select ${name}` })
    .locator("xpath=ancestor::*[@role='row']")
    .getByRole("button", { name: "Update", exact: true });

  await button.click();
  await page.keyboard.press("x");

  await expect(page.getByRole("checkbox", { name: `Select ${name}` })).toBeChecked();
  await expect(first).toHaveAttribute("aria-selected", "false");
});

test("clicking the checkbox in a Skills row's slot selects the skill and stays on Skills", async ({
  page,
}) => {
  await openSkillsPage(page);
  const row = page.getByRole("grid", { name: "Skills" }).locator(SKILL_ROW).first();
  const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  await row.hover();

  await page.getByRole("checkbox", { name: `Select ${name}` }).click();

  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("heading", { name: "Skills", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open", exact: true })).toBeVisible();
});

test("Tab from a focused Skills row with a fix reaches its checkbox and then its named fix button", async ({
  page,
}) => {
  const name = await reinstallParkedSkill(page);
  await openSkillsPage(page);
  const row = page.getByRole("grid", { name: "Skills" }).getByRole("row").filter({ hasText: name });
  const fix = row.locator('[role="gridcell"]:last-child button');
  await expect(fix).toHaveCount(1);
  const fixLabel = (await fix.textContent())!.trim();
  expect(fixLabel).not.toBe("");

  await row.focus();
  await page.keyboard.press("Tab");
  await expect(row.getByRole("checkbox", { name: `Select ${name}` })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(row.getByRole("button", { name: fixLabel, exact: true })).toBeFocused();
});

test("a skill only in the Universal folder shows the agents that read it, not Claude Code", async ({
  page,
}) => {
  await openFilteredSkills(page, "Skills", "pdf-processing");
  const row = skillRow(page, "pdf-processing");

  for (const agent of ["Codex", "OpenCode", "pi", "Cursor", "Grok Build"]) {
    await expect(row.getByRole("img", { name: new RegExp(`^${agent} · `) })).toBeVisible();
  }
  await expect(row.getByRole("img", { name: /^Claude Code/ })).toHaveCount(0);
});

test("a plugin skill with an update shows Claude Code, which loads the plugin, as reading it", async ({
  page,
}) => {
  await page.goto("/?persona=everything-wrong");
  await page.getByRole("button", { name: "Continue" }).click();
  await openFilteredSkills(page, "Skills", "changelog-formatter");

  await expect(
    skillRow(page, "changelog-formatter").getByRole("img", { name: /^Claude Code · / }),
  ).toBeVisible();
});

// Code review round 4: a skill with a personal Codex copy and a Claude Code plugin copy showed
// only Codex, because the list drops plugin copies from the user's own skills.
test("a skill with a personal Codex copy and a Claude Code plugin copy shows both agents as reading it", async ({
  page,
}) => {
  const name = "pdf-processing";
  const copies = [
    deployment({ agent: "Codex", scope: "global", path: `${HARNESS_HOME}/.codex/skills/${name}` }),
    deployment({
      agent: "Claude Code",
      scope: "global",
      path: `${HARNESS_HOME}/.claude/plugins/cache/docs-plugins/docs/skills/${name}`,
      owner_kind: "plugin",
      mutability: "read-only",
      plugin: {
        harness: "Claude Code",
        id: "docs@docs-plugins",
        name: "docs",
        marketplace: "docs-plugins",
        version: "1.0.0",
      },
    }),
  ];
  await page.evaluate(
    ([skillName, deployments]) =>
      window.__harness?.updateSkill(skillName, (skill: InstalledSkill) => ({
        ...skill,
        deployments,
      })),
    [name, copies] as const,
  );
  await openFilteredSkills(page, "Skills", name);
  const row = skillRow(page, name);

  await expect(row.getByRole("img", { name: /^Codex · / })).toBeVisible();
  await expect(row.getByRole("img", { name: /^Claude Code · / })).toBeVisible();
});

test("a skill installed only globally shows the globe alone in Where, and a project install shows its project", async ({
  page,
}) => {
  const where = (name: string) => skillRow(page, name).locator('[role="gridcell"]:nth-child(3)');

  await openFilteredSkills(page, "Skills", "pdf-processing");
  await expect(
    where("pdf-processing").getByRole("img", { name: "Global", exact: true }),
  ).toBeVisible();
  await expect(where("pdf-processing")).toHaveText("");

  await openFilteredSkills(page, "Skills", "sdk-migration");
  await expect(where("sdk-migration")).toHaveText("acme-dashboard");
  await expect(
    where("sdk-migration").getByRole("img", { name: "Global", exact: true }),
  ).toHaveCount(0);
});
