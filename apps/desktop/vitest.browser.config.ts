import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import stylex from "@stylexjs/unplugin";
import { playwright } from "@vitest/browser-playwright";

// Layout tests: happy-dom does no layout, so a Tailwind class that loses to another
// one (and so changes a width) only shows up in a real browser.
export default defineConfig({
  publicDir: "../../packages/marketing/public",
  plugins: [stylex.vite({ useCSSLayers: true }), react(), tailwindcss()],
  test: {
    include: ["src/**/*.browser.test.tsx"],
    // On CI the first marketing focus test took 15 to 19 s (measured 2026-11-28: clicks of 6.1 s
    // and 4.2 s), so the 15 s default ran out mid-click.
    testTimeout: 45_000,
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium", viewport: { width: 1280, height: 800 } }],
    },
  },
});
