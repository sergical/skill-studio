// @vitest-environment happy-dom
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { RowActionSwitch } from "./SkillLocationRowButtons";
import type { LocationAction } from "./skill-location-status";

const action: LocationAction = { kind: "compare" };
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("park with a slow snapshot stays off and disabled after five seconds instead of bouncing back", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  render(
    <RowActionSwitch
      checked
      ariaLabel="Global Codex copy"
      action={action}
      onAction={async () => true}
      changesAtOnce
    />,
  );
  await user.click(screen.getByRole("switch"));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000);
  });
  expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("false");
  expect(screen.getByRole("switch").getAttribute("aria-disabled")).toBe("true");
});

it("snapshot confirms the parked target and enables the switch instead of inverting the new checked prop", async () => {
  const props = {
    ariaLabel: "Global Codex copy",
    action,
    onAction: async () => true,
    changesAtOnce: true,
  };
  const view = render(<RowActionSwitch checked {...props} />);
  await userEvent.click(screen.getByRole("switch"));
  view.rerender(<RowActionSwitch checked={false} {...props} />);
  expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("false");
  expect(screen.getByRole("switch").getAttribute("aria-disabled")).toBe(null);
});
