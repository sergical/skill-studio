import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { openSkillsPage } from "./app-harness";

/** The clipboard keeps what was copied on the page, where the test can read it. */
async function catchClipboard(page: Page) {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text: string) => {
          document.documentElement.dataset.copied = text;
        },
      },
    });
  });
}
const copiedText = async (page: Page) =>
  (await page.locator("html").getAttribute("data-copied")) ?? "";

async function openHome(page: Page, persona: string) {
  await page.goto(`/?persona=${persona}`);
  await page.getByRole("button", { name: "Continue" }).click();
  return page.getByRole("region", { name: "Your skills at a glance" });
}

/** The count the Skills page's filter bar gives for what it lists. */
async function skillsListed(page: Page): Promise<number> {
  await expect(page.getByRole("heading", { name: "Skills", level: 1 })).toBeVisible();
  const status = page.getByRole("status").filter({ hasText: /^\d+ skills?$/ });
  return Number((await status.textContent())?.match(/\d+/)?.[0]);
}

test("Copy prompt copies the prompt the preview shows, one line per thing that needs the user", async ({
  page,
}) => {
  await catchClipboard(page);
  const home = await openHome(page, "everything-wrong");
  await expect(home.getByRole("button", { name: /^4 broken$/ })).toBeVisible();

  await home.getByRole("button", { name: "Show the prompt" }).click();
  const preview = home.getByRole("list", { name: "Prompt lines" });
  await expect(preview.getByRole("listitem").first()).toContainText("Fix the 4 broken skills:");

  await home.getByRole("button", { name: "Copy prompt for your agent" }).click();
  await expect(home.getByRole("button", { name: "Prompt copied" })).toBeVisible();
  const copied = await copiedText(page);
  const lines = await preview.getByRole("listitem").allInnerTexts();
  expect(lines.length).toBeGreaterThan(1);
  for (const line of lines) expect(copied).toContain(line.trim());

  // Each line copies on its own as a whole prompt.
  await preview
    .getByRole("button", { name: /^Copy only this line as a prompt: Fix the 4/ })
    .click();
  expect(await copiedText(page)).toMatch(/^Use the Skill Studio MCP tools to fix the 4 broken/);
});

