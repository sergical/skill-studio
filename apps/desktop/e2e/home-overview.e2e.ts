import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test("Home opens an agent's skill list, and a day of skill use opens Activity", async ({
  page,
}) => {
  await openApp(page);
  const agents = page.getByRole("region", { name: "What your agents see" });
  await expect(agents.getByText(/with full description|name only|shortened/).first()).toBeVisible();
  await page.setViewportSize({ width: 860, height: 660 });
  await expect(agents.getByRole("columnheader", { name: "Last session" })).toBeInViewport();
  await agents.getByRole("button", { name: "Codex", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Agent" })).toContainText("Codex");
  await expect(page.getByRole("tab", { name: "What it sees", selected: true })).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "Home", exact: true }).click();
  const chart = page.getByRole("toolbar", { name: /Skill uses by day/ });
  await expect(chart.getByRole("button")).toHaveCount(30);
  await page.getByRole("button", { name: "7 days", exact: true }).click();
  await expect(chart.getByRole("button")).toHaveCount(7);
  const days = chart.getByRole("button");
  await days.last().focus();
  await page.keyboard.press("ArrowLeft");
  const selected = days.nth(5);
  await expect(selected).toBeFocused();
  await expect(page.getByRole("figure").locator("figcaption")).toContainText(
    (await selected.getAttribute("aria-label"))!,
  );
  // The bar reads "Tue, Nov 3 · 41 uses"; Activity selects that day with the same count.
  const [day, uses] = (await selected.getAttribute("aria-label"))!.split(" · ");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Activity", exact: true })).toBeVisible();
  await expect(
    page.getByRole("listbox", { name: /^Uses per day/ }).getByRole("option", { selected: true }),
  ).toHaveAccessibleName(`${day}: ${uses}`);
});

test("switching agent tabs from the keyboard keeps focus on the tabs and keeps the agent", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Agents" }).click();
  const agent = page.getByRole("combobox", { name: "Agent" });
  await agent.click();
  await page.getByRole("option", { name: "Cursor", exact: true }).click();
  await page.getByRole("tab", { name: "What it sees" }).focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  const used = page.getByRole("tab", { name: "What it used" });
  await expect(used).toHaveAttribute("aria-selected", "true");
  await expect(used).toBeFocused();
  await expect(agent).toContainText("Cursor");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Enter");
  const sees = page.getByRole("tab", { name: "What it sees" });
  await expect(sees).toHaveAttribute("aria-selected", "true");
  await expect(sees).toBeFocused();
  await expect(agent).toContainText("Cursor");
});
