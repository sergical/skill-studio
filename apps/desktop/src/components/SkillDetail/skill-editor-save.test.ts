// ============================================================================
// Skill Studio - inline SKILL.md editor save tests
// ============================================================================

import { describe, expect, it, vi } from "vitest";
import { saveSkillEditorDraft } from "./skill-editor-save";

describe("saveSkillEditorDraft", () => {
  it("advances the baseline to the saved draft after a successful save", async () => {
    const writeIfUnchanged = vi.fn(async (): Promise<void> => undefined);

    const saved = await saveSkillEditorDraft(
      {
        path: "/skills/example/SKILL.md",
        openedContent: "content when editor opened",
        draftContent: "saved draft",
      },
      writeIfUnchanged,
    );

    expect(saved).toEqual({
      path: "/skills/example/SKILL.md",
      openedContent: "saved draft",
      draftContent: "saved draft",
    });
  });
});
