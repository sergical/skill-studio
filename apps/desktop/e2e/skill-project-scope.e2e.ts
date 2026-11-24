import { expect, test } from "@playwright/test";
import { deployment, HARNESS_HOME, HARNESS_PROJECT } from "../src/dev/harness/skill-fixture";
import { openApp, openSkillsPage, skillRow } from "./app-harness";

test("a project's client filter, coverage and open file use that project's copies and its global copies", async ({
  page,
}) => {
  await openApp(page);
  const name = "internal-runbook";
  const otherProject = `${HARNESS_HOME}/src/acme-dashboard`;
  const copies = [
    deployment({
      agent: "Claude Code",
      scope: "project",
      project_path: otherProject,
      path: `${otherProject}/.claude/skills/${name}`,
    }),
    deployment({
      agent: "Codex",
      scope: "project",
      project_path: HARNESS_PROJECT,
      path: `${HARNESS_PROJECT}/.codex/skills/${name}`,
    }),
    deployment({ agent: "pi", scope: "global", path: `${HARNESS_HOME}/.pi/agent/skills/${name}` }),
  ];
  await page.evaluate(
    ([name, copies]) =>
      window.__harness!.updateSkill(name, (skill) => ({ ...skill, deployments: copies })),
    [name, copies] as const,
  );
  await openSkillsPage(page, "Skills", name);
  await page.getByRole("button", { name: /^Project:/ }).click();
  await page.getByRole("menuitemradio", { name: /^agent-studio/ }).click();
  await page.getByRole("button", { name: /^Filter,/ }).click();
  await page.getByRole("menuitemradio", { name: "Claude Code" }).click();
  await expect(skillRow(page, name)).toHaveCount(0);

  await page.getByRole("button", { name: /^Filter,/ }).click();
  await page.getByRole("menuitemradio", { name: "pi", exact: true }).click();
  await skillRow(page, name).getByText(name, { exact: true }).click();
  const selectedCopy = page.getByRole("combobox", { name: "Copy to show and edit" });
  await expect(selectedCopy).toContainText("Global · pi");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Coverage matrix view" }).click();
  const row = page
    .getByRole("row")
    .filter({ has: page.getByRole("button", { name, exact: true }) });
  const claudeColumn = await page
    .getByRole("columnheader", { name: "Claude Code" })
    .evaluate((header) => header.closest("th")!.cellIndex);
  await expect(row.getByRole("cell").nth(claudeColumn)).toHaveAttribute("title", "Not deployed");
  await row.getByRole("button", { name, exact: true }).click();
  await expect(selectedCopy).toContainText("Global · pi");
});
