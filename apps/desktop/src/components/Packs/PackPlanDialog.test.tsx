// @vitest-environment happy-dom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { PackActionReport } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { PackPlanDialog } from "./PackPlanDialog";

const initial = useAppStore.getState();

afterEach(() => {
  cleanup();
  useAppStore.setState(initial, true);
});

it("removing a pack installed in one project names that project before Remove", async () => {
  const project = "/Users/me/src/app";
  const plan: PackActionReport = {
    dry_run: true,
    summary: "",
    steps: ["one", "two"].map((skill) => ({
      source: "owner/repo",
      skill,
      scope: { scope: "project", project },
      action: "remove",
      state: { state: "planned" },
    })),
  };
  useAppStore.setState({ runPackAction: async () => plan });
  render(<PackPlanDialog intent={{ kind: "remove", source: "owner/repo" }} onClose={() => {}} />);

  const dialog = await screen.findByRole("alertdialog");
  await within(dialog).findByRole("button", { name: "Remove 2 skills" });
  expect(within(dialog).getByText(`Project ${project}`)).toBeTruthy();
});
