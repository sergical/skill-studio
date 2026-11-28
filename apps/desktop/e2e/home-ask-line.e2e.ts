import { expect, test } from "@playwright/test";

// PROTOTYPE home-library: "Ask in one line" (?v=5). Delete with src/prototype/home-library.
// The prompt the user copies is the one they previewed, and it names the skills that need them.
test("Copy prompt on Home copies a prompt that names the broken skills, and the preview shows it", async ({
  page,
}) => {
  await page.addInitScript(() => {
    // The clipboard keeps what was copied on the page, where the test can read it.
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text: string) => {
          document.documentElement.dataset.copied = text;
        },
      },
    });
  });
  await page.goto("/?persona=everything-wrong&v=5");
  await page.getByRole("button", { name: "Continue" }).click();
  const home = page.getByRole("region", { name: "Your skills at a glance" });
  await expect(home.getByRole("button", { name: /^4 broken$/ })).toBeVisible();
  // The counts open the same lists, so the long list is not on this Home.
  await expect(page.getByRole("heading", { name: "Your skills" })).toHaveCount(0);

  await home.getByRole("button", { name: "Show the prompt" }).click();
  const preview = home.getByRole("list", { name: "Prompt lines" });
  await expect(preview).toContainText("Fix the 4 broken skills: agent-browser");

  await home.getByRole("button", { name: "Copy prompt for your agent" }).click();
  await expect(home.getByRole("button", { name: "Prompt copied" })).toBeVisible();
  const copied = (await page.locator("html").getAttribute("data-copied")) ?? "";
  expect(copied).toContain("- Fix the 4 broken skills: agent-browser");
  for (const line of await preview.getByRole("listitem").allInnerTexts())
    expect(copied).toContain(line.trim());
});

// With no set-up agent on the Skill Studio MCP server, the prompt cannot work yet, so the way to
// set the server up sits beside Copy prompt instead of inside the closed preview.
test("Home offers to connect the MCP server beside Copy prompt when no agent has it", async ({
  page,
}) => {
  await page.goto("/?persona=everything-wrong&v=5");
  await page.getByRole("button", { name: "Continue" }).click();
  const home = page.getByRole("region", { name: "Your skills at a glance" });
  await expect(home.getByRole("button", { name: "Copy prompt for your agent" })).toBeVisible();
  await home.getByRole("button", { name: "Connect the MCP server" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
});
