// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { SkillSnapshot } from "@skill-studio/lib";
import { HomeView } from "./HomeView";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";

afterEach(() => {
  cleanup();
  clearMocks();
});
it.each([
  { key: "{Enter}", label: "Enter" },
  { key: " ", label: "Space" },
])(
  "Home shows a failed update check and retries with $label or claims unavailable checks are current",
  async ({ key }) => {
    const snapshot: SkillSnapshot = {
      ...buildHarnessSnapshot(),
      skills: [],
      invocations: [],
      update_check: {
        checked_at: null,
        gh_status: "missing",
        message: "GitHub CLI is not installed",
        updates_available: 0,
        upstream_ahead: [],
      },
    };
    let finish: ((value: typeof snapshot.update_check) => void) | undefined;
    let attempts = 0;
    mockIPC((command) => {
      if (command === "check_skill_updates_now") {
        attempts += 1;
        if (attempts === 1) throw new Error("Network is unavailable");
        return new Promise<typeof snapshot.update_check>((resolve) => {
          finish = resolve;
        });
      }
      throw new Error(`Unexpected ${command}`);
    });
    const user = userEvent.setup();
    render(<HomeView snapshot={snapshot} isLoading={false} active onSelectSkill={() => {}} />);
    expect(await screen.findByText("GitHub CLI is not installed")).toBeTruthy();
    let check = screen.getByRole("button", { name: "Check now" });
    for (let i = 0; i < 24 && document.activeElement !== check; i += 1) await user.tab();
    expect(document.activeElement).toBe(check);
    await user.keyboard(key);
    expect((await screen.findByRole("alert")).textContent).toContain("Network is unavailable");
    check = screen.getByRole("button", { name: "Check now" });
    await user.click(check);
    expect(screen.getByRole("button", { name: "Checking…" }).hasAttribute("disabled")).toBe(true);
    finish?.({
      checked_at: "2026-11-10T00:00:00Z",
      gh_status: "ok",
      message: null,
      updates_available: 0,
      upstream_ahead: [],
    });
    await waitFor(() => expect(screen.getByText("No updates available")).toBeTruthy());
    expect(screen.queryByText("GitHub CLI is not installed")).toBeNull();
  },
);

it("Check now settles every success and failure transition or keeps an old result after a retry", async () => {
  const snapshot = {
    ...buildHarnessSnapshot(),
    skills: [],
    invocations: [],
    update_check: {
      checked_at: null,
      gh_status: "ok",
      message: null,
      updates_available: 0,
      upstream_ahead: [],
    },
  };
  const outcomes = [true, true, false, false, true];
  let request = 0;
  mockIPC((command) => {
    if (command !== "check_skill_updates_now") throw new Error(`Unexpected ${command}`);
    if (!outcomes[request++]) throw new Error("Check failed");
    return { ...snapshot.update_check, checked_at: "2026-11-10T00:00:00Z" };
  });
  const user = userEvent.setup();
  render(<HomeView snapshot={snapshot} isLoading={false} active onSelectSkill={() => {}} />);
  expect(await screen.findByText("Updates have not been checked yet")).toBeTruthy();
  for (const succeeds of outcomes) {
    await user.click(screen.getByRole("button", { name: "Check now" }));
    if (succeeds) {
      expect(await screen.findByText("No updates available")).toBeTruthy();
      expect(screen.queryByRole("alert")).toBeNull();
    } else {
      expect((await screen.findByRole("alert")).textContent).toBe("Check failed");
      expect(screen.getByText("Could not check for updates")).toBeTruthy();
    }
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Check now" }).hasAttribute("disabled")).toBe(
        false,
      ),
    );
  }
  expect(request).toBe(outcomes.length);
});

it("Check now that returns an undated summary yields to a newer dated snapshot instead of keeping Home on not checked yet", async () => {
  const unchecked = {
    checked_at: null,
    gh_status: "ok" as const,
    message: null,
    updates_available: 0,
    upstream_ahead: [],
  };
  const snapshot = {
    ...buildHarnessSnapshot(),
    skills: [],
    invocations: [],
    update_check: unchecked,
  };
  mockIPC((command) => {
    if (command !== "check_skill_updates_now") throw new Error(`Unexpected ${command}`);
    return unchecked;
  });
  const user = userEvent.setup();
  const view = render(
    <HomeView snapshot={snapshot} isLoading={false} active onSelectSkill={() => {}} />,
  );
  await user.click(screen.getByRole("button", { name: "Check now" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Check now" }).hasAttribute("disabled")).toBe(false),
  );
  view.rerender(
    <HomeView
      snapshot={{
        ...snapshot,
        update_check: { ...unchecked, checked_at: "2026-11-10T00:00:00Z", updates_available: 2 },
      }}
      isLoading={false}
      active
      onSelectSkill={() => {}}
    />,
  );
  expect(await screen.findByText("2 updates available")).toBeTruthy();
});
