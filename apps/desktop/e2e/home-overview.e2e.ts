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
  await expect(page.getByRole("button", { name: "What it sees", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

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
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Activity", exact: true })).toBeVisible();
});
