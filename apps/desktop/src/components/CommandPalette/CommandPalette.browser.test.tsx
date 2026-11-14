// ============================================================================
// CommandPalette.browser.test - keyboard and styling a real engine must show.
// ============================================================================

import { beforeEach, expect, it } from "vitest";
import { page, userEvent } from "vitest/browser";
import { render } from "vitest-browser-react";
import "../../App.css";
import { useAppStore } from "../../store/appStore";
import { CommandPalette } from "./CommandPalette";

beforeEach(() => {
  useAppStore.setState({ commandPaletteOpen: false });
  document.documentElement.setAttribute("data-theme", "dark");
});

async function openPalette() {
  await render(<CommandPalette snapshot={undefined} requestRescan={async () => {}} />);
  useAppStore.getState().setCommandPaletteOpen(true);
  await expect.element(page.getByRole("combobox")).toHaveFocus();
}

function highlightedLabel(): string | null | undefined {
  return document.querySelector('[role="option"][aria-selected="true"] .truncate')?.textContent;
}

// WebKit (the Tauri webview) sends a mousemove at the unchanged pointer position whenever content
// scrolls under a resting pointer; the dispatch below stands in for that event.
it("pressing_ArrowDown_to_the_last_option_while_the_pointer_rests_on_the_list_highlights_the_last_option_instead_of_snapping_back_to_the_option_under_the_pointer", async () => {
  await openPalette();
  const options = page.getByRole("option").elements();
  await userEvent.hover(options[2]);
  const rect = options[2].getBoundingClientRect();
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;

  for (let i = 0; i < options.length; i++) {
    await userEvent.keyboard("{ArrowDown}");
    document
      .elementFromPoint(x, y)
      ?.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: x, clientY: y }));
  }

  expect(highlightedLabel()).toBe("Settings");
});

it("moving_the_pointer_onto_an_option_after_arrowing_highlights_that_option", async () => {
  await openPalette();
  await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
  await userEvent.hover(page.getByRole("option", { name: "Plugins" }));

  expect(highlightedLabel()).toBe("Plugins");
});

it("opening_the_palette_in_dark_mode_shows_the_query_input_without_its_own_filled_background", async () => {
  await openPalette();
  const input = page.getByRole("combobox").element();

  expect(getComputedStyle(input).backgroundColor).toBe("rgba(0, 0, 0, 0)");
});
