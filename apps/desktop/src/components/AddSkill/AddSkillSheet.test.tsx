// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import { useAppStore } from "../../store/appStore";
import { AddSkillSheet } from "./AddSkillSheet";
import { TrustConfirmFooter } from "./TrustConfirmFooter";

afterEach(() => {
  cleanup();
  clearMocks();
  useAppStore.setState({ addSkillSheet: { open: false } });
});

it("Add by source submits a local Copy and shows its error or the pack cleanup breaks normal Add", async () => {
  const requests: unknown[] = [];
  mockIPC(
    (command, args) => {
      if (command === "get_add_method_defaults")
        return {
          dotagents_installed: false,
          has_skill_lock: false,
          installed_harnesses: [],
          claude_reads_shared_folder: false,
        };
      if (command === "get_harnesses_choice") return null;
      if (command === "start_add_skill_operation") {
        requests.push(args);
        return {
          operation_id: args && "operationId" in args ? args.operationId : "operation",
          sequence: 1,
          phase: "failed",
          message: "Fixture folder is missing",
        };
      }
      throw new Error(`Unexpected command: ${command}`);
    },
    { shouldMockEvents: true },
  );
  useAppStore.getState().openAddSkillSheet("/fixture/sample");
  const user = userEvent.setup();
  render(<AddSkillSheet skills={[]} />);
  const add = await screen.findByRole("button", { name: "Add skill" });
  await waitFor(() => expect(add.hasAttribute("disabled")).toBe(false));
  for (let i = 0; i < 24 && document.activeElement !== add; i += 1) await user.tab();
  expect(document.activeElement).toBe(add);
  await user.keyboard("{Enter}");
  expect((await screen.findByRole("alert")).textContent).toContain("Fixture folder is missing");
  expect(requests).toEqual([
    expect.objectContaining({
      request: expect.objectContaining({ method: "copy", destination: "universal" }),
    }),
  ]);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it.each([
  { key: "{Enter}", label: "Enter" },
  { key: " ", label: "Space" },
])(
  "repository trust can retry with $label or pack cleanup disables normal trust",
  async ({ key }) => {
    const retry = vi.fn();
    const close = vi.fn();
    const user = userEvent.setup();
    const view = render(
      <TrustConfirmFooter
        untrustedIdentity="owner/repo"
        trustBusy={false}
        onCancel={close}
        onTrustAndRetry={retry}
      />,
    );
    expect(screen.getByText("owner/repo")).toBeTruthy();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close" }));
    await user.keyboard(key);
    expect(close).toHaveBeenCalledOnce();
    await user.tab();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Trust repository and retry" }),
    );
    await user.keyboard(key);
    expect(retry).toHaveBeenCalledOnce();
    view.rerender(
      <TrustConfirmFooter
        untrustedIdentity="owner/repo"
        trustBusy
        onCancel={close}
        onTrustAndRetry={retry}
      />,
    );
    expect(
      screen.getByRole("button", { name: "Trust repository and retry" }).hasAttribute("disabled"),
    ).toBe(true);
  },
);
