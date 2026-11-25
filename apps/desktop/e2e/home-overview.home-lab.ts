import { expect, test } from "@playwright/test";

test("open a client report and compare the native Home designs", async ({ page }) => {
  await page.goto("/");
  const heading = page.getByRole("heading", { name: "Library overview", exact: true });
  await expect(heading).toBeVisible();
  const table = page.getByRole("region", { name: "Client visibility" });
  await expect(table.getByText(/shortened|described|name only/i).first()).toBeVisible();
  await page.setViewportSize({ width: 860, height: 660 });
  await expect(table.getByRole("columnheader", { name: "Last saved listing" })).toBeInViewport();
  await table.getByRole("button", { name: "Codex", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Client configuration", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Agent" })).toContainText("Codex");
  await page
    .getByRole("button", { name: "Home", exact: true })
    .filter({ visible: true })
    .first()
    .click();
  const picker = page.getByRole("button", { name: "Compare designs" });
  for (const [name, region] of [
    ["Repairs", "Repair queue"],
    ["Clients", "Client overview"],
    ["Recent work", "Recent sessions"],
  ]) {
    await picker.click();
    await page.getByRole("menuitemradio", { name, exact: true }).click();
    await expect(page.getByRole("region", { name: region, exact: true })).toBeVisible();
  }
  await picker.click();
  await page.getByRole("menuitemradio", { name: "Library overview" }).click();
  await expect(heading).toBeVisible();
  const chart = page.getByRole("toolbar", { name: /Recorded loads by day/ });
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

test("Recent work shows the next session report while the view stays open", async ({ page }) => {
  await page.goto("/?v=4");
  const view = page.getByRole("region", { name: "Recent sessions" });
  await expect(view.locator("summary").first()).toBeVisible();
  const [previous, next] = await page.evaluate(() =>
    window
      .__harness!.snapshot()
      .skills.slice(0, 2)
      .map((skill) => skill.name),
  );
  await expect(view.locator("summary").getByText(previous, { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const harness = window.__harness!;
    const snapshot = harness.snapshot();
    // The session fixture uses the first skill as the latest recorded load.
    const [first, second, ...rest] = snapshot.skills;
    await harness.publish({ ...snapshot, skills: [second, first, ...rest] });
  });
  await expect(view.locator("summary").getByText(next, { exact: true })).toBeVisible();
});
