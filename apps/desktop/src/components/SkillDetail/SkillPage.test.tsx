// @vitest-environment happy-dom

// ============================================================================
// SkillPage.test - the two ways into the SKILL.md editor from a skill whose
// frontmatter is not valid YAML: both open on the broken line.
// ============================================================================

import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Deployment, FrontmatterRepairPreview, InstalledSkill } from "@skill-studio/lib";
import { CommandPalette } from "../CommandPalette/CommandPalette";
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
    if (command.startsWith("lab_")) return [];
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
    localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    localStorage.clear();
  });

  it("clicking_Edit_on_a_skill_with_broken_YAML_marks_the_error_line_in_red_and_puts_the_caret_on_it_so_the_user_lands_on_the_mistake_instead_of_the_top_of_the_file", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));

    const editor = await openedEditor();
    const { start } = lineOffsets(BROKEN_SKILL_MD, 3);
    await waitFor(() => expect(editor.selectionStart).toBe(start));
    expect(editor.selectionEnd).toBe(start);
    expect(screen.getByText("Line 3 breaks the YAML frontmatter")).toBeTruthy();
  });

  it("clicking_Edit_manually_in_the_YAML_fix_dialog_marks_the_error_line_in_red_and_puts_the_caret_on_it_so_the_user_lands_on_the_mistake_instead_of_the_top_of_the_file", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Fix" }));
    await userEvent.click(await screen.findByRole("button", { name: "Edit manually" }));

    const editor = await openedEditor();
    const { start } = lineOffsets(BROKEN_SKILL_MD, 3);
    await waitFor(() => expect(editor.selectionStart).toBe(start));
    expect(screen.getByText("Line 3 breaks the YAML frontmatter")).toBeTruthy();
  });

  it("typing_on_the_marked_line_clears_the_red_mark_so_a_line_the_user_is_fixing_is_not_still_flagged_as_broken", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    const editor = await openedEditor();
    await waitFor(() => expect(editor.selectionStart).toBe(lineOffsets(BROKEN_SKILL_MD, 3).start));

    await userEvent.type(editor, "x", { skipClick: true });

    expect(screen.queryByText("Line 3 breaks the YAML frontmatter")).toBeNull();
  });

  it("fixing_the_YAML_on_another_frontmatter_line_clears_the_red_mark_so_a_fixed_file_is_not_still_flagged_as_broken", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    const editor = await openedEditor();
    await waitFor(() => expect(editor.selectionStart).toBe(lineOffsets(BROKEN_SKILL_MD, 3).start));
    const lineTwoEnd = lineOffsets(BROKEN_SKILL_MD, 2).end;
    editor.setSelectionRange(lineTwoEnd, lineTwoEnd);

    await userEvent.type(editor, "x", { skipClick: true, initialSelectionStart: lineTwoEnd });

    expect(editor.value.split("\n")[1]).toBe("name: find-bugsx");
    expect(screen.queryByText("Line 3 breaks the YAML frontmatter")).toBeNull();
  });

  it("editing_the_body_keeps_the_red_mark_and_the_textarea_names_the_error_so_a_screen_reader_user_hears_which_line_is_broken", async () => {
    renderPage(BROKEN_SKILL_MD, [YAML_VIOLATION]);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    const editor = await openedEditor();
    await waitFor(() => expect(editor.selectionStart).toBe(lineOffsets(BROKEN_SKILL_MD, 3).start));
    const end = BROKEN_SKILL_MD.length;
    editor.setSelectionRange(end, end);

    await userEvent.type(editor, "x", { skipClick: true, initialSelectionStart: end });

    expect(editor.value.endsWith("# Find bugsx")).toBe(true);
    expect(editor.getAttribute("aria-invalid")).toBe("true");
    expect(editor.getAttribute("aria-describedby")).toBe(
      screen.getByText("Line 3 breaks the YAML frontmatter").id,
    );
  });

  it("clicking_Edit_on_a_skill_without_a_YAML_error_shows_no_red_mark_so_the_editor_does_not_flag_a_line_that_is_fine", async () => {
    renderPage(CLEAN_SKILL_MD, []);
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));

    await openedEditor();
    expect(screen.queryByText(/breaks the YAML frontmatter/)).toBeNull();
  });
  it("dirty editor Back Discard closes the editor before navigation resumes instead of leaving stale edits open", async () => {
    const user = userEvent.setup();
    useAppStore.getState().openSkill("find-bugs");
    renderPage(CLEAN_SKILL_MD, []);
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    await user.type(await openedEditor(), " changed");
    act(() => useAppStore.getState().goBack());
    expect(await screen.findByRole("alertdialog")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
    expect(useAppStore.getState().activeView.kind).toBe("home");
  });
  it("Back after the default copy changes shows the original SKILL.md instead of another copy", async () => {
    const user = userEvent.setup();
    const first = fixtureDeployment();
    const second = fixtureDeployment({
      id: "second",
      path: "/home/.codex/skills/find-bugs",
      agent: "codex",
    });
    const skill = { ...fixtureSkill(first), deployments: [first, second] };
    mockIPC((command, args) => {
      if (command === "read_installed_skill_md")
        return args && "path" in args && args.path === `${first.path}/SKILL.md`
          ? "# Original copy"
          : "# Other copy";
      throw new Error(`unexpected command ${command}`);
    });
    useAppStore
      .getState()
      .setKnownSkillNames(new Set([skill.name]), new Map([[skill.name, first.path]]));
    useAppStore.getState().openSkill(skill.name);
    function Page() {
      const view = useAppStore((state) => state.activeView);
      return (
        <>
          {view.kind === "skill" && view.name === skill.name ? (
            <SkillPage
              skill={skill}
              deploymentPath={view.deploymentPath}
              onBack={() => useAppStore.getState().closeSkill()}
              onRemoveComplete={() => {}}
              from={{ kind: "home" }}
            />
          ) : (
            <p>Home page</p>
          )}
          <CommandPalette snapshot={undefined} requestRescan={async () => {}} />
        </>
      );
    }
    render(<Page />);
    expect(await screen.findByRole("heading", { name: "Original copy" })).toBeTruthy();
    act(() => useAppStore.getState().openSkill("other"));
    act(() =>
      useAppStore
        .getState()
        .setKnownSkillNames(new Set([skill.name]), new Map([[skill.name, second.path]])),
    );
    skill.deployments = [second, first];
    act(() => useAppStore.getState().setCommandPaletteOpen(true));
    await user.type(await screen.findByRole("combobox"), "Back");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Original copy" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Other copy" })).toBeNull();
  });

  it("has one flagged Assistant entry that opens a page and returns to the same skill", async () => {
    localStorage.setItem("feature:skill-assistant", "on");
    const called = vi.fn();
    mockIPC((command) => {
      called(command);
      if (command === "read_installed_skill_md") return CLEAN_SKILL_MD;
      if (command === "lab_clients") return [{ id: "codex", available: true }];
      return [];
    });
    const props = {
      skill: fixtureSkill(fixtureDeployment()),
      onBack: vi.fn(),
      onRemoveComplete: vi.fn(),
      from: { kind: "home" as const },
    };
    const page = render(<SkillPage {...props} />);
    const user = userEvent.setup();
    const assistant = screen.getByRole("button", { name: "Assistant" });
    expect(screen.queryByRole("button", { name: "Test skill" })).toBeNull();
    expect(called.mock.calls.some(([command]) => command.startsWith("lab_"))).toBe(false);
    await user.click(assistant);
    await user.type(await screen.findByLabelText("Task"), "Check a real change");
    expect(screen.getByRole("region", { name: "Assistant for find-bugs" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("button", { name: "Assistant" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "find-bugs" }));
    await waitFor(() => expect(document.activeElement).toBe(assistant));
    await user.click(assistant);
    expect(screen.getByLabelText<HTMLTextAreaElement>("Task").value).toBe("Check a real change");
    page.rerender(<SkillPage {...props} deploymentPath="/removed/copy" />);
    expect(screen.queryByRole("region", { name: "Assistant for find-bugs" })).toBeNull();
    expect(screen.getByRole("button", { name: "Home" })).toBeTruthy();
    localStorage.setItem("feature:skill-assistant", "off");
    page.unmount();
    called.mockClear();
    render(<SkillPage {...props} />);
    expect(screen.queryByRole("region", { name: "Assistant for find-bugs" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Assistant" })).toBeNull();
    expect(called.mock.calls.some(([command]) => command.startsWith("lab_"))).toBe(false);
  });

  it("returns from Assistant with Escape without discarding the skill editor draft", async () => {
    localStorage.setItem("feature:skill-assistant", "on");
    renderPage(CLEAN_SKILL_MD, []);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const editor = await openedEditor();
    await user.clear(editor);
    await user.type(editor, "Unsaved skill edit");
    await user.click(screen.getByRole("button", { name: "Assistant" }));
    await screen.findByLabelText("Task");
    await user.click(screen.getByRole("button", { name: "Task" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect((await openedEditor()).value).toBe("Unsaved skill edit");
  });
});
