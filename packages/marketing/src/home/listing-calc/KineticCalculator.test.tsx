// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";

import { KineticCalculator } from "./KineticCalculator";

afterEach(cleanup);

it("counts the Codex skills still listed by name when a low token cap leaves the rest out", async () => {
  const user = userEvent.setup();
  render(<KineticCalculator />);

  await user.click(screen.getByRole("radio", { name: /Codex/ }));
  await user.click(screen.getByRole("switch", { name: /Custom token cap/ }));
  screen.getByRole("slider", { name: "Max context tokens" }).focus();
  await user.keyboard("{Home}");

  const sentence = screen.getByText(/skills are left out of the list/).textContent ?? "";
  const [, leftOut, total] = sentence.match(/([\d,]+) of ([\d,]+) skills/) ?? [];
  const headline = screen.getByText("stay in the list", { exact: false }).parentElement;
  const readable = screen
    .getAllByText(/^\d[\d,]*$/)
    .find((el) => headline?.contains(el) && !el.closest('[aria-hidden="true"]'));
  const shown = Number(readable?.textContent?.replaceAll(",", ""));

  expect(Number(leftOut)).toBeGreaterThan(0);
  expect(shown).toBeGreaterThan(0);
  expect(shown + Number(leftOut.replaceAll(",", ""))).toBe(Number(total.replaceAll(",", "")));
});
