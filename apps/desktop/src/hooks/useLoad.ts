// ============================================================================
// useLoad - Loads data for a component: the one place a component's read
// from the backend runs in an effect. It drops a result that a newer load or
// an unmount replaced, and reports loading without a state write in the
// effect.
// ============================================================================

import { useEffect, useEffectEvent, useState } from "react";
import { invokeErrorMessage } from "../lib/skill-command-error";

interface LoadOptions<T> {
  /** Show the last loaded data while a load for a new key runs. Off, a new key shows no data
   * until its own load lands, so data for one key never shows under another. */
  keepPreviousData?: boolean;
  /** Called with each result that lands, for a component that keeps its own state machine. */
  onLoaded?: (data: T) => void;
  /** Called with the message of each load that fails. */
  onFailed?: (message: string) => void;
}

interface Load<T> {
  /** The loaded data; `undefined` until a load lands. A failed reload keeps the data it had. */
  data: T | undefined;
  /** The message of the last load to land, when it failed; else `null`. It stays through a
   * reload, until that reload lands. */
  error: string | null;
  /** True while the latest load has not landed. */
  isLoading: boolean;
  /** Loads again for the same key and keeps the current data until the new result lands. */
  reload: () => void;
  /** Replaces the data in place, for a change the user just made. */
  setData: (data: T) => void;
}

interface Settled<T> {
  run: string;
  key: string;
  data: T | undefined;
  error: string | null;
}

/**
 * Calls `load` when the component mounts, each time `key` changes, and on `reload()`.
 * `key` names what is loaded (a path, an id, or a fixed word); `null` loads nothing.
 * `load` may read props and state: each run uses the function from the latest render.
 */
export function useLoad<T>(
  key: string | null,
  load: () => Promise<T>,
  options: LoadOptions<T> = {},
): Load<T> {
  const { keepPreviousData = false, onLoaded, onFailed } = options;
  const [revision, setRevision] = useState(0);
  const [settled, setSettled] = useState<Settled<T> | null>(null);
  const run = key === null ? null : `${revision}:${key}`;

  // An Effect Event, so it reads `run`, `key` and `load` from the render that scheduled the effect.
  const start = useEffectEvent((isCurrent: () => boolean) => {
    if (run === null || key === null) return;
    const [forRun, forKey] = [run, key];
    load().then(
      (data) => {
        if (!isCurrent()) return;
        setSettled({ run: forRun, key: forKey, data, error: null });
        onLoaded?.(data);
      },
      (cause) => {
        if (!isCurrent()) return;
        const error = invokeErrorMessage(cause);
        setSettled((previous) => ({
          run: forRun,
          key: forKey,
          data:
            previous && (keepPreviousData || previous.key === forKey) ? previous.data : undefined,
          error,
        }));
        onFailed?.(error);
      },
    );
  });

  useEffect(() => {
    if (run === null) return;
    let current = true;
    start(() => current);
    return () => {
      current = false;
    };
  }, [run]);

  const shown = settled !== null && (keepPreviousData || settled.key === key) ? settled : null;
  return {
    data: shown?.data,
    error: shown?.error ?? null,
    isLoading: run !== null && settled?.run !== run,
    reload: () => setRevision((current) => current + 1),
    setData: (data) => {
      if (run !== null && key !== null) setSettled({ run, key, data, error: null });
    },
  };
}
