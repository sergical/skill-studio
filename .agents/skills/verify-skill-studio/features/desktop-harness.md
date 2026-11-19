# Desktop UI on the dev harness

The React app runs in a browser against `src/dev/harness`, which mocks every Tauri command with fixture data. Use it to prove UI flows: keyboard, focus, dialogs, menus, and what the UI sends.

## Sub-features

- `ui-add-sheet` the Add Skill sheet, including skills already installed in the scope.
- `ui-packs` the Packs page: plan, confirm and result dialogs.
- `ui-selection-bar` the selection bar's bulk actions.

## How to get to it (user POV)

- Run the app: `pnpm --filter skill-studio run dev` serves the harness (see the `dev-servers` skill before you start a server).
- Run the specs: `pnpm --filter skill-studio run test:e2e` (WebKit and Chromium). Specs are in `apps/desktop/e2e/*.e2e.ts`.

## Driving it with Playwright

Preconditions: dependencies installed (`pnpm install`).

- **Run one spec file.** Run `cd apps/desktop && CI=1 pnpm exec playwright test e2e/packs.e2e.ts`. Each test passes on both browsers.
- **Write a new journey.** Start from `openApp(page)` in `e2e/app-harness.ts`, then use `getByRole` with accessible names, for example `page.getByRole("button", { name: "Add from a repo" })` and `sheet.getByRole("textbox", { name: "Source" })`.
- **Prove it fails without the change.** Put the changed component back to `main` (`git show origin/main:<path> > <path>`), run the spec, see it fail, and restore the file.

## Gotchas

- The harness mocks the backend: a passing spec proves the UI, not the core. Pair it with a CLI run from the other feature files.
- Mock data must follow the core's real shapes and texts (`src/dev/harness/mock-packs.ts` copies the core's summaries by hand); a mismatch tests the mock, not the app.
- The real Tauri window is not reachable from Playwright or a browser tool.
