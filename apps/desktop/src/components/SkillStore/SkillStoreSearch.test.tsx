// @vitest-environment happy-dom
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PaginatedSkillsResponse, SkillSearchResult } from "@skill-studio/lib";
import { SkillStore } from "./SkillStore";
import { useAppStore } from "../../store/appStore";
import { skill } from "../../dev/harness/skill-fixture";

// Fake timers that still advance with real time, so Testing Library's polling works while a test
// can jump past the search box's debounce without sleeping.
beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  cleanup();
  useAppStore.getState().setTrackedProjects({ added: [], excluded: [] });
  clearMocks();
  vi.useRealTimers();
});

/** Longer than the search box's debounce. */
const PAST_DEBOUNCE_MS = 1_000;

function typist() {
  return userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
}

/** Runs every timer the component has queued, so a late retry would have fired. */
async function flushTimers(): Promise<void> {
  await act(() => vi.runAllTimersAsync());
}

function result(name: string): SkillSearchResult {
  return {
    id: `owner/repo/${name}`,
    name,
    description: null,
    installs: 1,
    top_source: "owner/repo",
    author: null,
    tags: null,
  };
}

it("a failed browse keeps the search box, or the user can only retry the query that failed", async () => {
  mockIPC((command) => {
    if (command === "get_installed_skills") return [];
    if (command === "get_popular_skills") throw new Error("fixture server is down");
    return null;
  });
  render(<SkillStore />);

  expect(await screen.findByText("Browsing isn't available right now")).toBeTruthy();
  expect(screen.getByRole("textbox", { name: "Search skills.sh" })).toBeTruthy();
});

it("a slow earlier search cannot replace the results of the query in the box", async () => {
  let releaseFirst: (response: PaginatedSkillsResponse) => void = () => {};
  mockIPC((command, args) => {
    if (command === "get_installed_skills") return [];
    if (command === "get_popular_skills") return { skills: [], has_more: false };
    if (command === "search_skills") {
      const query = args && "query" in args ? args.query : "";
      if (query === "react")
        return new Promise((resolve) => {
          releaseFirst = resolve;
        });
      return { skills: [result("react-hooks-guide")], has_more: false };
    }
    return null;
  });
  const user = typist();
  render(<SkillStore />);
  const box = await screen.findByRole("textbox", { name: "Search skills.sh" });

  await user.type(box, "react");
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  await user.type(box, " hooks");
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  expect(await screen.findByText("react-hooks-guide")).toBeTruthy();

  await act(async () => releaseFirst({ skills: [result("react-stale-result")], has_more: false }));
  await flushTimers();
  expect(screen.queryByText("react-stale-result")).toBeNull();
  expect(screen.getByText("react-hooks-guide")).toBeTruthy();
});

it("a server-down first load keeps its error and makes only one popular request", async () => {
  let requests = 0;
  mockIPC((command) => {
    if (command === "get_installed_skills") return [];
    if (command === "get_popular_skills") {
      requests += 1;
      throw new Error("server down");
    }
    return null;
  });
  render(<SkillStore />);
  await screen.findByText("server down");
  await flushTimers();
  expect(requests).toBe(1);
  expect(screen.getByText("server down")).toBeTruthy();
});

it("a late initial failure cannot replace an active search or stop its spinner", async () => {
  let rejectPopular: (error: Error) => void = () => {};
  let resolveSearch: (response: PaginatedSkillsResponse) => void = () => {};
  mockIPC((command) => {
    if (command === "get_installed_skills") return [];
    if (command === "get_popular_skills")
      return new Promise((_, reject) => {
        rejectPopular = reject;
      });
    if (command === "search_skills")
      return new Promise((resolve) => {
        resolveSearch = resolve;
      });
    return null;
  });
  render(<SkillStore />);
  await typist().type(screen.getByRole("textbox", { name: "Search skills.sh" }), "react");
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  await act(async () => rejectPopular(new Error("stale failure")));
  expect(screen.queryByText("stale failure")).toBeNull();
  expect(screen.getByText("Searching skills.sh…")).toBeTruthy();
  await act(async () => resolveSearch({ skills: [result("current-result")], has_more: false }));
  expect(screen.getByText("current-result")).toBeTruthy();
});

it("tracking a project marks its skills installed and keeps the current search until the query is cleared", async () => {
  mockIPC((command) => {
    if (command === "get_installed_skills") {
      return useAppStore.getState().userAddedProjects.length
        ? [
            skill({
              name: "git-guide",
              source: "owner/repo",
              source_url: "https://github.com/owner/repo",
              deployments: [],
            }),
          ]
        : [];
    }
    if (command === "get_popular_skills")
      return { skills: [result("popular-skill")], has_more: false };
    if (command === "search_skills") return { skills: [result("git-guide")], has_more: false };
    return null;
  });
  const user = typist();
  render(<SkillStore />);
  expect(await screen.findByText("popular-skill")).toBeTruthy();
  const box = screen.getByRole("textbox", { name: "Search skills.sh" });
  await user.type(box, "git");
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  await act(async () =>
    useAppStore.getState().setTrackedProjects({ added: ["/project"], excluded: [] }),
  );
  const card = await screen.findByRole("button", { name: /git-guide/ });
  expect(await within(card).findByTitle("Installed")).toBeTruthy();
  expect(screen.getByRole("textbox", { name: "Search skills.sh" })).toHaveProperty("value", "git");
  await user.clear(screen.getByRole("textbox", { name: "Search skills.sh" }));
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  expect(await screen.findByText("popular-skill")).toBeTruthy();
});

it("Load more adds the next page, and a search made while a page is on its way leaves Load more ready for the fresh list", async () => {
  let releaseSlowPage: (response: PaginatedSkillsResponse) => void = () => {};
  mockIPC((command, args) => {
    if (command === "get_installed_skills") return [];
    if (command === "get_popular_skills") {
      const page = args && "page" in args ? args.page : 0;
      if (page === 0) return { skills: [result("popular-one")], has_more: true };
      if (page === 1) return { skills: [result("popular-two")], has_more: true };
      return new Promise((resolve) => {
        releaseSlowPage = resolve;
      });
    }
    if (command === "search_skills") return { skills: [result("git-guide")], has_more: false };
    return null;
  });
  const user = typist();
  render(<SkillStore />);
  expect(await screen.findByText("popular-one")).toBeTruthy();

  await user.click(screen.getByRole("button", { name: "Load more" }));
  expect(await screen.findByText("popular-two")).toBeTruthy();
  expect(screen.getByText("popular-one")).toBeTruthy();

  // The third page is still on its way when the user searches, then clears the search.
  await user.click(screen.getByRole("button", { name: "Load more" }));
  const box = screen.getByRole("textbox", { name: "Search skills.sh" });
  await user.type(box, "git");
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  expect(await screen.findByText("git-guide")).toBeTruthy();
  expect(screen.queryByText("popular-one")).toBeNull();
  await user.clear(box);
  await act(() => vi.advanceTimersByTimeAsync(PAST_DEBOUNCE_MS));
  expect(await screen.findByText("popular-one")).toBeTruthy();
  expect(screen.queryByText("popular-two")).toBeNull();
  expect(await screen.findByRole("button", { name: "Load more" })).toBeTruthy();

  await act(async () => releaseSlowPage({ skills: [result("popular-three")], has_more: false }));
  await flushTimers();
  expect(screen.queryByText("popular-three")).toBeNull();
  expect(screen.getByRole("button", { name: "Load more" })).toBeTruthy();
});
