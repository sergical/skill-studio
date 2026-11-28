// @vitest-environment happy-dom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import type { Pack, PackReport } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { PacksView } from "./PacksView";

const initial = useAppStore.getState();

afterEach(() => {
  cleanup();
  useAppStore.setState(initial, true);
});

function pack(overrides: Partial<Pack>): Pack {
  return {
    source: "owner/repo",
    kind: "skills_sh",
    git_url: "https://github.com/owner/repo.git",
    ref_pin: null,
    update_commands: [],
    scopes: [{ scope: "global" }],
    skills: [
      {
        name: "one",
        scope: { scope: "global" },
        path: null,
        agents: [],
        edited: null,
        behind: null,
      },
    ],
    new_upstream: [],
    edited_count: 0,
    behind_count: 0,
    remote: { state: "not_checked" },
    original: null,
    summary: "",
    ...overrides,
  };
}

/** Lists `checked` after Check upstream and `packs` before it. */
function servePacks(packs: Pack[], checked: Pack[]) {
  useAppStore.setState({
    listPacks: async (remote): Promise<PackReport> => ({
      packs: remote ? checked : packs,
      summary: "",
    }),
  });
}

it("says no pack could be checked when no pack has a Git source, not Up to date", async () => {
  const plugin = pack({
    source: "formatter@market",
    kind: "plugin",
    git_url: null,
    update_commands: ["claude plugin update formatter@market"],
  });
  servePacks([plugin], [{ ...plugin, remote: { state: "no_git_source" } }]);
  render(<PacksView />);

  await userEvent.click(await screen.findByRole("button", { name: "Check upstream" }));

  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("No pack can be checked upstream");
});

it("counts only the packs the check reached before saying Up to date", async () => {
  const repo = pack({});
  const pinned = pack({ source: "owner/pinned", ref_pin: "a".repeat(40) });
  servePacks(
    [repo, pinned],
    [
      { ...repo, remote: { state: "checked", commit: "abc" } },
      { ...pinned, remote: { state: "pinned_to_commit" } },
    ],
  );
  render(<PacksView />);

  await userEvent.click(await screen.findByRole("button", { name: "Check upstream" }));

  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("1 of 2 checked · Up to date");
});

it("a moved pack's row keeps what the check found about its original source", async () => {
  const moved = pack({
    source: "me/fork",
    remote: { state: "checked", commit: "abc" },
    original: {
      source: "owner/repo",
      remote: { state: "checked", commit: "def" },
      differs: ["one"],
      not_in_source: ["two", "three"],
    },
  });
  const failed = pack({
    source: "me/other-fork",
    remote: { state: "checked", commit: "abc" },
    original: {
      source: "owner/gone",
      remote: { state: "failed", reason: "repository not found" },
      differs: [],
      not_in_source: [],
    },
  });
  servePacks([moved, failed], [moved, failed]);
  render(<PacksView />);

  const movedRow = await screen.findByRole("listitem", { name: "me/fork" });
  expect(within(movedRow).getByText("1 differ from the original")).toBeTruthy();
  expect(within(movedRow).getByText("2 only in the original")).toBeTruthy();
  const failedRow = screen.getByRole("listitem", { name: "me/other-fork" });
  expect(within(failedRow).getByText(/repository not found/)).toBeTruthy();
});

it("counts a moved pack's unreachable fork once, not again for its original", async () => {
  const fork = pack({ source: "me/fork" });
  const unreachable = { state: "failed", reason: "repository not found" } as const;
  servePacks(
    [fork],
    [
      {
        ...fork,
        remote: unreachable,
        original: { source: "owner/repo", remote: unreachable, differs: [], not_in_source: [] },
      },
    ],
  );
  render(<PacksView />);

  await userEvent.click(await screen.findByRole("button", { name: "Check upstream" }));

  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("0 of 1 checked · 1 check failed");
});
