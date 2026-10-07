import { describe, expect, it } from "vitest";
import type { InstalledSkill } from "@skill-studio/lib";
import { skillSourceLine } from "./skill-source-line";

function skill(overrides: Partial<InstalledSkill>): InstalledSkill {
  // SAFETY: the line reads only name, source, source_kind, source_url, and deployments, all set here.
  return {
    name: "write-tests",
    source: "obra/write-tests",
    source_kind: "skills-sh",
    source_type: "github",
    source_url: "https://github.com/obra/write-tests",
    deployments: [],
    ...overrides,
  } as InstalledSkill;
}

describe("skillSourceLine", () => {
  // Flow: a skills.sh skill whose count is cached.
  // Expectation: owner/repo links to the lock file's source_url, count abbreviated like Browse.
  // A failure means the line shows the wrong maker, a dead link, or an unabbreviated count.
  it("shows the owner/repo, its link, and the abbreviated install count", () => {
    expect(skillSourceLine(skill({}), 12_345)).toEqual({
      prefix: "by ",
      label: "obra/write-tests",
      href: "https://github.com/obra/write-tests",
      installs: "12.3k installs",
    });
  });

  // Flow: the count has not loaded, or the machine is offline.
  // Expectation: the maker still shows; the count is absent, not "0 installs".
  // A failure means offline use shows a fake zero.
  it("omits the count while it is unknown", () => {
    expect(skillSourceLine(skill({}), null).installs).toBeNull();
    expect(skillSourceLine(skill({}), undefined).installs).toBeNull();
  });

  // Flow: a skill with one install.
  // Expectation: singular noun. A failure means "1 installs".
  it("uses the singular for one install", () => {
    expect(skillSourceLine(skill({}), 1).installs).toBe("1 install");
  });

  // Flow: a lock file with no source_url.
  // Expectation: the link falls back to the GitHub page of owner/repo.
  // A failure means the maker is shown with no way to open it.
  it("links to GitHub when the lock file has no source url", () => {
    expect(skillSourceLine(skill({ source_url: null }), 5).href).toBe(
      "https://github.com/obra/write-tests",
    );
  });

  // Flow: an in-repo skill, passed a count anyway.
  // Expectation: its existing source label, no link, no count.
  // A failure means a local skill claims skills.sh numbers.
  it("shows only the existing source label for non-skills.sh sources", () => {
    const line = skillSourceLine(skill({ source_kind: "in-repo", source: "local" }), 99);
    expect(line).toEqual({ prefix: "", label: "Local repository", href: null, installs: null });
  });

  // Flow: a skill installed from a git URL source.
  // Expectation: the URL is not shown as a maker; the existing label shows, no link, no count.
  // A failure means a private git URL is printed as "by https://...".
  it("does not show a git URL source as the maker", () => {
    const line = skillSourceLine(
      skill({ source_type: "git", source: "https://git.example.com/team/repo.git" }),
      42,
    );
    expect(line.prefix).toBe("");
    expect(line.href).toBeNull();
    expect(line.installs).toBeNull();
  });

  // Flow: a skill installed from a local path source.
  // Expectation: same as a git URL: no maker, no count.
  it("does not show a local source as the maker", () => {
    const line = skillSourceLine(skill({ source_type: "local", source: "/Users/me/skills" }), 42);
    expect(line.prefix).toBe("");
    expect(line.installs).toBeNull();
  });

  // Flow: a lock file whose source_url ends in .git.
  // Expectation: the link drops the suffix, so it opens the repo page.
  it("strips a trailing .git from the link", () => {
    expect(
      skillSourceLine(skill({ source_url: "https://github.com/obra/write-tests.git" }), 1).href,
    ).toBe("https://github.com/obra/write-tests");
  });
});
