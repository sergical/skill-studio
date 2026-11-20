// @vitest-environment happy-dom
import html from "../../index.html?raw";
import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});
it.each([
  ["light", true, "light"],
  ["dark", false, "dark"],
  ["system", false, "light"],
])("stamps %s before the module loads", (stored, dark, expected) => {
  localStorage.setItem("theme", stored);
  vi.stubGlobal("matchMedia", () => ({ matches: dark }));
  const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  expect(script).toBeTruthy();
  window.eval(script ?? "");
  expect(document.documentElement.dataset.theme).toBe(expected);
});
it("uses the OS when storage is blocked", () => {
  // Restored here, not by afterEach: in Vitest and in bun test alike, vi.restoreAllMocks() left
  // this prototype spy in place, so a stamp test that ran after it read blocked storage.
  const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("blocked");
  });
  try {
    vi.stubGlobal("matchMedia", () => ({ matches: false }));
    const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
    expect(script).toBeTruthy();
    window.eval(script ?? "");
    expect(document.documentElement.dataset.theme).toBe("light");
  } finally {
    getItem.mockRestore();
  }
});
