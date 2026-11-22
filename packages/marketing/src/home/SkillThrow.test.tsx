// @vitest-environment happy-dom

import { act } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockInstance } from "vitest";

import { SkillThrow } from "./SkillThrow";
import { PILE_COUNTS } from "./skill-pile-data";
import { fly, frame, poseTransform } from "./skill-pile-kit";

/** The `Element.prototype.animate` spy that each test starts with. */
let animate: MockInstance<Element["animate"]>;

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
  animate = vi.spyOn(Element.prototype, "animate").mockImplementation(() => {
    const animation = new Animation();
    Object.defineProperty(animation, "finished", { value: Promise.resolve(animation) });
    return Object.assign(animation, { commitStyles() {}, cancel() {} });
  });
});

afterEach(() => {
  cleanup();
  sessionStorage.clear();
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

describe("intro", () => {
  it("plays once per session, then opens on the sorted result", async () => {
    setReducedMotion(false);
    const first = render(<SkillThrow />);
    await act(async () => {});
    expect(animate).toHaveBeenCalled();
    expect(first.container.querySelector('[aria-live="polite"]')?.textContent).toContain(
      `${PILE_COUNTS.total} skills`,
    );
    first.unmount();
    animate.mockClear();
    const second = render(<SkillThrow />);
    await act(async () => {});
    expect(animate).not.toHaveBeenCalled();
    expect(second.container.querySelector('[aria-live="polite"]')?.textContent).toContain(
      `${PILE_COUNTS.parked} unused skills parked`,
    );
    expect(screen.getByRole("button", { name: "Make a mess again" })).toBeTruthy();
  });
});

describe("fly", () => {
  // WebKit can commit the first keyframe at the end of a flight. The mock commits nothing.
  it("ends on its last keyframe, whatever the browser commits", async () => {
    const chip = document.createElement("span");
    fly(chip, [frame({ x: 10, y: 20 })], { x: 30, y: 40, o: 0 }, { duration: 100 });
    await Promise.resolve();
    expect(chip.style.transform).toBe(poseTransform({ x: 30, y: 40 }));
    expect(chip.style.opacity).toBe("0");
  });
});
