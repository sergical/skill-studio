// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { paceWhileVisible } from "./WarpDither";

let now = 0;
let nextFrame: FrameRequestCallback | null = null;
let deliver: (intersecting: boolean[]) => void = () => {};

/** Runs the display's vsync for `ms` milliseconds at `hz`. */
function runDisplay(hz: number, ms: number) {
  const end = now + ms;
  while (now + 1000 / hz <= end + 0.001) {
    now += 1000 / hz;
    const callback = nextFrame;
    nextFrame = null;
    callback?.(now);
  }
}

beforeEach(() => {
  now = 0;
  nextFrame = null;
  deliver = () => {};
  vi.spyOn(performance, "now").mockImplementation(() => now);
  vi.stubGlobal(
    "requestAnimationFrame",
    vi.fn((callback: FrameRequestCallback) => {
      nextFrame = callback;
      return 1;
    }),
  );
  vi.stubGlobal(
    "cancelAnimationFrame",
    vi.fn(() => {
      nextFrame = null;
    }),
  );
  vi.stubGlobal(
    "IntersectionObserver",
    class implements IntersectionObserver {
      readonly root = null;
      readonly rootMargin = "0px";
      readonly thresholds = [0];
      readonly scrollMargin = "0px";
      constructor(callback: IntersectionObserverCallback) {
        deliver = (states) => {
          const target = document.body;
          const rect = target.getBoundingClientRect();
          const entries = states.map((isIntersecting) => ({
            target,
            isIntersecting,
            intersectionRatio: isIntersecting ? 1 : 0,
            time: now,
            boundingClientRect: rect,
            intersectionRect: rect,
            rootBounds: null,
          }));
          callback(entries, this);
        };
      }
      observe() {}
      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
    },
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Starts the pacer and returns its step count and the animation time it reported. */
function startPacer() {
  const steps: number[] = [];
  const stop = paceWhileVisible(document.body, (ms) => steps.push(ms));
  const total = () => steps.reduce((sum, ms) => sum + ms, 0);
  return { steps, stop, total };
}

describe("paceWhileVisible", () => {
  it.each([60, 120, 144])("draws 30 frames a second on a %i Hz display", (hz) => {
    const pacer = startPacer();
    deliver([true]);

    runDisplay(hz, 10_000);
    pacer.stop();

    expect(pacer.steps).toHaveLength(300);
    expect(pacer.total()).toBeGreaterThan(9_990);
    expect(pacer.total()).toBeLessThan(10_000.001);
  });

  it("draws nothing while off screen, and reads the newest of queued entries", () => {
    const pacer = startPacer();

    deliver([true, false]);
    runDisplay(60, 500);
    expect(pacer.steps).toHaveLength(0);

    deliver([false, true]);
    runDisplay(60, 500);
    pacer.stop();
    expect(pacer.steps).toHaveLength(15);
  });

  it("counts time off screen as one frame", () => {
    const pacer = startPacer();
    deliver([true]);
    runDisplay(60, 1000);
    deliver([false]);
    runDisplay(60, 5000);
    deliver([true]);
    runDisplay(60, 1000);
    pacer.stop();

    expect(pacer.total()).toBeCloseTo(2000, 0);
  });

  it("counts a hidden tab as one frame, and draws the next frame on time", () => {
    const pacer = startPacer();
    deliver([true]);
    runDisplay(60, 1000);
    // A hidden tab gets no display frames.
    now += 5000;
    runDisplay(60, 1000);
    pacer.stop();

    expect(pacer.steps.slice(30, 32)).toEqual([1000 / 30, expect.closeTo(1000 / 30, 5)]);
    expect(pacer.total()).toBeCloseTo(2000, 0);
  });

  it("stops drawing when stopped", () => {
    const pacer = startPacer();
    deliver([true]);
    pacer.stop();
    runDisplay(60, 1000);

    expect(pacer.steps).toHaveLength(0);
  });
});
