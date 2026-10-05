// ============================================================================
// skill-scope-model tests
// Guards the cross-scope warning: a live copy in the other scope is reported,
// a parked copy or a same-scope copy is not.
// ============================================================================

import { describe, expect, it } from "vitest";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { otherScopeNote, scopeMarker, scopePresence } from "./skill-scope-model";

function dep(scope: string, projectPath?: string): Deployment {
  // SAFETY: the model reads only `scope` and `project_path`.
  return { scope, project_path: projectPath ?? null } as Deployment;
}

function skill(name: string, deployments: Deployment[]): InstalledSkill {
  // SAFETY: the model reads only `name` and `deployments`.
  return { name, deployments } as InstalledSkill;
}

describe("otherScopeNote", () => {
  it("warns a project install when a global copy exists", () => {
    const skills = [skill("tidy", [dep("global")])];
    expect(otherScopeNote(skills, "tidy", "project", "/work/app")).toBe(
      "Already installed globally. Installing here adds a second copy for this project.",
    );
  });

  it("stays quiet for a project install when only another project has it", () => {
    const skills = [skill("tidy", [dep("project", "/work/other")])];
    expect(otherScopeNote(skills, "tidy", "project", "/work/app")).toBeNull();
  });

  it("warns a global install when a project has it, naming the project", () => {
    const skills = [skill("tidy", [dep("project", "/work/app")])];
    expect(otherScopeNote(skills, "tidy", "global", null)).toBe(
      "Already in app. A global copy will apply to every project.",
    );
  });

  it("names two projects when a global copy also exists", () => {
    const skills = [
      skill("tidy", [dep("global"), dep("project", "/a/one"), dep("project", "/b/two")]),
    ];
    expect(otherScopeNote(skills, "tidy", "global", null)).toBe(
      "Already in one and two. A global copy will apply to every project.",
    );
  });

  it("stays quiet for a global-only skill installed globally and for an unknown name", () => {
    const skills = [skill("tidy", [dep("global")])];
    expect(otherScopeNote(skills, "tidy", "global", null)).toBeNull();
    expect(otherScopeNote(skills, "other", "project", "/work/app")).toBeNull();
  });

  it("ignores a parked copy", () => {
    const skills = [skill("tidy", [dep("parked")])];
    expect(otherScopeNote(skills, "tidy", "project", "/work/app")).toBeNull();
  });
});

describe("scopeMarker", () => {
  const both = scopePresence([dep("global"), dep("project", "/a/one"), dep("project", "/b/two")]);

  it("marks the global group with the project count and a project group as also global", () => {
    expect(scopeMarker(both, { isGlobal: true })).toBe("Also in 2 projects");
    expect(scopeMarker(both, { isGlobal: false, projectPath: "/a/one" })).toBe("Also global");
  });

  it("uses the singular for one project", () => {
    const one = scopePresence([dep("global"), dep("project", "/a/one")]);
    expect(scopeMarker(one, { isGlobal: true })).toBe("Also in 1 project");
  });

  it("marks nothing for global only or project only", () => {
    expect(scopeMarker(scopePresence([dep("global")]), { isGlobal: true })).toBeNull();
    const projectOnly = scopePresence([dep("project", "/a/one")]);
    expect(scopeMarker(projectOnly, { isGlobal: false, projectPath: "/a/one" })).toBeNull();
  });

  it("does not count a parked copy", () => {
    const parked = scopePresence([dep("global"), dep("parked", "/a/one")]);
    expect(scopeMarker(parked, { isGlobal: true })).toBeNull();
  });
});
