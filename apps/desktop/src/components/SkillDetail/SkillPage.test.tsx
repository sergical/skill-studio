// @vitest-environment happy-dom

// ============================================================================
// SkillPage.test - the two ways into the SKILL.md editor from a skill whose
// frontmatter is not valid YAML: both open on the broken line.
// ============================================================================

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Deployment, FrontmatterRepairPreview, InstalledSkill } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { SkillPage } from "./SkillPage";

const DESCRIPTION_LINE =
  "description: Use when the user asks to find bugs: race conditions, leaks, and off-by-one errors in changed code, then report each with a file and line";
const BROKEN_SKILL_MD = ["---", "name: find-bugs", DESCRIPTION_LINE, "---", "# Find bugs"].join(
  "\n",
);
const CLEAN_SKILL_MD = [
  "---",
  "name: find-bugs",
  "description: Find bugs",
  "---",
  "# Find bugs",
].join("\n");
const YAML_VIOLATION =
  "invalid YAML frontmatter at line 3, column 59: mapping values are not allowed in this context";

function fixtureDeployment(overrides: Partial<Deployment> = {}): Deployment {
  return {
    id: "dep:v1/global/universal/find-bugs",
    destination: "universal",
    owner_kind: "manual",
    mutability: "mutable",
    backing: { kind: "canonical" },
    agent: "shared",
    scope: "global",
    path: "/home/.agents/skills/find-bugs",
    is_symlink: false,
    symlink_is_broken: false,
    content_hash: "abc",
    disabled: false,
    codex_implicit_invocation: null,
    disabled_by: null,
    invocation: "both",
    spec_violations: [],
    shared_via_whole_dir_link: false,
    ...overrides,
  };
}

function fixtureSkill(deployment: Deployment): InstalledSkill {
  return {
    name: "find-bugs",
    source: "",
    source_type: "local",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "manual",
    deployments: [deployment],
    has_spec: true,
    spec_violations: deployment.spec_violations,
    skill_md_tokens: 0,
    description_tokens: 0,
    folder_bytes: 0,
    file_count: 0,
    content_hash: "",
    content_hashes: [],
    frontmatter_fields: {},
    folder_truncated: false,
    parked: false,
    invocation: "both",
    update_owners: [],
    update_owner_ids: [],
    description: null,
    fork: null,
    parked_at: null,
    skill_path: null,
    source_url: null,
    update_commit: null,
    update_commit_at: null,
    updated_at: null,
  };
}

const REPAIR_PREVIEW: FrontmatterRepairPreview = {
  deployment_id: "dep:v1/global/universal/find-bugs",
  path: "/home/.agents/skills/find-bugs/SKILL.md",
  scope: "global",
  reason: "The description contains a colon followed by a space.",
  kind: "colon-scalar",
  choice: null,
  expected_content_fingerprint: "fingerprint",
  proposal_id: "proposal",
  original_content: BROKEN_SKILL_MD,
  proposed_content: BROKEN_SKILL_MD.replace(
    DESCRIPTION_LINE,
    `description: "${DESCRIPTION_LINE.slice("description: ".length)}"`,
  ),
  allowed_apply_modes: ["apply-fix"],
};

function renderPage(skillMd: string, violations: string[]) {
  mockIPC((command) => {
    if (command === "read_installed_skill_md") return skillMd;
    if (command === "preview_skill_frontmatter_repair") return REPAIR_PREVIEW;
    throw new Error(`unexpected command ${command}`);
  });
  const skill = fixtureSkill(fixtureDeployment({ spec_violations: violations }));
  render(
    <SkillPage
      skill={skill}
      onBack={() => {}}
      onRemoveComplete={() => {}}
      from={{ kind: "home" }}
    />,
  );
}

/** The 0-based start and end offsets of 1-based `line` in `content`. */
function lineOffsets(content: string, line: number) {
  const start =
    content
      .split("\n")
      .slice(0, line - 1)
      .join("\n").length + (line > 1 ? 1 : 0);
  return { start, end: start + content.split("\n")[line - 1]!.length };
}

async function openedEditor() {
  const editor = await screen.findByRole("textbox");
  if (!(editor instanceof HTMLTextAreaElement)) throw new Error("the editor is not a textarea");
  return editor;
}

describe("SkillPage editor entry points", () => {
  const initialState = useAppStore.getState();

  beforeEach(() => {
    useAppStore.setState(initialState, true);
  });

  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("clicking_Edit_on_a_skill_with_broken_YAML_selects_the_error_line_so_the_user_lands_on_the_mistake_instead_of_the_top_of_the_file", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));

    const editor = await openedEditor();
    const { start, end } = lineOffsets(BROKEN_SKILL_MD, 3);
    await waitFor(() => expect(editor.selectionStart).toBe(start));
    expect(editor.selectionEnd).toBe(end);
  });

  it("clicking_Edit_manually_in_the_YAML_fix_dialog_selects_the_error_line_so_the_user_lands_on_the_mistake_instead_of_the_top_of_the_file", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Fix" }));
    await userEvent.click(await screen.findByRole("button", { name: "Edit manually" }));

    const editor = await openedEditor();
    const { start, end } = lineOffsets(BROKEN_SKILL_MD, 3);
    await waitFor(() => expect(editor.selectionStart).toBe(start));
    expect(editor.selectionEnd).toBe(end);
  });

  it("clicking_Edit_on_a_skill_without_a_YAML_error_leaves_nothing_selected_so_the_editor_does_not_mark_a_line_that_is_fine", async () => {
    renderPage(CLEAN_SKILL_MD, []);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));

    const editor = await openedEditor();
    expect(editor.selectionStart).toBe(editor.selectionEnd);
  });
});
