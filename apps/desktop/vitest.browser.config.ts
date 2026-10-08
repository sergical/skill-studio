import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { playwright } from "@vitest/browser-playwright";

// Layout tests: happy-dom does no layout, so a Tailwind class that loses to another
// one (and so changes a width) only shows up in a real browser.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    include: ["src/**/*.browser.test.tsx"],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium", viewport: { width: 1280, height: 800 } }],
    },
  },
});
