---
name: verify-screen-states
description: Screenshot one Skill Studio screen for every dev-harness persona at three window sizes, then inspect each shot against a fixed list of edge cases. Use before you call a screen design done, after a change to Home or any screen whose content depends on agents, skill counts, usage history, budgets or health, or when asked to "check every state" or "make sure we didn't miss an edge case".
---

# Verify Screen States

A screen that looks right with one fixture breaks with another: one agent instead
of six, no history, a skill name 40 characters long, the smallest window. The
dev harness has personas (`apps/desktop/src/dev/harness/personas.ts`), each a
very different setup. This skill renders a screen for all of them at three sizes
and makes you look at every shot. Passing tests are not the proof here; the
screenshots are.

## 1. Render

From `apps/desktop`, with a port no other worktree uses:

```bash
E2E_PORT=1452 SCREEN_STATES=/tmp/screen-states \
  pnpm exec playwright test e2e/screen-states.e2e.ts --project=webkit --reporter=line \
  --output=/tmp/screen-states/test-results
```

- With no other setting it renders Home.
- `SCREEN_STATES_NAV` reaches another screen: comma-separated names of links,
  buttons or tabs clicked in order after Home loads, matched from the start of
  the name, so `Skills` matches the sidebar's "Skills 34". Examples:
  `Skills`, `Agents,What it used`, `Activity`, `Settings`.
- Output: `shots/<persona>-<size>.png`, one `.json` per shot with its persona,
  agents, size and any page errors, and `index.html`, a contact sheet.
- Two screens need steps the shared render can't take, so they have their own
  suites: `e2e/screen-states-skill-detail.e2e.ts` (opens named skills, writes
  `shots-skill-detail/` and `index-skill-detail.html`) and
  `e2e/screen-states-packs.e2e.ts` (opens the plan and export dialogs, writes
  `shots-packs/` and `index-packs.html`). Each suite has its own image folder,
  so all three can share one `SCREEN_STATES` folder.
- Agents running e2e side by side pass their own `--output` folder, or their
  runs delete each other's trace files in `test-results/`.
- Sizes: `min` 900×600 (the window's minimum in `tauri.conf.json`), `default`
  1200×800, `wide` 1600×1000.
- Any page error fails the run. Fix those first.

If a state you need has no persona, add one to `personas.ts` (agents, skill
count, `withUses` pattern: which agents, which days, one dominant skill) and
render again. Name the gap in your report.

## 2. Inspect every shot

Open each PNG. Do not sample. For every shot, check:

**Truth**

- Every number matches the persona: agent count, skill count, uses, broken,
  updates. Recount from `personas.ts` when one looks off.
- No claim without data: no "unused" without readable history, no "fits" or
  "cut" for an agent with no estimate, no "% from last week" without a whole
  week of history before it.
- An agent the persona has is on screen; an agent it lacks is not.
- Empty and zero states say something useful, and only once.

**Layout**

- Nothing overlaps, clips mid-word, or runs past its box. Long names truncate
  with an ellipsis.
- Left edges line up between sections; spacing matches between rows.
- At `min`, nothing needs horizontal scroll, and the first screen still shows
  the main answer. At `wide`, nothing stretches into a line too long to read.

**Meaning**

- Every chart has a legend of marks and labelled axes, not a sentence.
- Color means one thing: red broken, amber cut or drift, the data color for
  counts. A control that looks pressable is pressable.
- The same fact uses the same word everywhere on the screen.

## 3. Report

One table row per problem: persona and size, what is wrong, why it matters to a
user, and the file to change. Rank by harm. Then list the states no persona
covers yet. Attach the shot paths. Say "no problems" only after you opened every
shot.
