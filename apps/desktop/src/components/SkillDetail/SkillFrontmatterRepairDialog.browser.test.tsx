// ============================================================================
// SkillFrontmatterRepairDialog.browser.test - layout the DOM tests cannot see.
// ============================================================================

import { expect, it } from "vitest";
import { page } from "vitest/browser";
import { render } from "vitest-browser-react";
import type { FrontmatterRepairPreview, LifecycleTarget } from "@skill-studio/lib";
import "../../App.css";
import { SkillFrontmatterRepairDialog } from "./SkillFrontmatterRepairDialog";

const ORIGINAL = ["---", "name: find-bugs", "description: Find bugs: leaks and races", "---"].join(
  "\n",
);

const PREVIEW: FrontmatterRepairPreview = {
  deployment_id: "dep:v1/global/universal/find-bugs",
  path: "/home/.agents/skills/find-bugs/SKILL.md",
  scope: "global",
  reason: "The description contains a colon followed by a space.",
  kind: "colon-scalar",
  choice: null,
  expected_content_fingerprint: "fingerprint",
  proposal_id: "proposal",
  original_content: ORIGINAL,
  proposed_content: ORIGINAL.replace("Find bugs: leaks and races", '"Find bugs: leaks and races"'),
  allowed_apply_modes: ["apply-fix"],
};

const TARGET: LifecycleTarget = {};

it("opening_the_YAML_fix_preview_at_1280px_makes_the_dialog_wider_than_700px_so_a_long_description_line_is_not_squeezed_into_the_384px_base_width", async () => {
  await render(
    <SkillFrontmatterRepairDialog
      target={TARGET}
      preview={PREVIEW}
      onClose={() => {}}
      onApplied={() => {}}
      onEditManually={() => {}}
    />,
  );

  const dialog = page.getByRole("dialog");
  await expect.element(dialog).toBeVisible();
  const { width } = dialog.element().getBoundingClientRect();
  expect(width).toBeGreaterThan(700);
});
