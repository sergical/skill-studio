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

describe("paceWhileVisible", () => {
  it.each([60, 120])("draws 30 frames a second on a %i Hz display", (hz) => {
    const onFrame = vi.fn<(frame: number) => void>();
    const stop = paceWhileVisible(document.body, onFrame);
    deliver([true]);

    runDisplay(hz, 1000);
    stop();

    expect(onFrame).toHaveBeenCalledTimes(30);
    // Animation time runs at the shader speed, 0.35 ms per millisecond.
    expect(onFrame.mock.lastCall?.[0]).toBeCloseTo(350, 0);
  });

  it("draws nothing while off screen, and reads the newest of queued entries", () => {
    const onFrame = vi.fn<(frame: number) => void>();
    const stop = paceWhileVisible(document.body, onFrame);

    deliver([true, false]);
    runDisplay(60, 500);
    expect(onFrame).not.toHaveBeenCalled();

    deliver([false, true]);
    runDisplay(60, 500);
    stop();
    expect(onFrame).toHaveBeenCalledTimes(15);
  });

  it("goes on where it stopped when the target comes back on screen", () => {
    const onFrame = vi.fn<(frame: number) => void>();
    const stop = paceWhileVisible(document.body, onFrame);
    deliver([true]);
    runDisplay(60, 1000);
    deliver([false]);
    runDisplay(60, 5000);
    deliver([true]);
    runDisplay(60, 1000);
    stop();

    expect(onFrame.mock.lastCall?.[0]).toBeCloseTo(700, 0);
  });

  it("stops drawing when stopped", () => {
    const onFrame = vi.fn<(frame: number) => void>();
    const stop = paceWhileVisible(document.body, onFrame);
    deliver([true]);
    stop();
    runDisplay(60, 1000);

    expect(onFrame).not.toHaveBeenCalled();
  });
});
