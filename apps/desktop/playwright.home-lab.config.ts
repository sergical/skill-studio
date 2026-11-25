import { defineConfig } from "@playwright/test";
import base from "./playwright.config";

export default defineConfig(base, {
  globalSetup: [],
  testMatch: "**/*.home-lab.ts",
  use: { baseURL: "http://localhost:1441" },
  webServer: {
    command: "pnpm exec vite --port 1441 --strictPort",
    url: "http://localhost:1441",
    env: { VITE_HOME_DIRECTIONS: "1" },
    reuseExistingServer: false,
  },
});
