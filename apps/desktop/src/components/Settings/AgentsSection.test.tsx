// @vitest-environment happy-dom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { z } from "zod";
import { Button } from "@skill-studio/ui";
import { agentSetup, type DiscoverySourceSetting } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { continueIsBlocked, useFirstRunScreen } from "../../hooks/useFirstRun";
import { AgentsSection } from "./AgentsSection";

Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
afterEach(() => {
  clearMocks();
  useAppStore
    .getState()
    .setHarnessDetection({ report: null, choice: null, error: null, saving: false });
});

const evidence = { source: "fixture", confidence: "unknown" as const };
const row = (
  id: string,
  name: string,
  { onPath = true, found = true }: { onPath?: boolean; found?: boolean } = {},
) => ({
  id,
  display_name: name,
  state: found ? ("installed" as const) : ("not_found" as const),
  executable: found ? "/fixture/bin/agent" : null,
  executable_on_path: found ? onPath : null,
  version: { value: found ? "1" : null, evidence },
  install_method: { value: null, evidence },
  configured: found,
  used: false,
});

const choice = (kept: string[]) => ({
  kept,
  search_project_folders: true,
  saved_at: "2026-01-01T00:00:00Z",
});

async function render(
  node: React.ReactNode = <AgentsSection sources={null} onToggleSource={() => undefined} />,
) {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(node));
  const button = (name: string) =>
    Array.from(host.querySelectorAll("button")).find(
      (b) => (b.getAttribute("aria-label") ?? b.textContent) === name,
    );
  const agentRow = (name: string) =>
    Array.from(host.querySelectorAll("tbody tr")).find((tr) =>
      tr.querySelector("th")?.textContent?.startsWith(name),
    );
  return {
    host,
    text: () => host.textContent ?? "",
    agentRow,
    click: (name: string) => act(async () => button(name)?.click()),
    cleanup: async () => {
      await act(async () => root.unmount());
      host.remove();
    },
  };
}

function WelcomeDetection() {
  const [continued, setContinued] = useState(false);
  const welcome = useFirstRunScreen(() => setContinued(true));
  if (continued) return <AgentsSection sources={null} onToggleSource={() => undefined} />;
  return (
    <>
      {welcome.error && <p role="alert">{welcome.error}</p>}
      <Button disabled={continueIsBlocked(welcome)} onClick={welcome.continue}>
        Continue
      </Button>
    </>
  );
}

it("a welcome detection failure reaches Settings instead of a permanent checking message after Continue", async () => {
  const failure = new Error("fixture probe failed");
  mockIPC((command) => {
    if (command === "detect_harnesses") throw failure;
    if (command === "save_harnesses_choice") return;
    if (command === "get_mcp_connections") return [];
    throw new Error(`unexpected command: ${command}`);
  });
  const view = await render(<WelcomeDetection />);
  try {
    expect(view.host.querySelector('[role="alert"]')?.textContent).toContain(failure.message);
    await act(async () => view.host.querySelector("button")?.click());
    expect(view.host.querySelector('[role="alert"]')?.textContent).toContain(failure.message);
    expect(view.host.querySelector('[role="status"]')).toBeNull();
  } finally {
    await view.cleanup();
  }
});

it("every agent is listed: a new one stays off, a missing one is named as not found, and only shown agents get a history switch", async () => {
  mockIPC((command) => {
    if (command === "get_mcp_connections") return [];
    throw new Error(`unexpected command: ${command}`);
  });
  useAppStore.getState().setHarnessDetection({
    report: {
      harnesses: [
        row("claude-code", "Claude Code"),
        row("codex", "Codex", { onPath: false }),
        row("pi", "pi", { found: false }),
      ],
    },
    choice: choice(["claude-code"]),
  });
  const sources: DiscoverySourceSetting[] = [
    { harness: "claude-code", enabled: true },
    { harness: "codex", enabled: true },
    { harness: "pi", enabled: true },
  ];
  const view = await render(<AgentsSection sources={sources} onToggleSource={() => undefined} />);
  try {
    const checked = (name: string, label: string) =>
      view.agentRow(name)?.querySelector(`[aria-label="${label}"]`)?.getAttribute("aria-checked");
    expect(checked("Claude Code", "Show Claude Code skills")).toBe("true");
    expect(checked("Claude Code", "Find projects in Claude Code history")).toBe("true");
    expect(checked("Codex", "Show Codex skills")).toBe("false");
    expect(view.agentRow("Codex")?.textContent).toContain("Not on PATH");
    expect(
      view.agentRow("Codex")?.querySelector('[aria-label="Find projects in Codex history"]'),
    ).toBeNull();
    expect(view.agentRow("pi")).toBeUndefined();
    expect(view.text()).toContain("Not found on this Macpi");
    expect(view.host.querySelector('[aria-label="Show pi skills"]')).toBeNull();
    expect(useAppStore.getState().harnessDetection.choice?.kept).toEqual(["claude-code"]);
  } finally {
    await view.cleanup();
  }
});

it("Connect adds Codex, Reconnect replaces Claude Code, and Cursor shows Added once its dialog writes the entry", async () => {
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
  const view = await render();
  try {
    expect(view.agentRow("Claude Code")?.textContent).toContain("Added");
    expect(view.text()).not.toContain("Cline");

    await view.click("Connect Codex");
    expect(connectCalls).toEqual(["codex"]);
    expect(view.agentRow("Codex")?.textContent).toContain("Added");

    await view.click("Add to Cursor");
    expect(connectCalls).toEqual(["codex", "cursor"]);
    expect(view.agentRow("Cursor")?.textContent).toContain("Confirm in Cursor");

    connected.add("cursor");
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(view.agentRow("Cursor")?.textContent).toContain("Added");
  } finally {
    await view.cleanup();
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
    report: { harnesses: [row("opencode", "OpenCode", { onPath: false })] },
  });
  const view = await render();
  try {
    expect(view.agentRow("OpenCode")?.textContent).toContain("Not on PATH");
    await view.click("Copy command");
    expect(written).toEqual([agentSetup("opencode")?.command]);
  } finally {
    await view.cleanup();
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
  const view = await render();
  try {
    await view.click("Reconnect Claude Code");

    const alert = view.host.querySelector('[role="alert"]');
    expect(alert?.querySelector("p")?.textContent).toBe("Claude Code couldn't add the server.");
    expect(alert?.querySelector("details pre")?.textContent).toContain("disk full");
    expect(view.agentRow("Claude Code")?.textContent).not.toContain("Added");
    expect(
      view.agentRow("Claude Code")?.querySelector('[aria-label="Connect Claude Code"]'),
    ).not.toBeNull();
  } finally {
    await view.cleanup();
  }
});
