import { expect, test } from "@playwright/test";
import { deployment, HARNESS_HOME } from "../src/dev/harness/skill-fixture";
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
  await page.evaluate(
    ([name, copy]) =>
      window.__harness?.updateSkill(name, (skill) => ({
        ...skill,
        deployments: [...skill.deployments, copy],
      })),
    [name, copy] as const,
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
  const source = `Codex plugin · ${name} · v2.0.0`;
  await expect(page.getByRole("paragraph").filter({ hasText: source })).toBeVisible();
  await expect(
    page.getByRole("complementary", { name: "Properties" }).getByText(source),
  ).toBeVisible();
});
