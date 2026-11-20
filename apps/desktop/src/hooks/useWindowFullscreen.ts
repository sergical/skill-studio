// ============================================================================
// Skill Studio - useWindowFullscreen
// True while the window is in macOS full screen, where the window buttons
// (traffic lights) are hidden and the sidebar needs no room for them.
// ============================================================================

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** How long the window must stop resizing before the state is read again: the full-screen animation outlasts its first resize events. */
const SETTLE_MS = 400;

/** The two calls of a Tauri window that the full-screen watch needs. */
interface FullscreenWindow {
  isFullscreen(): Promise<boolean>;
  onResized(handler: () => void): Promise<() => void>;
}

/**
 * Reports the window's full-screen state now, and again when the window stops resizing.
 * Entering or leaving full screen resizes the window, but during the animation the window
 * still reports its old state (measured: leaving full screen read `true`). Returns a stop function.
 */
export function watchFullscreen(
  appWindow: FullscreenWindow,
  onChange: (isFullscreen: boolean) => void,
): () => void {
  let stopped = false;
  let settle: ReturnType<typeof setTimeout> | undefined;
  const read = () => {
    // A resize event can arrive after stop while the listener is still registering.
    if (stopped) return;
    appWindow
      .isFullscreen()
      .then((next) => {
        if (!stopped) onChange(next);
      })
      // A failed read keeps the last known state.
      .catch(() => {});
  };
  read();
  const unlisten = appWindow.onResized(() => {
    clearTimeout(settle);
    settle = setTimeout(read, SETTLE_MS);
  });
  return () => {
    stopped = true;
    clearTimeout(settle);
    void unlisten.then((stop) => stop()).catch(() => {});
  };
}

export function useWindowFullscreen(): boolean {
  const [isFullscreen, setIsFullscreen] = useState(false);

  useEffect(() => {
    // Outside the Tauri window (tests, a plain browser) there is no window to read.
    if (!("__TAURI_INTERNALS__" in window)) return;
    return watchFullscreen(getCurrentWindow(), setIsFullscreen);
  }, []);

  return isFullscreen;
}
