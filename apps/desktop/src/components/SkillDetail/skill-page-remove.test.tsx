// @vitest-environment happy-dom
import { act, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { toast } from "sonner";
import { afterEach, expect, it, vi } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { useSkillPageActions } from "./skill-page-actions";

afterEach(() => {
  vi.restoreAllMocks();
  clearMocks();
});

const installed = skill({
  name: "demo",
  source_kind: "manual",
  deployments: [
    deployment({
      agent: "Codex",
      scope: "global",
      path: "/fixture/.codex/skills/demo",
      owner_id: "owner:v1/global/demo",
    }),
  ],
});

/** Runs the page's one Remove action, confirmed, with `remove_skill` failing with `failure` if set. */
function removeWith(failure: string | null) {
  mockIPC((command, args) => {
    // `ask` resolves true when the dialog answers with its OK label.
    if (command === "plugin:dialog|message") {
      // SAFETY: plugin-dialog's ask() with an okLabel sends buttons as { OkCancelCustom: [ok, cancel] }.
      const { buttons } = args as { buttons: { OkCancelCustom: [string, string] } };
      return buttons.OkCancelCustom[0];
    }
    if (command === "remove_skill") {
      if (failure !== null) throw failure;
      return {};
    }
    return null;
  });
  const onRemoveComplete = vi.fn();
  const { result } = renderHook(() => useSkillPageActions(installed, onRemoveComplete));
  expect(result.current.removeActions).toHaveLength(1);
  act(() => result.current.removeActions[0].run());
  return onRemoveComplete;
}

it("header Remove leaves the page before it says Removed, so the success toast is the last step", async () => {
  const success = vi.spyOn(toast, "success");
  const onRemoveComplete = removeWith(null);

  await waitFor(() => expect(success).toHaveBeenCalledOnce());
  expect(onRemoveComplete).toHaveBeenCalledOnce();
  expect(onRemoveComplete.mock.invocationCallOrder[0]).toBeLessThan(
    success.mock.invocationCallOrder[0],
  );
});

it("a failed header Remove shows the backend's reason, stays on the page and never says Removed", async () => {
  const success = vi.spyOn(toast, "success");
  const error = vi.spyOn(toast, "error");
  const onRemoveComplete = removeWith("fixture: permission denied");

  await waitFor(() => expect(error).toHaveBeenCalledOnce());
  expect(error.mock.calls[0][1]).toMatchObject({ description: "fixture: permission denied" });
  expect(success).not.toHaveBeenCalled();
  expect(onRemoveComplete).not.toHaveBeenCalled();
});
