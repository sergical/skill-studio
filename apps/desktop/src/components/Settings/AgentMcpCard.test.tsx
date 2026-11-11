// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { z } from "zod";
import { useAppStore } from "../../store/appStore";
import { AgentMcpCard } from "./AgentMcpCard";

Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
afterEach(() => {
  clearMocks();
  useAppStore
    .getState()
    .setHarnessDetection({ report: null, choice: null, error: null, saving: false });
});

const evidence = { source: "fixture", confidence: "unknown" as const };
const row = (id: "codex" | "claude-code" | "cursor", name: string) => ({
  id,
  display_name: name,
  state: "installed" as const,
  executable: "/fixture/bin/agent",
  executable_on_path: true,
  version: { value: "1", evidence },
  install_method: { value: null, evidence },
  configured: true,
  used: false,
});

it("connecting Codex runs connect_mcp for codex and shows Added, or names the stale row", async () => {
  const connected = new Set(["claude-code"]);
  const connectCalls: unknown[] = [];
  mockIPC((command, payload) => {
    if (command === "get_mcp_connections") return [...connected];
    if (command === "connect_mcp") {
      const { harness } = z.object({ harness: z.string() }).parse(payload);
      connectCalls.push(harness);
      connected.add(harness);
      return;
    }
    throw new Error(`unexpected command: ${command}`);
  });
  useAppStore.getState().setHarnessDetection({
    report: {
      harnesses: [
        row("claude-code", "Claude Code"),
        row("codex", "Codex"),
        row("cursor", "Cursor"),
      ],
    },
  });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await act(async () => root.render(<AgentMcpCard />));
    const text = () => host.textContent ?? "";
    expect(text()).toContain("Claude CodeAdded");
    expect(text()).not.toContain("Cursor");
    const buttons = Array.from(host.querySelectorAll("button"));
    expect(buttons.map((b) => b.textContent)).toEqual(["Connect"]);

    await act(async () => buttons[0].click());

    expect(connectCalls).toEqual(["codex"]);
    expect(text()).toContain("CodexAdded");
    expect(host.querySelector("button")).toBeNull();
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
