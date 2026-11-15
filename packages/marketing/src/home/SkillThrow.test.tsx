// @vitest-environment happy-dom

import { act } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SkillThrow } from "./SkillThrow";
import { PILE, PILE_COUNTS } from "./skill-pile-data";

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal(
    "IntersectionObserver",
    class implements IntersectionObserver {
      readonly root = null;
      readonly rootMargin = "0px";
      readonly thresholds = [0.3, 0.5];
      readonly scrollMargin = "0px";
      constructor(private callback: IntersectionObserverCallback) {}
      observe(target: Element) {
        const rect = target.getBoundingClientRect();
        this.callback(
          [
            {
              target,
              intersectionRatio: 1,
              isIntersecting: true,
              time: 0,
              boundingClientRect: rect,
              intersectionRect: rect,
              rootBounds: null,
            },
          ],
          this,
        );
      }
      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
    },
  );
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn(() => 1),
  );
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 640,
    bottom: 480,
    width: 640,
    height: 480,
    toJSON: () => ({}),
  });
  vi.spyOn(Element.prototype, "getAnimations").mockReturnValue([]);
  vi.spyOn(Element.prototype, "animate").mockImplementation(() => {
    const animation = new Animation();
    Object.defineProperty(animation, "finished", { value: Promise.resolve(animation) });
    return Object.assign(animation, { commitStyles() {}, cancel() {} });
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function setReducedMotion(reduced: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => {
    const media = new EventTarget();
    return Object.assign(media, {
      media: query,
      matches: reduced,
      onchange: null,
      addListener() {},
      removeListener() {},
    });
  });
}

describe("single-copy parking", () => {
  it("parks the next copy with Enter and Space, announces counts, and stops at the last copy", async () => {
    setReducedMotion(true);
    const user = userEvent.setup();
    const { container } = render(<SkillThrow />);
    const button = screen.getByRole("button", { name: "Park a copy" });
    const live = container.querySelector('[aria-live="polite"]');
    const animate = vi.mocked(Element.prototype.animate);
    await user.tab();
    await user.tab();
    expect(document.activeElement).toBe(button);
    for (let count = 1; count <= PILE_COUNTS.copies; count += 1) {
      await user.keyboard(count % 2 ? "{Enter}" : " ");
      expect(live?.textContent).toContain(`${count} parked.`);
      expect(document.activeElement).toBe(button);
    }
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.hasAttribute("disabled")).toBe(false);
    await user.keyboard("{Enter}");
    expect(live?.textContent).toContain(`${PILE_COUNTS.copies} parked.`);
    expect(document.activeElement).toBe(button);
    expect(animate).not.toHaveBeenCalled();
    const chips = [...container.querySelectorAll('span[aria-hidden="true"]')];
    expect(
      chips.filter((chip) => chip instanceof HTMLElement && chip.style.opacity === "0"),
    ).toHaveLength(PILE_COUNTS.copies);
    expect(PILE_COUNTS.copies).toBeLessThan(PILE.length);
  });

  it("uses the animated pointer parking path when motion is allowed", async () => {
    setReducedMotion(false);
    const user = userEvent.setup();
    const { container } = render(<SkillThrow />);
    await act(async () => {});
    const button = screen.getByRole("button", { name: "Park a copy" });
    vi.mocked(Element.prototype.animate).mockClear();
    button.focus();
    await user.keyboard("{Enter}");
    expect(Element.prototype.animate).toHaveBeenCalled();
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toContain("1 parked.");
  });
});
