import { describe, expect, it } from "vitest";
import type { Deployment, UpstreamAhead } from "@skill-studio/lib";
import { skillUpstreamNote } from "./skill-upstream-note";

const OWNER_ID = "owner:v1/global/tdd";

function deployment(
  overrides: { owner_kind?: Deployment["owner_kind"]; owner_id?: string | null } = {},
): Deployment {
  const { owner_kind = "dotagents", owner_id = OWNER_ID } = overrides;
  // SAFETY: the note reads only owner_kind and owner_id, both set here.
  return { owner_kind, owner_id } as Deployment;
}

function ahead(overrides: Partial<UpstreamAhead>): UpstreamAhead {
  return {
    repo: "sergical/mattpocock-skills",
    upstream_repo: "mattpocock/skills",
    behind_by: 12,
    compare_url:
      "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main",
    owner_ids: [OWNER_ID],
    ...overrides,
  };
}

describe("skillUpstreamNote", () => {
  // Flow: a dotagents-only install (no skills.sh lock entry, so source is "local") from a fork
  // whose original is 12 commits ahead.
  // Expectation: the line names the original with the count and links to the compare page.
  // A failure means the user sees no note, the wrong repo, a wrong count, or a dead link.
  it("names the original repo, the change count, and the compare link", () => {
    expect(skillUpstreamNote(deployment(), [ahead({})])).toEqual({
      text: "Forked from mattpocock/skills, which has 12 changes this fork doesn't.",
      href: "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main",
    });
  });

  // Flow: the original is exactly one commit ahead.
  // Expectation: "1 change", not "1 changes".
  it("uses the singular for one change", () => {
    expect(skillUpstreamNote(deployment(), [ahead({ behind_by: 1 })])?.text).toBe(
      "Forked from mattpocock/skills, which has 1 change this fork doesn't.",
    );
  });

  // Flow: the shown copy's owner is in no record, it has no owner, no copy resolved, or no
  // records exist.
  // Expectation: no note, so a skill from a plain repo never shows a fork line.
  it("returns nothing when the shown copy's owner is in no record", () => {
    const other = deployment({ owner_id: "owner:v1/global/other" });
    expect(skillUpstreamNote(other, [ahead({})])).toBeNull();
    expect(skillUpstreamNote(deployment({ owner_id: null }), [ahead({})])).toBeNull();
    expect(skillUpstreamNote(undefined, [ahead({})])).toBeNull();
    expect(skillUpstreamNote(deployment(), [])).toBeNull();
  });

  // Flow: a global copy from one fork and a project copy of the same name from another; only
  // the global copy's fork is behind, then both are.
  // Expectation: each page names only its own copy's fork, and the project page shows none while
  // its fork is current.
  // A failure means the page shows another copy's fork, count, and compare link.
  it("matches only the deployment the page shows", () => {
    const project = deployment({ owner_id: "owner:v1/project/%2Fwork%2Fapp/tdd" });
    expect(skillUpstreamNote(deployment(), [ahead({})])).not.toBeNull();
    expect(skillUpstreamNote(project, [ahead({})])).toBeNull();
    const other = ahead({ upstream_repo: "someone/skills", owner_ids: [project.owner_id!] });
    expect(skillUpstreamNote(project, [ahead({}), other])?.text).toContain("someone/skills");
  });

  // Flow: a copy owned by a plugin, a Skill Studio fork, or a manual copy whose owner id happens
  // to be in a record.
  // Expectation: no note, because only dotagents and skills.sh installs follow a fork.
  it("returns nothing for a copy not installed by dotagents or skills.sh", () => {
    for (const owner_kind of ["plugin", "fork", "copy", "manual"] as const) {
      expect(skillUpstreamNote(deployment({ owner_kind }), [ahead({})])).toBeNull();
    }
  });

  // Flow: a skills.sh or wildcard dotagents install from a fork, whatever the skill-level source
  // kind (a same-named plugin copy can make that "plugin").
  // Expectation: the note shows, same as for dotagents.
  it("shows the note for skills.sh and wildcard dotagents copies", () => {
    for (const owner_kind of ["skills-sh", "wildcard-dotagents"] as const) {
      expect(skillUpstreamNote(deployment({ owner_kind }), [ahead({})])).not.toBeNull();
    }
  });
});
