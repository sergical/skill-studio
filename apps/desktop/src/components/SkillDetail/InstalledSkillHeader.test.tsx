// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { InstalledSkillHeader } from "./InstalledSkillHeader";

afterEach(cleanup);

it("opening a fork note with duplicate source records shows one comparison link that works with Tab and Enter instead of repeating the note", async () => {
  const copy = {
    ...deployment({
      agent: "shared",
      scope: "global",
      path: "/fixture/.agents/skills/demo",
      owner_kind: "dotagents",
    }),
    owner_id: "owner:v1/global/demo",
  };
  const note = {
    repo: "me/fork",
    upstream_repo: "original/repo",
    behind_by: 3,
    compare_url: "https://github.com/original/repo/compare/me:main...main",
    owner_ids: [copy.owner_id],
  };
  render(
    <InstalledSkillHeader
      skill={skill({ name: "demo", source_kind: "dotagents", deployments: [copy] })}
      deployment={copy}
      upstreamAhead={[note, note]}
      onFixRepair={() => {}}
      onEditManually={() => {}}
    />,
  );
  expect(screen.getAllByText(/Forked from original\/repo/)).toHaveLength(1);
  const link = screen.getByRole("link", { name: "See changes" });
  expect(link.getAttribute("href")).toBe(note.compare_url);
  const user = userEvent.setup();
  await user.tab();
  expect(document.activeElement).toBe(link);
  let activated = false;
  link.addEventListener("click", (event) => {
    event.preventDefault();
    activated = true;
  });
  await user.keyboard("{Enter}");
  expect(activated).toBe(true);
});

it("a skill whose SKILL.md lacks a description shows an Edit SKILL.md button that opens the editor, so the red line is not a dead end", async () => {
  const copy = {
    ...deployment({ agent: "shared", scope: "global", path: "/fixture/.agents/skills/lint" }),
    spec_violations: ["missing required frontmatter field: description"],
  };
  let opened = false;
  render(
    <InstalledSkillHeader
      skill={skill({ name: "lint", deployments: [copy] })}
      deployment={copy}
      onFixRepair={() => {}}
      onEditManually={() => {
        opened = true;
      }}
    />,
  );
  await userEvent.setup().click(screen.getByRole("button", { name: "Edit SKILL.md" }));
  expect(opened).toBe(true);
});
