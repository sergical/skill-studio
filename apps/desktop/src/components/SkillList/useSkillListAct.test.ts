// ============================================================================
// Skill Studio - useSkillListAct tests: fix outcomes and acts held in place
// ============================================================================

import { afterEach, describe, expect, it, vi } from "vitest";
import type { InstalledSkill } from "@skill-studio/lib";

import { useAppStore } from "../../store/appStore";
import { actInPlace, pastTenseOf, reportFixOutcome, type FixSkillDeps } from "./useSkillListAct";

function fixtureSkill(overrides: Partial<InstalledSkill> = {}): InstalledSkill {
  return {
    name: "find-bugs",
    source: "getsentry/find-bugs",
    source_type: "github",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "dotagents",
    deployments: [],
    has_spec: true,
    spec_violations: [],
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
    ...overrides,
  };
}

describe("reportFixOutcome", () => {
  it("calls the fixSkill wrapper for a skill whose only issue is invalid YAML, and toasts success once applied", async () => {
    const skill = fixtureSkill({
      spec_violations: ["invalid YAML frontmatter at line 3, column 1: mapping values not allowed"],
    });
    const fixSkill = vi.fn().mockResolvedValue({
      skill: skill.name,
      applied: [{ kind: "frontmatter_repair", deployment_id: "dep:1", event_id: "evt:1" }],
      unrepaired: [],
      conflicts: [],
    });
    const openConflictPaths = vi.fn();
    const addToast = vi.fn();
    const openDetail = vi.fn();
    const deps: FixSkillDeps = { fixSkill, openConflictPaths };

    await reportFixOutcome(skill, addToast, openDetail, deps);

    expect(fixSkill).toHaveBeenCalledWith("find-bugs");
    expect(addToast).toHaveBeenCalledWith(
      expect.objectContaining({ type: "success", title: "Fixed find-bugs" }),
    );
    expect(openDetail).not.toHaveBeenCalled();
  });

  it("falls through to the detail page's own repair card when the only unrepaired issue is a dangling link", async () => {
    const skill = fixtureSkill();
    const fixSkill = vi.fn().mockResolvedValue({
      skill: skill.name,
      applied: [],
      unrepaired: [
        {
          path: "/home/.claude/skills/find-bugs",
          message: "/home/.claude/skills/find-bugs links to a missing target",
          kind: "link",
        },
      ],
      conflicts: [],
    });
    const openConflictPaths = vi.fn();
    const addToast = vi.fn();
    const openDetail = vi.fn();
    const deps: FixSkillDeps = { fixSkill, openConflictPaths };

    await reportFixOutcome(skill, addToast, openDetail, deps);

    expect(openDetail).toHaveBeenCalledOnce();
    expect(addToast).not.toHaveBeenCalled();
  });

  it("falls through to the detail page and shows no error toast when the only unrepaired issue is a missing description, or names the toast that would have stranded the user", async () => {
    const skill = fixtureSkill({
      spec_violations: ["missing required frontmatter field: description"],
    });
    const fixSkill = vi.fn().mockResolvedValue({
      skill: skill.name,
      applied: [],
      unrepaired: [
        {
          path: "/home/.claude/skills/find-bugs/SKILL.md",
          message: "missing required frontmatter field: description",
          kind: "frontmatter",
        },
      ],
      conflicts: [],
    });
    const openConflictPaths = vi.fn();
    const addToast = vi.fn();
    const openDetail = vi.fn();
    const deps: FixSkillDeps = { fixSkill, openConflictPaths };

    await reportFixOutcome(skill, addToast, openDetail, deps);

    expect(openDetail).toHaveBeenCalledOnce();
    expect(addToast).not.toHaveBeenCalled();
  });
});

describe("actInPlace", () => {
  afterEach(() => useAppStore.getState().clearRowActs());
  const acts = () => useAppStore.getState().rowActs;

  it("holds the row while the act runs and leaves its past tense after, or the row would jump back mid-act", async () => {
    let finish!: (ok: boolean) => void;
    const running = actInPlace(
      ["a"],
      "Updated",
      () => new Promise((resolve) => (finish = resolve)),
    );
    expect(acts().get("a")).toBeNull();
    finish(true);
    expect(await running).toBe(true);
    expect(acts().get("a")).toBe("Updated");
  });

  it("releases the row when the act fails or throws, or a failed row would claim it was done", async () => {
    expect(await actInPlace(["a"], "Parked", async () => false)).toBe(false);
    expect(await actInPlace(["b"], "Parked", () => Promise.reject(new Error("boom")))).toBe(false);
    expect(acts().has("a") || acts().has("b")).toBe(false);
  });

  it("drops a second act on a row that is still running, or two updates would race on one folder", async () => {
    let finish!: (ok: boolean) => void;
    const first = actInPlace(["a"], "Updated", () => new Promise((resolve) => (finish = resolve)));
    const second = vi.fn(async () => true);
    expect(await actInPlace(["a", "b"], "Parked", second)).toBe(false);
    expect(second).not.toHaveBeenCalled();
    expect(acts().has("b")).toBe(false);
    finish(true);
    await first;
  });

  it("keeps a running act and drops a finished one when the view changes, or a done row would say Updated forever", async () => {
    await actInPlace(["done"], "Updated", async () => true);
    void actInPlace(["busy"], "Updated", () => new Promise(() => {}));
    useAppStore.getState().setActiveView({ kind: "skills" });
    expect([...acts()]).toEqual([["busy", null]]);
  });

  it("names the past tense only for verbs that act in place, so Open and Compare never hold a row", () => {
    expect(["Park", "Unpark", "Fix", "Update"].map(pastTenseOf)).toEqual([
      "Parked",
      "Unparked",
      "Fixed",
      "Updated",
    ]);
    expect(pastTenseOf("Compare")).toBeNull();
    expect(pastTenseOf("Open")).toBeNull();
  });
});
