// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { SkillsView } from "./SkillsView";
import { PluginSkillsView } from "./PluginSkillsView";
import { SkillActivityView } from "../Activity/SkillActivityView";

afterEach(cleanup);

it.each([SkillsView, PluginSkillsView, SkillActivityView])(
  "shows first scan loading and a retryable failure",
  async (View) => {
    const retry = vi.fn();
    const props = {
      snapshot: undefined,
      onSelectSkill: vi.fn(),
      active: true,
      isLoading: true,
      error: null,
      onRetry: retry,
    };
    const view = render(<View {...props} />);
    expect(screen.queryByText("You haven't added a skill yet")).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("Loading skills");
    view.rerender(<View {...props} isLoading={false} error="Scan failed" />);
    expect(screen.getByRole("alert").textContent).toContain("Scan failed");
    await userEvent.setup().click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
  },
);