test("Home offers to connect the MCP server beside Copy prompt when no agent has it", async ({
  page,
}) => {
  const home = await openHome(page, "everything-wrong");
  await expect(home.getByRole("button", { name: "Copy prompt for your agent" })).toBeVisible();
  await home.getByRole("button", { name: "Connect the MCP server" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
});

for (const { word, status } of [
  { word: "broken", status: "Broken" },
  { word: "updates", status: "Updates" },
  { word: "unused", status: "Not used in 30 days" },
]) {
  test(`the ${word} count on Home opens Skills listing that many skills`, async ({ page }) => {
    const home = await openHome(page, "everything-wrong");
    const count = home.getByRole("button", { name: new RegExp(`^\\d+ ${word}$`) });
    const shown = Number((await count.textContent())?.match(/\d+/)?.[0]);
    expect(shown).toBeGreaterThan(0);
    await count.click();
    expect(await skillsListed(page)).toBe(shown);
    await expect(page.getByRole("button", { name: status, exact: true })).toBeVisible();
  });
}

test("an agent on Home opens What it sees for that agent", async ({ page }) => {
  const home = await openHome(page, "two-agents");
  await home
    .getByRole("list", { name: "Agents" })
    .getByRole("button", { name: /^Codex/ })
    .click();
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
  await expect(page.getByRole("tab", { name: "What it sees", selected: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Agent" })).toHaveText(/Codex/);
});

test("with no readable history Home says so, and calls nothing unused", async ({ page }) => {
  const home = await openHome(page, "no-history");
  await expect(home).toContainText("No readable history yet");
  await expect(home.getByRole("button", { name: /unused$/ })).toHaveCount(0);
  await expect(home.getByRole("button", { name: /^Uses this week/ })).toHaveCount(0);
});

test("Check for updates on Home runs a check and changes the caption from never checked to checked", async ({
  page,
}) => {
  const home = await openHome(page, "everything-wrong");
  await page.evaluate(() => {
    const harness = window.__harness;
    if (!harness) throw new Error("dev harness missing");
    const snapshot = harness.snapshot();
    return harness.publish({
      ...snapshot,
      update_check: { ...snapshot.update_check, checked_at: null },
    });
  });
  await expect(home).toContainText("Not checked yet");

  await home.getByRole("button", { name: "Check for updates" }).click();

  await expect(home).toContainText("Checked");
  await expect(home).not.toContainText("Not checked yet");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

/** Gives Cursor one use of a skill Claude Code also used, so only who has history changes; returns
 * the skill only Cursor reads. */
async function addCursorHistory(page: Page): Promise<string> {
  const name = await page.evaluate(() => {
    const harness = window.__harness;
    if (!harness) throw new Error("dev harness missing");
    const snapshot = harness.snapshot();
    const used = snapshot.invocations[0];
    const hour = used.by_hour[0];
    const invocations = [
      {
        ...used,
        last_30_days: used.last_30_days + 1,
        by_harness_30_days: { ...used.by_harness_30_days, cursor: 1 },
        by_hour: [...used.by_hour, { ...hour, harness: "cursor", count: 1 }],
      },
      ...snapshot.invocations.slice(1),
    ];
    void harness.publish({ ...snapshot, invocations });
    return snapshot.skills.find((skill) => skill.deployments.every((d) => d.agent === "Cursor"))
      ?.name;
  });
  if (!name) throw new Error("no Cursor-only skill");
  return name;
}

// Code review round 3: with Cursor turned off and its earlier history still in the snapshot, Home
// offered 10 skills to park, one of them read only by Cursor. Only the kept agents' history counts.
test("with Cursor turned off, its history adds nothing to Home's unused count, the prompt or the Skills list", async ({
  page,
}) => {
  await catchClipboard(page);
  // Cursor kept: its history makes its one skill unused, ten to park.
  let home = await openHome(page, "claude-history-cursor-skill");
  const cursorOnly = await addCursorHistory(page);
  await expect(home.getByRole("button", { name: /^10 unused$/ })).toBeVisible();

  await page.goto("/?persona=claude-history-cursor-skill");
  await page.getByRole("checkbox", { name: "Cursor", exact: true }).press("Space");
  await page.getByRole("button", { name: "Continue" }).click();
  home = page.getByRole("region", { name: "Your skills at a glance" });
  await addCursorHistory(page);

  const unused = home.getByRole("button", { name: /^9 unused$/ });
  await expect(unused).toBeVisible();
  await home.getByRole("button", { name: "Show the prompt" }).click();
  const park = home
    .getByRole("list", { name: "Prompt lines" })
    .getByRole("listitem")
    .filter({ hasText: "not used in 30 days" });
  await expect(park).toContainText(/park the 9 skills/i);
  await expect(park).not.toContainText(cursorOnly);
  await home.getByRole("button", { name: "Copy prompt for your agent" }).click();
  await expect(home.getByRole("button", { name: "Prompt copied" })).toBeVisible();
  expect(await copiedText(page)).not.toContain(cursorOnly);

  await unused.click();
  expect(await skillsListed(page)).toBe(9);
});

// Code review round 4: with Cursor turned off, a skill with 7 uses by Cursor only was listed as
// unused and its row still said 7 uses. The row count reads the same kept-agent history.
test("with Cursor turned off, an unused skill's row shows no uses Cursor made", async ({
  page,
}) => {
  await page.goto("/?persona=claude-history-cursor-skill");
  await page.getByRole("checkbox", { name: "Cursor", exact: true }).press("Space");
  await page.getByRole("button", { name: "Continue" }).click();
  const home = page.getByRole("region", { name: "Your skills at a glance" });
  const idle = await page.evaluate(() => {
    const harness = window.__harness;
    if (!harness) throw new Error("dev harness missing");
    const snapshot = harness.snapshot();
    const used = new Set(snapshot.invocations.map((stat) => stat.skill));
    const skill = snapshot.skills.find(
      (s) => !used.has(s.name) && s.deployments.every((d) => d.agent === "Claude Code"),
    );
    if (!skill) throw new Error("no idle Claude Code skill");
    const template = snapshot.invocations[0];
    const stat = {
      ...template,
      skill: skill.name,
      last_24_hours: 0,
      last_7_days: 7,
      last_14_days: 7,
      last_30_days: 7,
      by_project_30_days: {},
      by_day: {},
      by_harness_30_days: { cursor: 7 },
      by_hour: [],
    };
    void harness.publish({ ...snapshot, invocations: [...snapshot.invocations, stat] });
    return skill.name;
  });

  const unused = home.getByRole("button", { name: /^9 unused$/ });
  await expect(unused).toBeVisible();
  await unused.click();
  expect(await skillsListed(page)).toBe(9);
  // Only skill rows carry `aria-selected`; a group header row holds its rows' text too.
  const row = page
    .getByRole("grid", { name: "Skills" })
    .locator('[role="row"][aria-selected]')
    .filter({ has: page.getByRole("gridcell", { name: idle, exact: true }) });
  await expect(row).toHaveCount(1);
  await expect(row.getByTitle(/uses? in the last 30 days/)).toHaveCount(0);
});

// Code review round 5: Most used must rank by the kept agents' uses too. With Cursor turned off,
// a skill Cursor used 20 times ranks below one Claude Code used 5 times.
test("with Cursor turned off, Most used ranks a skill by Claude Code's uses, not Cursor's", async ({
  page,
}) => {
  await page.goto("/?persona=claude-history-cursor-skill");
  await page.getByRole("checkbox", { name: "Cursor", exact: true }).press("Space");
  await page.getByRole("button", { name: "Continue" }).click();
  const [byCursor, byClaude] = await page.evaluate(() => {
    const harness = window.__harness;
    if (!harness) throw new Error("dev harness missing");
    const snapshot = harness.snapshot();
    const used = new Set(snapshot.invocations.map((stat) => stat.skill));
    const [cursorSkill, claudeSkill] = snapshot.skills
      .filter((s) => !used.has(s.name) && s.deployments.every((d) => d.agent === "Claude Code"))
      .map((s) => s.name);
    if (!claudeSkill) throw new Error("need two idle Claude Code skills");
    const template = snapshot.invocations[0];
    const stat = (skill: string, harnessId: string, uses: number) => ({
      ...template,
      skill,
      last_24_hours: 0,
      last_7_days: uses,
      last_14_days: uses,
      last_30_days: uses,
      by_project_30_days: {},
      by_day: {},
      by_harness_30_days: { [harnessId]: uses },
      by_hour: [],
    });
    // One shared description, so a filter lists just these two.
    const probe = (skill: (typeof snapshot.skills)[number]) =>
      skill.name === cursorSkill || skill.name === claudeSkill
        ? { ...skill, description: "Most used sort probe" }
        : skill;
    void harness.publish({
      ...snapshot,
      skills: snapshot.skills.map(probe),
      invocations: [
        ...snapshot.invocations,
        stat(cursorSkill, "cursor", 20),
        stat(claudeSkill, "claude-code", 5),
      ],
    });
    return [cursorSkill, claudeSkill];
  });

  await openSkillsPage(page, "Skills", "Most used sort probe");
  await page.getByRole("combobox", { name: "Sort" }).click();
  await page.getByRole("option", { name: "Most used" }).click();

  const rows = page.getByRole("grid", { name: "Skills" }).locator('[role="row"][aria-selected]');
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0).getByRole("gridcell", { name: byClaude, exact: true })).toBeVisible();
  await expect(rows.nth(1).getByRole("gridcell", { name: byCursor, exact: true })).toBeVisible();
});
