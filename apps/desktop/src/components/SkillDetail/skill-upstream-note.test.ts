import { describe, expect, it } from "vitest";
import type { Deployment, InstalledSkill, UpstreamAhead } from "@skill-studio/lib";
import { skillUpstreamNote } from "./skill-upstream-note";

const OWNER_ID = "owner:v1/global/tdd";

function skill(
  overrides: { source_kind?: InstalledSkill["source_kind"]; owners?: Array<string | null> } = {},
): InstalledSkill {
  const { source_kind = "dotagents", owners = [OWNER_ID] } = overrides;
  // SAFETY: the note reads only source_kind and deployments[].owner_id, both set here.
  return {
    name: "tdd",
    source: "local",
    source_kind,
    deployments: owners.map((owner_id) => ({ owner_id })),
  } as InstalledSkill;
}

/** The note for the skill's first deployment, the one the page shows by default. */
function noteFor(installed: InstalledSkill, records: UpstreamAhead[]) {
  return skillUpstreamNote(installed, installed.deployments[0], records);
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
    expect(noteFor(skill({}), [ahead({})])).toEqual({
      text: "Forked from mattpocock/skills, which has 12 changes this fork doesn't.",
      href: "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main",
    });
  });

  // Flow: the original is exactly one commit ahead.
  // Expectation: "1 change", not "1 changes".
  it("uses the singular for one change", () => {
    expect(noteFor(skill({}), [ahead({ behind_by: 1 })])?.text).toBe(
      "Forked from mattpocock/skills, which has 1 change this fork doesn't.",
    );
  });

  // Flow: no deployment of the skill is owned by an owner in the record, or no records exist.
  // Expectation: no note, so a skill from a plain repo never shows a fork line.
  it("returns nothing when no deployment owner is in a record", () => {
    expect(noteFor(skill({ owners: ["owner:v1/global/other"] }), [ahead({})])).toBeNull();
    expect(noteFor(skill({ owners: [null] }), [ahead({})])).toBeNull();
    expect(noteFor(skill({}), [])).toBeNull();
  });

  // Flow: a global copy from one fork and a project copy of the same name from another; only
  // the global copy's fork is behind.
  // Expectation: the global copy's page shows its note; the project copy's page shows none, and
  // when both forks are behind, each page names its own fork.
  // A failure means the page shows another copy's fork, count, and compare link.
  it("matches only the deployment the page shows", () => {
    const PROJECT_OWNER = "owner:v1/project/%2Fwork%2Fapp/tdd";
    const both = skill({ owners: [OWNER_ID, PROJECT_OWNER] });
    const [global, project] = both.deployments as [Deployment, Deployment];
    expect(skillUpstreamNote(both, global, [ahead({})])).not.toBeNull();
    expect(skillUpstreamNote(both, project, [ahead({})])).toBeNull();
    const other = ahead({ upstream_repo: "someone/skills", owner_ids: [PROJECT_OWNER] });
    expect(skillUpstreamNote(both, project, [ahead({}), other])?.text).toContain("someone/skills");
    expect(skillUpstreamNote(both, undefined, [ahead({})])).toBeNull();
  });

  // Flow: a plugin or manual skill whose deployment owner id happens to be in a record.
  // Expectation: no note, because only dotagents and skills.sh installs follow a fork.
  it("returns nothing for a skill that is not from dotagents or skills.sh", () => {
    expect(noteFor(skill({ source_kind: "plugin" }), [ahead({})])).toBeNull();
  });

  // Flow: a skills.sh install from a fork.
  // Expectation: the note shows, same as for dotagents.
  it("shows the note for a skills.sh install", () => {
    expect(noteFor(skill({ source_kind: "skills-sh" }), [ahead({})])).not.toBeNull();
  });
});
