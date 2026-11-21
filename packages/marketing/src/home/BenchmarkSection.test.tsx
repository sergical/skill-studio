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
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn(() => 1),
  );
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("does not start the bars again when an entry queued before reduced motion arrives after it", () => {
  render(<BenchmarkSection />);
  expect(observed.length).toBeGreaterThan(0);

  act(() => {
    reduced.matches = true;
    for (const listener of reduced.listeners) listener();
  });
  act(() => {
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
  });

  expect(requestAnimationFrame).not.toHaveBeenCalled();
});
