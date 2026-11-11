// @vitest-environment happy-dom
import { renderHook, render, screen, waitFor, cleanup, act } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import userEvent from "@testing-library/user-event";
import { addToast as showToast } from "../lib/toast";
import { Toaster, toast } from "sonner";
import type { Toast } from "@skill-studio/lib";
import { useParkRecovery } from "./useParkRecovery";

afterEach(() => {
  cleanup();
  clearMocks();
});

it("desktop startup shows retained paths until explicit acknowledgement or drops recovery notice", async () => {
  const acknowledgements: string[] = [];
  mockIPC((command) => {
    if (command === "park_recovery_notices")
      return { interrupted: ["crashed-move"], retained_quarantines: ["/fixture/.park-stage-kept"] };
    if (command === "acknowledge_park_recovery") acknowledgements.push(command);
  });
  const addToast = vi.fn<(notice: Omit<Toast, "id">) => string>(() => "notice");
  renderHook(() => useParkRecovery(addToast));
  await waitFor(() => expect(addToast).toHaveBeenCalledOnce());
  const notice = addToast.mock.calls[0]?.[0];
  expect(notice?.message).toContain("/fixture/.park-stage-kept");
  expect(notice?.duration).toBe(Infinity);
  expect(acknowledgements).toEqual([]);
  notice?.action?.onClick({ preventDefault: vi.fn() });
  await waitFor(() => expect(acknowledgements).toEqual(["acknowledge_park_recovery"]));
});

it("desktop move refreshes recovery after another process crashes or drops the notice until restart", async () => {
  let crashed = false;
  let reads = 0;
  mockIPC((command) => {
    if (command === "park_recovery_notices") {
      reads++;
      return {
        interrupted: crashed ? ["other-process"] : [],
        retained_quarantines: crashed ? ["/fixture/.park-quarantine-other"] : [],
      };
    }
  });
  const addToast = vi.fn<(notice: Omit<Toast, "id">) => string>(() => "notice");
  renderHook(() => useParkRecovery(addToast));
  await waitFor(() => expect(reads).toBe(1));
  expect(addToast).not.toHaveBeenCalled();
  crashed = true;
  window.dispatchEvent(new Event("skill-studio:move-finished"));
  await waitFor(() => expect(addToast).toHaveBeenCalledOnce());
  expect(addToast.mock.calls[0]?.[0].message).toContain("/fixture/.park-quarantine-other");
});

it("refresh twice then acknowledge removes the record notice or leaves duplicate persistent warnings", async () => {
  let reads = 0;
  let acknowledged = false;
  mockIPC((command, args) => {
    if (command === "park_recovery_notices") {
      reads++;
      return { interrupted: acknowledged ? [] : ["crashed-move"], retained_quarantines: [] };
    }
    if (command === "acknowledge_park_recovery") {
      expect(args).toEqual({ events: ["crashed-move"] });
      acknowledged = true;
    }
  });
  const dismiss = vi.spyOn(toast, "dismiss");
  const addToast = vi.fn<(notice: Omit<Toast, "id">) => string>(() => "recovery-toast");
  renderHook(() => useParkRecovery(addToast));
  await waitFor(() => expect(addToast).toHaveBeenCalledOnce());
  await act(async () => {
    window.dispatchEvent(new Event("skill-studio:move-finished"));
  });
  expect(reads).toBe(2);
  expect(addToast).toHaveBeenCalledOnce();
  await act(async () => {
    addToast.mock.calls[0]?.[0].action?.onClick({ preventDefault: vi.fn() });
  });
  expect(acknowledged).toBe(true);
  expect(dismiss).toHaveBeenCalledExactlyOnceWith("recovery-toast");
  dismiss.mockRestore();
});

it("recovery action keeps the rendered warning on rejected acknowledgement or hides an unresolved move", async () => {
  let reject = true;
  mockIPC((command) => {
    if (command === "park_recovery_notices")
      return { interrupted: ["rejected-move"], retained_quarantines: [] };
    if (command === "acknowledge_park_recovery" && reject)
      return Promise.reject(new Error("IPC rejected"));
  });
  function Recovery() {
    useParkRecovery(showToast);
    return <Toaster />;
  }
  render(<Recovery />);
  const user = userEvent.setup();
  const action = await screen.findByRole("button", { name: "Acknowledge" });
  await user.click(action);
  await screen.findByText("Could not acknowledge recovery");
  const warning = screen.getByText("Interrupted skill moves").closest("[data-sonner-toast]");
  expect(warning?.getAttribute("data-removed")).not.toBe("true");
  window.dispatchEvent(new Event("skill-studio:move-finished"));
  expect(screen.getAllByRole("button", { name: "Acknowledge" })).toHaveLength(1);
  reject = false;
  await user.click(action);
  await waitFor(() => expect(warning?.getAttribute("data-removed")).toBe("true"));
});
