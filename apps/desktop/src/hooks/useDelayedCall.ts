// ============================================================================
// useDelayedCall - The one timer a component may hold: run a function once,
// later. A new call replaces the one still waiting, and unmounting cancels
// it. It covers a "Copied" label that resets, a hover delay and a debounce.
// ============================================================================

import { useEffect, useRef } from "react";

interface DelayedCall {
  /** Runs `call` after `delayMs`, in place of any call still waiting. */
  schedule: (call: () => void, delayMs: number) => void;
  /** Drops the call still waiting, if any. */
  cancel: () => void;
}

export function useDelayedCall(): DelayedCall {
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const cancel = () => clearTimeout(timer.current);
  const schedule = (call: () => void, delayMs: number) => {
    clearTimeout(timer.current);
    timer.current = setTimeout(call, delayMs);
  };

  useEffect(() => () => clearTimeout(timer.current), []);

  return { schedule, cancel };
}
