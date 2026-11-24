import { expect, test } from "@playwright/test";
import { deployment, HARNESS_HOME, HARNESS_PROJECT } from "../src/dev/harness/skill-fixture";
import { openApp } from "./app-harness";

test("opening a plugin skill uses the copy from its client group", async ({ page }) => {
  await openApp(page);
  const name = "git-worktree-helper";
  const copy = deployment({
    agent: "Codex",
    scope: "global",
    path: `${HARNESS_HOME}/.codex/plugins/cache/workflows/skills/${name}`,
    owner_kind: "plugin",
    mutability: "read-only",
    plugin: {
      name,
      harness: "Codex",
      marketplace: "workflows",
      id: `${name}@workflows`,
      version: "2.0.0",
    },
  });
  const ownCopies = [
    deployment({
      agent: "shared",
      scope: "global",
      path: `${HARNESS_HOME}/.agents/skills/${name}`,
    }),
    deployment({
      agent: "shared",
      scope: "project",
      project_path: HARNESS_PROJECT,
      path: `${HARNESS_PROJECT}/.agents/skills/${name}`,
    }),
  ];
  await page.evaluate(
    ([name, copies]) =>
      window.__harness?.updateSkill(name, (skill) => ({
        ...skill,
        deployments: [...skill.deployments, ...copies],
      })),
    [name, [copy, ...ownCopies]] as const,
  );

  await page
    .getByRole("navigation")
    .getByRole("button", { name: /^Plugins/ })
    .click();
  await page
    .getByRole("region", { name: `${name} for Codex` })
    .getByRole("button", { name: new RegExp(name) })
    .click();
  await expect(page.getByText(`Managed by the ${name} plugin for Codex.`)).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Copy to show and edit" })).toHaveCount(0);
  const source = `Codex plugin · ${name} · v2.0.0`;
  await expect(page.getByRole("paragraph").filter({ hasText: source })).toBeVisible();
  await expect(
    page.getByRole("complementary", { name: "Properties" }).getByText(source),
  ).toBeVisible();
});
