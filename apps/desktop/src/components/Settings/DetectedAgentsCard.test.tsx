// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { useAppStore } from "../../store/appStore";
import { DetectedAgentsCard } from "./DetectedAgentsCard";

Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
afterEach(() => {
  useAppStore
    .getState()
    .setHarnessDetection({ report: null, choice: null, error: null, saving: false });
});
it("a newly detected agent stays visible and off until chosen or turns on without permission", async () => {
  const evidence = { source: "fixture", confidence: "unknown" as const };
  const row = (id: "codex" | "claude-code", name: string) => ({
    id,
    display_name: name,
    state: "installed" as const,
    executable: "/fixture/bin/agent",
    executable_on_path: false,
    version: { value: "1", evidence },
    install_method: { value: null, evidence },
    configured: false,
    used: false,
  });
  useAppStore.getState().setHarnessDetection({
    report: { harnesses: [row("claude-code", "Claude Code"), row("codex", "Codex")] },
    choice: {
      kept: ["claude-code"],
      search_project_folders: true,
      saved_at: "2026-01-01T00:00:00Z",
    },
  });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await act(async () => root.render(<DetectedAgentsCard />));
    const labels = Array.from(host.querySelectorAll("label"));
    const codex = labels.find((label) => label.textContent?.includes("Codex"));
    const claude = labels.find((label) => label.textContent?.includes("Claude Code"));
    expect(codex).toBeDefined();
    expect(codex?.querySelector("input")?.checked).toBe(false);
    expect(claude?.querySelector("input")?.checked).toBe(true);
    expect(codex?.textContent).toContain("Not on PATH");
    expect(useAppStore.getState().harnessDetection.choice?.kept).toEqual(["claude-code"]);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
