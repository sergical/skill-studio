// ============================================================================
// SkillMarkdownEditor.browser.test - the red mark on a broken line, in real layout.
// ============================================================================

import { expect, it } from "vitest";
import { page } from "vitest/browser";
import { render } from "vitest-browser-react";
import "../../App.css";
import { SkillMarkdownEditor } from "./SkillMarkdownEditor";

const LONG_DESCRIPTION =
  "Fix a reported issue from a GitHub issue. Use when the user provides a GitHub issue URL and asks to fix a bug, investigate an issue, or reproduce a problem. Handles the full workflow: fetching the issue, finding the reproduction, writing a failing test, and implementing the fix.";
const CONTENT = [
  "---",
  "name: fix-issue",
  `description: ${LONG_DESCRIPTION}`,
  "---",
  "",
  "# Fix issue",
].join("\n");

it("opening_the_editor_on_a_broken_wrapped_line_paints_a_visible_red_band_over_every_row_of_that_line_so_the_user_sees_what_is_broken", async () => {
  await render(
    <div style={{ width: 760 }}>
      <SkillMarkdownEditor
        initialContent={CONTENT}
        isSaving={false}
        onSave={() => {}}
        onCancel={() => {}}
        highlightLine={3}
      />
    </div>,
  );

  const textbox = page.getByRole("textbox");
  await expect.element(textbox).toBeVisible();
  const textarea = textbox.element();
  if (!(textarea instanceof HTMLTextAreaElement)) throw new Error("the editor is not a textarea");
  // An opaque textarea background would hide the band painted behind it.
  expect(getComputedStyle(textarea).backgroundColor).toMatch(/rgba\(0, 0, 0, 0\)|transparent/);

  const band = page.getByTestId("yaml-error-band").element();
  const bandStyle = getComputedStyle(band);
  // The band must paint: an opaque-enough fill and the red left border.
  expect(bandStyle.backgroundColor).not.toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
  expect(Number(bandStyle.opacity)).toBeGreaterThan(0.5);
  expect(bandStyle.borderLeftColor).not.toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
  expect(Number.parseFloat(bandStyle.borderLeftWidth)).toBeGreaterThan(0);

  const rowHeight = Number.parseFloat(getComputedStyle(textarea).lineHeight);
  const bandBox = band.getBoundingClientRect();
  // The description wraps at 760px, so its band covers more than one row.
  expect(bandBox.height).toBeGreaterThan(rowHeight * 1.5);
  // Line 3's number in the gutter starts the band's row.
  const gutterLine = page.getByText("3", { exact: true }).element();
  expect(Math.abs(bandBox.top - gutterLine.getBoundingClientRect().top)).toBeLessThan(1);
});
