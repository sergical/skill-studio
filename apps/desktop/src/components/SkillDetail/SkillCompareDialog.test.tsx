// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { SkillCompareDialog } from "./SkillCompareDialog";

beforeEach(() => {
  mockIPC((command) =>
    command === "read_installed_skill_md" ? "---\nname: demo\n---\nSame body\n" : undefined,
  );
});

afterEach(() => {
  cleanup();
  clearMocks();
});

it("comparing two copies whose folders differ but whose SKILL.md matches says another file differs, not that the copies are the same", async () => {
  const left = {
    ...deployment({ agent: "shared", scope: "global", path: "/fixture/.agents/skills/demo" }),
    content_hash: "aaa",
  };
  const right = {
    ...deployment({ agent: "cursor", scope: "global", path: "/fixture/.cursor/skills/demo" }),
    content_hash: "bbb",
  };
  render(
    <SkillCompareDialog
      skill={skill({ name: "demo", deployments: [left, right] })}
      onClose={() => {}}
    />,
  );
  expect(await screen.findByText(/Another file in the folder differs/)).toBeTruthy();
  expect(screen.queryByText("These copies are the same.")).toBeNull();
});
