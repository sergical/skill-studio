// @vitest-environment happy-dom

import { act } from "react";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { BenchmarkSection } from "./BenchmarkSection";

/** Each bar row's observer callback, so the test can deliver a queued entry late. */
let observed: { callback: IntersectionObserverCallback; target: Element }[] = [];
/** The reduced-motion query the section reads, switched by the test. */
interface ReducedMotionStub {
  matches: boolean;
  listeners: (() => void)[];
}
const reduced: ReducedMotionStub = { matches: false, listeners: [] };
/** Animation frames the section asked for, run by the test. */
let frames: FrameRequestCallback[] = [];

/** The text each bar chart shows, in order. */
function barTexts(container: HTMLElement): string[] {
  return [...container.querySelectorAll('[role="img"]')].map((bars) => bars.textContent ?? "");
}

/** The bar values with no animation: what a reader with reduced motion sees. */
function finalTexts(): string[] {
  reduced.matches = true;
  const { container, unmount } = render(<BenchmarkSection />);
  const texts = barTexts(container);
  unmount();
  reduced.matches = false;
  return texts;
}

/** Delivers an in-view entry to every bar row's observer. */
function scrollIntoView() {
  for (const { callback, target } of observed) {
    const rect = target.getBoundingClientRect();
    callback(
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
      // SAFETY: the hook under test never reads the observer argument.
      {} as IntersectionObserver,
    );
  }
}

/** Runs queued frames until none are left, at a time past every bar's growth. */
function runFrames() {
  for (let step = 0; frames.length > 0 && step < 1000; step += 1) {
    const now = performance.now() + 60_000;
    for (const frame of frames.splice(0)) frame(now);
  }
}

beforeEach(() => {
  observed = [];
  reduced.matches = false;
  reduced.listeners = [];
  vi.stubGlobal(
    "IntersectionObserver",
    class implements IntersectionObserver {
      readonly root = null;
      readonly rootMargin = "0px";
      readonly thresholds = [0.5];
      readonly scrollMargin = "0px";
      constructor(private callback: IntersectionObserverCallback) {}
      observe(target: Element) {
        observed.push({ callback: this.callback, target });
      }
      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
    },
  );
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      get matches() {
        return reduced.matches;
      },
      addEventListener: (_: string, listener: () => void) => reduced.listeners.push(listener),
      removeEventListener: () => {},
    })),
  );
  frames = [];
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => frames.push(callback)),
  );
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("grows the bars to their final values once they scroll into view", () => {
  const final = finalTexts();
  const { container } = render(<BenchmarkSection />);
  expect(barTexts(container)).not.toEqual(final);

  act(scrollIntoView);
  act(runFrames);

  expect(barTexts(container)).toEqual(final);
});

it("shows the final values and does not start again when an entry queued before reduced motion arrives after it", () => {
  const final = finalTexts();
  const { container } = render(<BenchmarkSection />);
  expect(observed.length).toBeGreaterThan(0);

  act(() => {
    reduced.matches = true;
    for (const listener of reduced.listeners) listener();
  });
  act(scrollIntoView);

  expect(frames).toEqual([]);
  expect(barTexts(container)).toEqual(final);
});
