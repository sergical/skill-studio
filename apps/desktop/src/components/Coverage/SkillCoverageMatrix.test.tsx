// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { skill } from "../../dev/harness/skill-fixture";
import { SkillCoverageMatrix } from "./SkillCoverageMatrix";

afterEach(cleanup);

it("keeps table rows and opens the scoped skill with Enter and Space on a button", async () => {
  const select = vi.fn();
  const user = userEvent.setup();
  render(
    <SkillCoverageMatrix
      skills={[skill({ name: "sample", deployments: [] })]}
      onSelectSkill={select}
      deploymentPathForSkill={() => "/project/sample"}
    />,
  );
  const button = screen.getByRole("button", { name: "sample" });
  expect(button.tagName).toBe("BUTTON");
  expect(screen.getAllByRole("row")).toHaveLength(2);
  await user.tab();
  expect(document.activeElement).toBe(button);
  await user.keyboard("{Enter}");
  await user.keyboard(" ");
  expect(select).toHaveBeenCalledTimes(2);
  expect(select).toHaveBeenLastCalledWith("sample", "/project/sample");
});
