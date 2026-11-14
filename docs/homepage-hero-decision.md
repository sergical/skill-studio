# Homepage hero — decided from the `/proto` hero picker (2026-11-14)

The homepage hero shows the skill mess at real scale and lets the visitor clean it up by hand. It
was chosen from an isolated prototype picker over three rounds; the picker and its variants are
deleted.

## Direction: Throw

- The hero is two columns: copy and Download on the left, a pile of 92 skill chips in 11 folders
  with the mascot on the right (`src/home/SkillThrow.tsx`). Below 1080 px it stacks, copy first.
- The pile is physical. Drag a chip and throw it: it tumbles, knocks its neighbours and lands on
  the heap. A chip held by the cursor swings like a pendulum from the grab point.
- Throw a chip at the mascot and the mascot sorts it: kept skills (ok, broken, outdated) fly to
  the kept shelf, copies and unused skills fly to the Parked drawer.
- A click on a copy or an unused skill pops it: it swells in place, then flies to the Parked
  drawer. A click on a kept skill does nothing. Park is the only "off" in the app, so the drawer
  is named Parked, not Deleted.
- A click on the mascot tidies everything; a second click makes the mess again.
- No caption tells the visitor what to do. The interaction is found by touch, not taught.
- The intro (chips fall into the heap) plays once per session (`sessionStorage`). Reduced motion
  skips all motion: pops and tidy move chips to their end positions at once.
- One rAF loop, 240 Hz substeps, gravity 2400 px/s², restitution 0.38, max release speed
  3200 px/s; the loop sleeps when nothing moves.

## Page order

Sticky header with Download → hero (Throw) → playable product mock → Works with → How it works
(`FirstScanSteps`) → day-to-day walkthrough → install → FAQ → closing call to action. The dither
sits at the top of the page and in the closing section.

## Rejected

- **Pile** (the first `SkillPile` round): chips flew in, the mascot popped, then the pile tidied
  itself. The motion played at the visitor instead of reacting to them.
- **Drop**: chips fell one folder at a time. It showed scale, but nothing to touch.
- **Merge**: copies slid into their source chip. It explained copies but hid the other states.
- **Sweep**: chips moved away from the cursor. The reaction to the cursor was the best part, and
  Throw keeps it by grabbing, but sweeping alone had no end state.
- **Onboarding first** (Matt's centred layout, `/?v=next`): steps before the product put the
  product far below the fold. The product mock now sits directly under the hero and How it works
  follows it.
- A caption under the pile ("92 skills in 11 folders… throw it at the mascot"): removed. The
  counts stay in a visually hidden live region for screen readers.
