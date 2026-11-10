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
