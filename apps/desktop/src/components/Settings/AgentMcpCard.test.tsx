// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { z } from "zod";
import { agentSetup } from "@skill-studio/lib";
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
const row = (id: string, name: string, onPath = true) => ({
  id,
  display_name: name,
  state: "installed" as const,
  executable: "/fixture/bin/agent",
  executable_on_path: onPath,
  version: { value: "1", evidence },
  install_method: { value: null, evidence },
  configured: true,
  used: false,
});

async function renderCard() {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<AgentMcpCard />));
  const button = (label: string) =>
    Array.from(host.querySelectorAll("button")).find((b) => b.textContent === label);
  return {
    host,
    text: () => host.textContent ?? "",
    labels: () => Array.from(host.querySelectorAll("button"), (b) => b.textContent),
    click: (label: string) => act(async () => button(label)?.click()),
    cleanup: async () => {
      await act(async () => root.unmount());
      host.remove();
    },
  };
}

it("Connect adds Codex, Reconnect replaces Claude Code, and Cursor shows Added once its dialog writes the entry, or names the wrong row", async () => {
  const connected = new Set(["claude-code"]);
  const connectCalls: string[] = [];
  mockIPC((command, payload) => {
    if (command === "get_mcp_connections") return [...connected];
    if (command === "connect_mcp") {
      const { harness } = z.object({ harness: z.string() }).parse(payload);
      connectCalls.push(harness);
      // Cursor writes its own entry only after the user confirms in its dialog.
      if (harness !== "cursor") connected.add(harness);
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
        row("cline", "Cline"),
      ],
    },
  });
  const card = await renderCard();
  try {
    expect(card.text()).toContain("Claude CodeAdded");
    expect(card.text()).not.toContain("Cline");
    expect(card.labels()).toEqual(["Copy setup prompt", "Reconnect", "Connect", "Add to Cursor"]);

    await card.click("Connect");
    expect(connectCalls).toEqual(["codex"]);
    expect(card.text()).toContain("CodexAdded");

    await card.click("Add to Cursor");
    expect(connectCalls).toEqual(["codex", "cursor"]);
    expect(card.text()).toContain("CursorConfirm in Cursor");

    connected.add("cursor");
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(card.text()).toContain("CursorAdded");
    expect(card.labels()).toEqual(["Copy setup prompt", "Reconnect", "Reconnect", "Reconnect"]);
  } finally {
    await card.cleanup();
  }
});

it("an agent whose CLI is not on PATH offers its add command to copy instead of a Connect that would fail", async () => {
  mockIPC((command) => {
    if (command === "get_mcp_connections") return [];
    throw new Error(`unexpected command: ${command}`);
  });
  const written: string[] = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: async (text: string) => void written.push(text) },
  });
  useAppStore.getState().setHarnessDetection({
    report: { harnesses: [row("open-code", "OpenCode", false)] },
  });
  const card = await renderCard();
  try {
    expect(card.text()).toContain("OpenCodeNot on PATH");
    await card.click("Copy command");
    expect(written).toEqual([agentSetup("open-code")?.command]);
  } finally {
    await card.cleanup();
  }
});

it("a failed reconnect shows the summary line with the command output behind Show details, and the row the config now holds", async () => {
  const connected = new Set(["claude-code"]);
  mockIPC((command) => {
    if (command === "get_mcp_connections") return [...connected];
    if (command === "connect_mcp") {
      connected.delete("claude-code");
      throw new Error("Claude Code couldn't add the server.\n$ claude mcp add …\ndisk full");
    }
    throw new Error(`unexpected command: ${command}`);
  });
  useAppStore.getState().setHarnessDetection({
    report: { harnesses: [row("claude-code", "Claude Code")] },
  });
  const card = await renderCard();
  try {
    await card.click("Reconnect");

    const alert = card.host.querySelector('[role="alert"]');
    expect(alert?.querySelector("p")?.textContent).toBe("Claude Code couldn't add the server.");
    expect(alert?.querySelector("details pre")?.textContent).toContain("disk full");
    expect(card.text()).not.toContain("Added");
    expect(card.labels()).toContain("Connect");
  } finally {
    await card.cleanup();
  }
});
