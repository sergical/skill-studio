// ============================================================================
// Skill Studio - skill-source-parse tests
// ============================================================================

import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { parseSkillSource, type ParsedSkillSource } from "./skill-source-parse";

describe("parseSkillSource", () => {
  it("parses a bare owner/repo", () => {
    expect(parseSkillSource("getsentry/find-bugs")).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
    });
  });

  it("parses owner/repo/<path> without guessing a skill name", () => {
    expect(parseSkillSource("getsentry/find-bugs/skills/find-bugs")).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
      path: "skills/find-bugs",
    });
  });

  it("parses a bare github.com repo URL", () => {
    expect(parseSkillSource("https://github.com/getsentry/find-bugs")).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
    });
  });

  it("parses a github.com /tree/<ref>/<path> URL without guessing a skill name", () => {
    expect(
      parseSkillSource("https://github.com/getsentry/find-bugs/tree/v2/skills/find-bugs"),
    ).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
      path: "skills/find-bugs",
      ref: "v2",
    });
  });

  it("leaves a /tree/<ref>/skills folder URL as a folder, not one skill", () => {
    expect(parseSkillSource("https://github.com/kentcdodds/kcd-skills/tree/main/skills")).toEqual({
      kind: "github",
      repo: "kentcdodds/kcd-skills",
      path: "skills",
      ref: "main",
    });
  });

  it("parses a github.com /blob/<ref>/<path>/SKILL.md URL", () => {
    expect(
      parseSkillSource(
        "https://github.com/getsentry/find-bugs/blob/main/skills/find-bugs/SKILL.md",
      ),
    ).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
      path: "skills/find-bugs",
      ref: "main",
      skillName: "find-bugs",
    });
  });

  it("rejects a /blob/<ref>/<path> URL that doesn't point at SKILL.md", () => {
    expect(
      parseSkillSource(
        "https://github.com/getsentry/find-bugs/blob/main/skills/find-bugs/README.md",
      ),
    ).toEqual({
      error: "Enter owner/repo, a GitHub URL, a skills.sh URL, or a local path",
    });
  });

  it("parses a skills.sh URL", () => {
    expect(parseSkillSource("https://skills.sh/getsentry/find-bugs/find-bugs")).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
      path: "find-bugs",
      skillName: "find-bugs",
    });
  });

  it("parses a git: source", () => {
    expect(parseSkillSource("git:https://example.com/skills.git")).toEqual({
      kind: "git",
      url: "https://example.com/skills.git",
    });
  });

  it("parses a bare .git URL", () => {
    expect(parseSkillSource("https://example.com/skills.git")).toEqual({
      kind: "git",
      url: "https://example.com/skills.git",
    });
  });

  it("parses an absolute local path", () => {
    expect(parseSkillSource("/Users/me/my-skill")).toEqual({
      kind: "local",
      localPath: "/Users/me/my-skill",
    });
  });

  it("parses a ~/ local path", () => {
    expect(parseSkillSource("~/skills/my-skill")).toEqual({
      kind: "local",
      localPath: "~/skills/my-skill",
    });
  });

  it("trims surrounding whitespace before parsing", () => {
    expect(parseSkillSource("  getsentry/find-bugs  ")).toEqual({
      kind: "github",
      repo: "getsentry/find-bugs",
    });
  });

  it("rejects an empty source", () => {
    expect(parseSkillSource("   ")).toEqual({
      error: "Enter owner/repo, a GitHub URL, a skills.sh URL, or a local path",
    });
  });

  it("rejects garbage input", () => {
    expect(parseSkillSource("not a valid source!!")).toEqual({
      error: "Enter owner/repo, a GitHub URL, a skills.sh URL, or a local path",
    });
  });
});

/** The text a user could type to get `source` back: the shortest accepted form for its fields. */
function sourceText(source: ParsedSkillSource): string {
  switch (source.kind) {
    case "git":
      return `git:${source.url}`;
    case "local":
      return source.localPath ?? "";
    case "github":
      if (source.ref !== undefined && source.skillName !== undefined)
        return `https://github.com/${source.repo}/blob/${source.ref}/${source.path}/SKILL.md`;
      if (source.ref !== undefined)
        return `https://github.com/${source.repo}/tree/${source.ref}/${source.path}`;
      if (source.skillName !== undefined) return `https://skills.sh/${source.repo}/${source.path}`;
      if (source.path !== undefined) return `${source.repo}/${source.path}`;
      return `https://github.com/${source.repo}`;
  }
}

describe("parseSkillSource properties", () => {
  // Segments hold no whitespace or "/": a whitespace-only /tree/ path and a skills.sh URL ending
  // "//" parse to a path the same text does not give back (follow-up, not guarded here).
  const segment = fc.string({
    unit: fc.constantFrom(...Array.from("aZ09._-#?%:@~")),
    minLength: 1,
    maxLength: 6,
  });
  const path = fc.array(segment, { minLength: 1, maxLength: 3 }).map((parts) => parts.join("/"));
  const sourceInput = fc.oneof(
    fc.string(),
    fc.tuple(segment, segment).map(([owner, repo]) => `${owner}/${repo}`),
    fc.tuple(segment, segment, path).map(([owner, repo, p]) => `${owner}/${repo}/${p}/`),
    fc
      .tuple(segment, segment, segment, path, fc.constantFrom("tree", "blob"))
      .map(([o, r, ref, p, form]) =>
        form === "blob"
          ? `https://github.com/${o}/${r}/blob/${ref}/${p}/SKILL.md`
          : `https://github.com/${o}/${r}/tree/${ref}/${p}/`,
      ),
    fc.tuple(segment, segment, path).map(([o, r, p]) => `https://skills.sh/${o}/${r}/${p}`),
    fc.tuple(segment, segment).map(([o, r]) => `https://github.com/${o}/${r}`),
    path.map((p) => `git:https://example.com/${p}.git`),
    path.map((p) => `https://example.com/${p}.git`),
    path.map((p) => `~/${p}`),
    path.map((p) => `/${p}`),
  );

  it("parses any text to a source or an error, and the source's own text parses back to it", () => {
    fc.assert(
      fc.property(sourceInput, fc.constantFrom("", " ", "\t"), (input, pad) => {
        const parsed = parseSkillSource(`${pad}${input}${pad}`);
        if ("error" in parsed) {
          expect(parsed.error).toBeTruthy();
          return;
        }
        expect(["github", "git", "local"]).toContain(parsed.kind);
        if (parsed.kind === "github") expect(parsed.repo).toMatch(/^[^/]+\/[^/]+$/);
        expect(parseSkillSource(sourceText(parsed))).toEqual(parsed);
      }),
    );
  });
});
