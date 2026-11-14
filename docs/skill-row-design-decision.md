# Skill row

## 2026-11-13: one row, one interaction model (supersedes parts of 2026-09-12)

Decided from a row-state prototype (Current, Slot, Rail, Tag, Status column) and a
published interaction mock. The prototype is deleted. This section replaces "The leading glyph
is the action" and "Colour and icons carry state; words do not" below.

### Layout

- Home and Skills use one grid: slot 14px · Skill name · Location 160px · Harnesses 148px ·
  Note 240px · Fix 88px, with `gap-x-3 px-3`. The group label starts at the same x as the
  skill name.
- **Slot** (14px): the state icon, or nothing for a healthy skill. On hover, focus or
  selection the checkbox replaces the icon in the same place. While an action runs, a spinner
  takes the slot.
- **Note** (240px): the reason in words on the left (`Broken link · Codex`, `2 days ago`), the
  token pair on the right. Colour and icon carry the level; the note says why.
- **Fix** (88px): one named button for the row's one fix (`Fix`, `Update`, `Compare`,
  `Unpark`, `Park`, `Resolve`). It is quiet text at rest and a button on hover or focus.

### Interaction

- Click the row: open the skill. Same on both pages.
- Click the fix: run the fix. Same on both pages.
- Click the slot, or press X or Space: select. The selection bar at the bottom is the place
  for every other action. One skill selected: its fix, Open, Park or Unpark, Remove. Several
  selected: Update n, Park n, Unpark n, Remove n.
- Keys: ↑ ↓ move, Enter opens, X or Space selects, Esc clears, Tab reaches the checkbox and
  then the fix.
- Selection is per skill. A skill that shows in two Home groups is checked in both.

### Rejected

- **Current** (glyph or empty space): a healthy row leaves a gap at the front, and the name
  column moves with the state.
- **Rail** (a colour bar at the left edge): colour alone carries the state, which fails WCAG
  1.4.1.
- **Tag** (an inline chip after the name): it moves with the name length, so the eye cannot
  scan one column for state.
- **Status column** (a labelled column): about 112px for words the note column can hold.
- **The icon as a menu**: it fights the checkbox for the same place, and the menu hides the
  name of the fix behind a click. A named button costs the same click and says what it does.
- **A per-row ⋯ menu**: the same actions as the selection bar, in a second place.
- **Tooltips on the state icon**: the note column shows the words.

## 2026-09-12: decided from the `PROTO:skill-row` picker

The Skills list row and the Home inbox row share one design. It was chosen from an in-place
prototype picker over eleven rounds; the picker and its variants are deleted.

### Direction: the Stack row

- Columns: glyph hit area · Skill name · Location · Harnesses · Prompt / Full tokens. Rows are
  32px (`h-8`) - denser, Linear-like list. No Description column.
- The leading glyph is the action: the row's one state icon (update, trial, broken, drift,
  invocation) with a menu on hover. Colour and icons carry state; words do not.
- Location shows the Global chip and up to two project chips, then `+N`.
- Harnesses is an overlapping avatar-group stack: the Universal folder disc first when present,
  then one disc per reached harness up to five, then a `+N` disc.
- Harness disc badges: a small link glyph when the harness reaches the skill only through the
  Universal folder; a red broken-link glyph when that link is broken.
- Disabled treatment: **Muted**. A disabled or parked disc keeps its shape and dims the glyph to
  `--color-icon-muted` at `opacity-60`. Parked marks every disc and dims the whole row; disabled
  marks one disc.

### Tooltips

Every harness disc has a one-line tooltip because an 11px icon cannot show how it reaches the
skill: `<glyph> <harness> · <how>[ · <state>]`.

- how: `linked to Universal folder` / `source in Universal folder` / `own copy`; a broken link
  replaces it with `link broken` in the error tone.
- state: `disabled` or `parked` in the warning tone, appended after the how-word.
- The Universal disc says `Universal folder`. The `+N` disc lists the hidden harnesses.
- Location: `+N` lists the hidden projects; a project chip has a tooltip with its full name
  only when it is truncated. The Global chip never has one.
- The invocation glyph says `You run it` or `The model runs it`. Nothing else in the row has a
  tooltip.

### Rejected

- Per-harness invocation tables and frontmatter-key captions in tooltips: a dictionary, not a
  row.
- A Description column: the name column needs the width more.
- Native `title` attributes: inconsistent look and delay.
- Six- and ten-harness toggles, "linked by" lists on the Universal disc, verbose disabled
  reasons, "N copies", "own copy" next to the Global chip: restate what the row already shows.
- Path tooltips (mono path only) and Map tooltips (whole-stack grid): "people know a global
  skill is at the home directory"; they spell out the obvious.
- Exception-only tooltips (nothing on a healthy disc): went too far, the icons are too small to
  tell symlinked from own copy.
- Gallery caption headers: the list must look like the end product.
- Disabled treatments Slash (thin diagonal line) and Dashed (dashed outline): the line alone
  did not read as disabled once the glyph stayed full tone; Muted is enough on its own.
- Earlier row variants Ghost, Reach, Bare, Lead: replaced by the Stack row in earlier rounds.

### Open

- Read-only copies (plugin cache, read-only own copy) have no mark in the row; the detail view
  carries that.
