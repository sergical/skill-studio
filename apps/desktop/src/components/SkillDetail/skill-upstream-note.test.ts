import { describe, expect, it } from "vitest";
import type { InstalledSkill, UpstreamAhead } from "@skill-studio/lib";
import { skillUpstreamNote } from "./skill-upstream-note";

function skill(overrides: Partial<InstalledSkill>): InstalledSkill {
  // SAFETY: the note reads only source and source_kind, both set here.
  return {
    name: "tdd",
    source: "sergical/mattpocock-skills",
    source_kind: "dotagents",
    ...overrides,
  } as InstalledSkill;
}

function ahead(overrides: Partial<UpstreamAhead>): UpstreamAhead {
  return {
    repo: "sergical/mattpocock-skills",
    upstream_repo: "mattpocock/skills",
    behind_by: 12,
    compare_url:
      "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main",
    ...overrides,
  };
}

describe("skillUpstreamNote", () => {
  // Flow: a dotagents skill installed from a fork whose original is 12 commits ahead.
  // Expectation: the line names the original with the count and links to the compare page.
  // A failure means the user sees the wrong repo, a wrong count, or a dead link.
  it("names the original repo, the change count, and the compare link", () => {
    expect(skillUpstreamNote(skill({}), [ahead({})])).toEqual({
      text: "Forked from mattpocock/skills, which has 12 changes this fork doesn't.",
      href: "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main",
    });
  });

  // Flow: the original is exactly one commit ahead.
  // Expectation: "1 change", not "1 changes".
  it("uses the singular for one change", () => {
    expect(skillUpstreamNote(skill({}), [ahead({ behind_by: 1 })])?.text).toBe(
      "Forked from mattpocock/skills, which has 1 change this fork doesn't.",
    );
  });

  // Flow: the skill comes from a repo with no record, or no records exist at all.
  // Expectation: no note, so a skill from a plain repo never shows a fork line.
  it("returns nothing when no record matches the skill's source", () => {
    expect(skillUpstreamNote(skill({ source: "someone/else" }), [ahead({})])).toBeNull();
    expect(skillUpstreamNote(skill({}), [])).toBeNull();
  });

  // Flow: a plugin or manual skill whose source string happens to equal a recorded repo.
  // Expectation: no note, because only dotagents and skills.sh installs follow a fork.
  it("returns nothing for a skill that is not from dotagents or skills.sh", () => {
    expect(skillUpstreamNote(skill({ source_kind: "plugin" }), [ahead({})])).toBeNull();
  });

  // Flow: skills.sh writes the source in a different letter case than the fork record.
  // Expectation: still matches, since GitHub repo names are case-insensitive.
  it("matches the source repo ignoring letter case", () => {
    expect(
      skillUpstreamNote(skill({ source_kind: "skills-sh", source: "Sergical/Mattpocock-Skills" }), [
        ahead({}),
      ]),
    ).not.toBeNull();
  });

  // Flow: the lock file records the source as a git URL, with or without `git:` and `.git`.
  // Expectation: the note still matches the fork's `owner/repo` record.
  // A failure means a skills.sh skill installed from a URL never shows its fork note.
  it.each([
    "git:https://github.com/sergical/mattpocock-skills.git",
    "https://github.com/sergical/mattpocock-skills",
    "git@github.com:sergical/mattpocock-skills.git",
    "Sergical/Mattpocock-Skills#main",
    "sergical/mattpocock-skills@v2",
  ])("matches the record when the lock source is %s", (source) => {
    expect(skillUpstreamNote(skill({ source }), [ahead({})])?.text).toContain("mattpocock/skills");
  });

  // Flow: a skill whose source is a GitLab URL, with a record for the same owner/repo path.
  // Expectation: no note; the record is about GitHub only.
  it("shows no note for a source that is not on github.com", () => {
    expect(
      skillUpstreamNote(
        skill({ source: "git:https://gitlab.com/sergical/mattpocock-skills.git" }),
        [ahead({})],
      ),
    ).toBeNull();
  });
});
