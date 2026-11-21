// @vitest-environment happy-dom

import { act } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockInstance } from "vitest";

import { SkillThrow } from "./SkillThrow";
import { PILE, PILE_COUNTS } from "./skill-pile-data";
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

describe("pile data", () => {
  it("keeps the measured ratio of described, name-only and hidden skills", () => {
    // Measured: 49 described, 107 name only, 129 hidden. Each count is that share of the pile.
    const share = (measured: number) => Math.round((measured / (49 + 107 + 129)) * PILE.length);
    expect(PILE_COUNTS.described).toBe(share(49));
    expect(PILE_COUNTS.nameOnly).toBe(share(107));
    expect(PILE_COUNTS.hidden).toBe(PILE.length - share(49) - share(107));
  });
});

describe("single-skill sorting", () => {
  it("sorts the next skill with Enter and Space, announces counts, and stops at the last one", async () => {
    setReducedMotion(true);
    const user = userEvent.setup();
    const { container } = render(<SkillThrow />);
    const button = screen.getByRole("button", { name: "Sort a skill" });
    const live = container.querySelector('[aria-live="polite"]');
    await user.tab();
    await user.tab();
    expect(document.activeElement).toBe(button);
    for (let count = 1; count <= PILE_COUNTS.notDescribed; count += 1) {
      await user.keyboard(count % 2 ? "{Enter}" : " ");
      expect(live?.textContent).toContain(`${count} sorted.`);
      expect(document.activeElement).toBe(button);
    }
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.hasAttribute("disabled")).toBe(false);
    await user.keyboard("{Enter}");
    expect(live?.textContent).toContain(`${PILE_COUNTS.notDescribed} sorted.`);
    expect(document.activeElement).toBe(button);
    expect(animate).not.toHaveBeenCalled();
    const chips = [...container.querySelectorAll('span[aria-hidden="true"]')];
    expect(
      chips.filter((chip) => chip instanceof HTMLElement && chip.style.opacity === "0"),
    ).toHaveLength(PILE_COUNTS.notDescribed);
    expect(PILE_COUNTS.notDescribed).toBeLessThan(PILE.length);
    // Each skill went to the drawer of its own state.
    expect(container.textContent).toContain(`Name only${PILE_COUNTS.nameOnly}`);
    expect(container.textContent).toContain(`Hidden${PILE_COUNTS.hidden}`);
  });

  it("uses the animated pointer sorting path when motion is allowed", async () => {
    setReducedMotion(false);
    const user = userEvent.setup();
    const { container } = render(<SkillThrow />);
    await act(async () => {});
    const button = screen.getByRole("button", { name: "Sort a skill" });
    animate.mockClear();
    button.focus();
    await user.keyboard("{Enter}");
    expect(animate).toHaveBeenCalled();
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toContain("1 sorted.");
  });
});

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
      `The model sees ${PILE_COUNTS.described} of ${PILE_COUNTS.total} skills with a description`,
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
