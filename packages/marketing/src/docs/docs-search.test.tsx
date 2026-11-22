// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";

import { DocsPage } from "./DocsPage";
import { DOCS_PAGES } from "./docs-pages";
import { setupDocsSearch } from "./docs-search";

it("opens a docs search result with the keyboard and clears it with Escape", async () => {
  const user = userEvent.setup();
  render(<DocsPage page={DOCS_PAGES[0]} />);
  setupDocsSearch();
  const nav = within(screen.getByRole("navigation", { name: "Docs" }));

  fireEvent.keyDown(document.body, { key: "/", shiftKey: true });
  const field = screen.getByRole("searchbox", { name: "Search docs" });
  expect(document.activeElement).toBe(field);

  await user.keyboard("install a pack{ArrowDown}");
  const result = screen.getByRole("link", { name: "Install a pack" });
  expect(document.activeElement).toBe(result);
  expect(nav.getByText("Troubleshooting", { exact: true }).hidden).toBe(true);

  await user.keyboard("{Escape}");
  expect(document.activeElement).toBe(field);
  expect(nav.getByText("Troubleshooting", { exact: true }).hidden).toBe(false);

  let opened = false;
  result.addEventListener("click", (event) => {
    event.preventDefault();
    opened = true;
  });
  await user.keyboard("install a pack{Enter}");
  expect(opened).toBe(true);
});
