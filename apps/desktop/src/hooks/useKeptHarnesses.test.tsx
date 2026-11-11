// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { useAppStore } from "../store/appStore";
import { useKeptHarnesses } from "./useKeptHarnesses";

Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
function KeptAgents() {
  return <output>{useKeptHarnesses().join(",")}</output>;
}
it("changing the saved agent choice updates mounted install choices or leaves a newly enabled agent hidden", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const choice = {
    kept: ["claude-code"],
    search_project_folders: true,
    saved_at: "2026-01-01T00:00:00Z",
  };
  useAppStore.getState().setHarnessDetection({ choice });
  try {
    await act(async () => root.render(<KeptAgents />));
    expect(host.textContent).toBe("claude-code");
    await act(async () => {
      useAppStore
        .getState()
        .setHarnessDetection({ choice: { ...choice, kept: [...choice.kept, "codex"] } });
    });
    expect(host.textContent).toBe("claude-code,codex");
  } finally {
    await act(async () => root.unmount());
    useAppStore.getState().setHarnessDetection({ choice: null });
    host.remove();
  }
});
