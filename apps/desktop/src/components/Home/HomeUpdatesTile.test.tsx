// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import type { SkillSnapshot } from "@skill-studio/lib";
import { HomeView } from "./HomeView";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";

afterEach(() => {
  cleanup();
  clearMocks();
});

const UNCHECKED: SkillSnapshot["update_check"] = {
  checked_at: null,
  gh_status: "ok",
  message: null,
  updates_available: 0,
  upstream_ahead: [],
};

function updatesTile() {
  return screen.getByRole("button", { name: /^Updates/, pressed: false });
}

it("Home_shows_the_source_update_check_inside_the_Updates_tile_or_repeats_it_as_a_second_card_with_a_different_count", () => {
  render(
    <HomeView
      snapshot={{
        ...buildHarnessSnapshot(),
        update_check: { ...UNCHECKED, updates_available: 65 },
      }}
      isLoading={false}
      active
      onSelectSkill={() => {}}
    />,
  );
  expect(screen.queryByRole("region", { name: "Source update check" })).toBeNull();
  expect(updatesTile().textContent).toContain("Not checked yet");
  expect(screen.queryByText(/65/)).toBeNull();
  expect(screen.getByRole("button", { name: "Check now" })).toBeTruthy();
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
      update_check: { ...UNCHECKED, gh_status: "missing", message: "GitHub CLI is not installed" },
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
    expect(updatesTile().textContent).toContain("GitHub CLI is not installed");
    let check = screen.getByRole("button", { name: "Check now" });
    for (let i = 0; i < 24 && document.activeElement !== check; i += 1) await user.tab();
    expect(document.activeElement).toBe(check);
    await user.keyboard(key);
    expect((await screen.findByRole("alert")).textContent).toContain("Network is unavailable");
    expect(updatesTile().textContent).toContain("Network is unavailable");
    check = screen.getByRole("button", { name: "Check now" });
    await user.click(check);
    expect(screen.getByRole("button", { name: "Checking…" }).hasAttribute("disabled")).toBe(true);
    finish?.({ ...UNCHECKED, checked_at: "2026-11-10T00:00:00Z" });
    await waitFor(() => expect(updatesTile().textContent).toMatch(/Checked /));
    expect(updatesTile().textContent).not.toContain("GitHub CLI is not installed");
    expect(screen.queryByRole("alert")).toBeNull();
  },
);

it("Check now settles and announces every success and failure transition or keeps an old result after a retry", async () => {
  const snapshot = {
    ...buildHarnessSnapshot(),
    skills: [],
    invocations: [],
    update_check: UNCHECKED,
  };
  const outcomes = [true, true, false, false, true];
  let request = 0;
  mockIPC((command) => {
    if (command !== "check_skill_updates_now") throw new Error(`Unexpected ${command}`);
    if (!outcomes[request++]) throw new Error("Check failed");
    return { ...UNCHECKED, checked_at: "2026-11-10T00:00:00Z" };
  });
  const user = userEvent.setup();
  render(<HomeView snapshot={snapshot} isLoading={false} active onSelectSkill={() => {}} />);
  expect(updatesTile().textContent).toContain("Not checked yet");
  for (const succeeds of outcomes) {
    await user.click(screen.getByRole("button", { name: "Check now" }));
    if (succeeds) {
      await waitFor(() => expect(updatesTile().textContent).toMatch(/Checked /));
      expect(screen.queryByRole("alert")).toBeNull();
      expect(
        screen
          .getAllByRole("status")
          .some((region) => (region.textContent ?? "").startsWith("Checked ")),
      ).toBe(true);
    } else {
      expect((await screen.findByRole("alert")).textContent).toBe("Check failed");
      expect(within(updatesTile()).getByText("Check failed")).toBeTruthy();
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
  const snapshot = {
    ...buildHarnessSnapshot(),
    skills: [],
    invocations: [],
    update_check: UNCHECKED,
  };
  mockIPC((command) => {
    if (command !== "check_skill_updates_now") throw new Error(`Unexpected ${command}`);
    return UNCHECKED;
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
        update_check: { ...UNCHECKED, checked_at: "2026-11-10T00:00:00Z", updates_available: 2 },
      }}
      isLoading={false}
      active
      onSelectSkill={() => {}}
    />,
  );
  await waitFor(() => expect(updatesTile().textContent).toMatch(/Checked /));
  expect(updatesTile().textContent).not.toContain("Not checked yet");
});

it("Broken_rows_carry_no_button_that_only_repeats_the_row_click_and_a_click_on_the_row_still_opens_the_skill", async () => {
  const onSelectSkill = vi.fn();
  render(
    <HomeView
      snapshot={buildHarnessSnapshot()}
      isLoading={false}
      active
      onSelectSkill={onSelectSkill}
    />,
  );
  const broken = screen
    .getAllByRole("rowgroup")
    .find((group) => group.getAttribute("data-group") === "broken");
  const rows = within(broken!).getAllByRole("row").slice(1);
  expect(rows.length).toBeGreaterThan(0);
  for (const row of rows) expect(within(row).queryAllByRole("button")).toEqual([]);
  await userEvent.setup().click(within(rows[0]).getByText(/broken link at/));
  expect(onSelectSkill).toHaveBeenCalledTimes(1);
});
