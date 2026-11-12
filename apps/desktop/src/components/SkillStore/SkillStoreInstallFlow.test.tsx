// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { SkillStoreInstallFlow } from "./SkillStoreInstallFlow";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
  toast.dismiss();
});

it("reports failed defaults and still starts an install with the Universal destination", async () => {
  const errorToast = vi.spyOn(toast, "error");
  const requests: unknown[] = [];
  mockIPC(
    (command, args) => {
      if (command === "get_add_method_defaults") throw new Error("Defaults unavailable");
      if (command === "start_add_skill_operation") {
        requests.push(args);
        throw new Error("Install fixture stopped");
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  const user = userEvent.setup();
  const complete = vi.fn();
  render(
    <SkillStoreInstallFlow
      skill={{
        id: "owner/repo/sample",
        name: "sample",
        description: null,
        installs: 0,
        top_source: "owner/repo",
        author: null,
        tags: null,
        is_installed: false,
      }}
      resolvedTopSource={null}
      onInstallStart={vi.fn()}
      onInstallPaused={vi.fn()}
      onInstallComplete={complete}
    />,
  );
  await waitFor(() =>
    expect(errorToast).toHaveBeenCalledWith(
      "Couldn't load install defaults",
      expect.objectContaining({ description: "Defaults unavailable" }),
    ),
  );
  await user.click(screen.getByRole("button", { name: "Install Skill" }));
  await waitFor(() => expect(requests).toHaveLength(1));
  expect(requests[0]).toEqual(
    expect.objectContaining({
      request: expect.objectContaining({
        destination: "universal",
      }),
    }),
  );
  await waitFor(() => expect(complete).toHaveBeenCalled());
});
