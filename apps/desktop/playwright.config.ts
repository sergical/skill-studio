import { defineConfig, devices } from "@playwright/test";

const PORT = 1440;

// The whole app on the dev harness (`src/dev/harness`), driven with real key presses. WebKit is
// the engine the Tauri window uses on macOS; Chromium catches the cases where they differ.
export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.e2e.ts",
  globalSetup: "./e2e/warm-up.ts",
  forbidOnly: !!process.env.CI,
  // Each test opens its own page on its own harness state, so tests in one file can run at the
  // same time, and CI shards split by test, not by file. CI uses one worker per core.
  fullyParallel: true,
  workers: process.env.CI ? "100%" : undefined,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 800 } },
    },
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
    },
  ],
  webServer: {
    command: `pnpm exec vite --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
  },
});
