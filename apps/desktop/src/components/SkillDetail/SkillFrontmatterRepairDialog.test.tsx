// @vitest-environment happy-dom

import { useState } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { FrontmatterRepairPreview } from "@skill-studio/lib";
import { SkillFrontmatterRepairDialog } from "./SkillFrontmatterRepairDialog";

const original =
  "---\nname: sample\ndescription: d\ndisable-model-invocation: true\nuser-invocable:\n  false\n---\nBody.\n";
const initial: FrontmatterRepairPreview = {
  deployment_id: "sample",
  path: "/fixture/sample",
  scope: "global",
  reason: "Choose who can run this skill.",
  kind: "invocation-conflict",
  choice: null,
  expected_content_fingerprint: "original",
  proposal_id: "choose",
  original_content: original,
  proposed_content: original,
  allowed_apply_modes: ["apply-fix"],
};

function Repair() {
  const [open, setOpen] = useState(true);
  const [applied, setApplied] = useState(false);
  return (
    <>
      {open && (
        <SkillFrontmatterRepairDialog
          target={{ deployment_id: "sample" }}
          preview={initial}
          onClose={() => setOpen(false)}
          onApplied={() => setApplied(true)}
          onEditManually={() => {}}
        />
      )}
      {applied && <p>Applied safe repair</p>}
    </>
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
});

it("a refused invocation choice shows its reason and a safe choice can apply or the dialog hides the refusal", async () => {
  const writes: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "preview_skill_frontmatter_repair") {
      if (args && "choice" in args && args.choice === "user-only")
        throw new Error("The user-invocable value is not a single-line scalar");
      return {
        ...initial,
        choice: "model-only",
        proposal_id: "safe-model",
        proposed_content: original.replace("disable-model-invocation: true\n", ""),
      };
    }
    if (command === "apply_skill_frontmatter_repair") {
      writes.push(args);
      return;
    }
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<Repair />);
  await user.click(screen.getByRole("button", { name: "Only you can run it" }));
  expect((await screen.findByRole("alert")).textContent).toContain("not a single-line scalar");
  expect(screen.getByRole("button", { name: "Apply fix" }).hasAttribute("disabled")).toBe(true);
  await user.click(screen.getByRole("button", { name: "Only the agent runs it" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Apply fix" }).hasAttribute("disabled")).toBe(false),
  );
  expect(screen.queryByRole("alert")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Apply fix" }));
  await screen.findByText("Applied safe repair");
  expect(writes).toEqual([
    {
      request: {
        target: { deployment_id: "sample" },
        proposal_id: "safe-model",
        expected_content_fingerprint: "original",
        mode: "apply-fix",
        kind: "invocation-conflict",
        choice: "model-only",
      },
    },
  ]);
  expect(screen.queryByRole("dialog")).toBeNull();
});
