import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { watchFullscreen } from "./useWindowFullscreen";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

it("reads the full-screen state again once the window stops resizing, so leaving full screen is not missed", async () => {
  let fullscreen = true;
  let resize = () => {};
  let latest: boolean | undefined;
  const stop = watchFullscreen(
    {
      isFullscreen: () => Promise.resolve(fullscreen),
      onResized: (handler) => {
        resize = handler;
        return Promise.resolve(() => {});
      },
    },
    (next) => {
      latest = next;
    },
  );
  await vi.runAllTimersAsync();
  expect(latest).toBe(true);

  // Leaving full screen: the window resizes while macOS still reports full screen (measured),
  // then the animation ends and the state reads false.
  resize();
  await vi.advanceTimersByTimeAsync(100);
  resize();
  fullscreen = false;
  await vi.advanceTimersByTimeAsync(100);
  expect(latest).toBe(true);
  await vi.runAllTimersAsync();
  expect(latest).toBe(false);
  stop();
});
