import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test("a larger budget marks the skills that gain a description and gives the setting to paste", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Client configuration" }).click();
  await page.getByText("Browse individual entries", { exact: true }).click();
  const rows = page.getByRole("list", { name: "Listing" }).getByRole("listitem");
  await expect(rows.first()).toBeVisible();
  const nameOnly = rows.filter({ hasText: "Name only" }).first();
  await expect(nameOnly).toBeVisible();
  const name = await nameOnly.getAttribute("aria-label");
  if (!name) throw new Error("Missing skill name");
  await page.getByText("Preview a different budget", { exact: true }).click();
  const budget = page.getByRole("textbox", {
    name: "Try a budget, in percent of the context window",
  });
  await budget.fill("2");
  await budget.press("Enter");
  await expect(page.getByText(/\d+ more fit in full/)).toBeVisible();

  await page.getByRole("button", { name: "Show only changes" }).click();
  await page.getByRole("textbox", { name: "Find an entry" }).fill(name);
  const changed = rows.and(page.getByLabel(name, { exact: true }));
  await expect(changed).toContainText("Name only");
  await expect(changed).toContainText("Described");

  const setting = page.getByRole("region", { name: "Listing setting" });
  await expect(setting).toContainText('"skillListingBudgetFraction": 0.02');
  await expect(setting).toContainText("chezmoi apply");
});

test("the Codex listing gives the tier and how far the budget is from the next one", async ({
  page,
}) => {
  await openApp(page);
  await page
    .getByRole("region", { name: "Your clients" })
    .getByRole("button", { name: /Codex/ })
    .click();
  await expect(page.getByRole("combobox", { name: "Agent" })).toContainText("Codex");
  await expect(page.getByRole("region", { name: "Listing sources" })).toBeVisible();
  await page.getByText("How the estimate was calculated", { exact: true }).click();
  const tier = page.getByRole("region", { name: "Listing tier" });
  await expect(tier).toContainText("Descriptions are shortened");
  await expect(
    tier.getByText(/Names and paths only 3,214, of which the skill roots table is 212\./),
  ).toBeVisible();
  await expect(
    page.getByText(/2,226 tokens are left before Codex drops all descriptions/),
  ).toBeVisible();
});

test("a Cursor-only setup can follow a scanned skill from Home and back", async ({ page }) => {
  await page.goto("/");
  const cursor = page.getByRole("checkbox", { name: "Cursor", exact: true });
  await expect(cursor).toBeChecked();
  const others = page.getByRole("checkbox", { name: /^(?!Cursor$).+/, checked: true });
  for (let count = await others.count(); count > 0; count--) {
    await others.first().press("Space");
    await expect(others).toHaveCount(count - 1);
  }
  await page.getByRole("button", { name: "Continue" }).click();
  const clients = page.getByRole("region", { name: "Your clients" });
  await expect(clients.getByRole("button")).toHaveCount(1);
  await clients.getByRole("button", { name: "Cursor" }).click();
  const agent = page.getByRole("combobox", { name: "Agent", exact: true });
  await expect(agent).toContainText("Cursor");
  await page.getByText("How Cursor lists skills", { exact: true }).click();
  await expect(page.getByRole("link", { name: "Cursor documentation" })).toHaveAttribute(
    "href",
    "https://cursor.com/docs/skills",
  );
  await page.getByRole("textbox", { name: "Find an entry" }).fill("code-review");
  await page
    .getByRole("list", { name: "Listing", exact: true })
    .getByRole("button", { name: "code-review", exact: true })
    .click();
  const preview = page.getByRole("region", { name: "Skill listing preview" });
  await expect(preview.getByRole("combobox", { name: "Client for this skill" })).toContainText(
    "Cursor",
  );
  await expect(preview).toContainText(".agents/skills/code-review");
  await expect(preview).not.toContainText("Full description");
  await page.getByRole("button", { name: "Client configuration", exact: true }).last().click();
  await expect(agent).toContainText("Cursor");
  await page.getByRole("navigation").getByRole("button", { name: "Sessions" }).click();
  await expect(agent).toContainText("Cursor");
  await page.getByRole("button", { name: "See skills for Cursor" }).click();
  await expect(page.getByRole("textbox", { name: "Find an entry" })).toBeVisible();
});

test("a client comparison shows the source of the entries behind a difference", async ({
  page,
}) => {
  await openApp(page);
  await page
    .getByRole("region", { name: "Your clients" })
    .getByRole("button", { name: "Cursor" })
    .click();
  await page.getByText("Compare clients", { exact: true }).click();
  await page.getByRole("combobox", { name: "Compare with" }).click();
  await page.getByRole("option", { name: "Codex", exact: true }).click();
  await page.getByText(/\d+ additional entr(?:y|ies) in Codex/).click();
  const extra = page.getByRole("listitem").filter({ hasText: "skill-installer" });
  await expect(extra).toContainText("Session record");
  await expect(extra).toContainText(".codex/skills/.system/skill-installer");
});

test("save and remove a personal Codex budget from client configuration", async ({ page }) => {
  await openApp(page);
  await page
    .getByRole("region", { name: "Your clients" })
    .getByRole("button", { name: "Codex", exact: true })
    .click();
  const settings = page.getByRole("region", { name: "Effective settings" });
  await settings.getByRole("button", { name: "Edit Listing budget" }).click();
  await settings.getByRole("textbox", { name: "Tokens" }).fill("6300");
  await settings.getByRole("button", { name: "Save", exact: true }).click();
  await expect(settings).toContainText("6,300 tokens configured");
  await settings.getByRole("button", { name: "Edit Listing budget" }).click();
  await settings.getByRole("button", { name: "Remove override" }).click();
  await expect(settings).toContainText("Client default");
});
