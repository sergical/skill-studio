// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import type { PaginatedSkillsResponse, SkillSearchResult } from "@skill-studio/lib";
import { SkillStore } from "./SkillStore";

afterEach(() => {
  cleanup();
  clearMocks();
});

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
  const user = userEvent.setup();
  render(<SkillStore />);
  const box = await screen.findByRole("textbox", { name: "Search skills.sh" });

  await user.type(box, "react");
  await new Promise((resolve) => setTimeout(resolve, 400));
  await user.type(box, " hooks");
  expect(await screen.findByText("react-hooks-guide", {}, { timeout: 2000 })).toBeTruthy();

  releaseFirst({ skills: [result("react-stale-result")], has_more: false });
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(screen.queryByText("react-stale-result")).toBeNull();
  expect(screen.getByText("react-hooks-guide")).toBeTruthy();
});
