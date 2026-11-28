// ============================================================================
// useBackendEvent - Listens to one backend event stream while the component
// is mounted. `subscribe` is a `skill-api.ts` listener such as
// `onUpdateStatus`: it registers a handler and resolves to the function that
// removes it.
// ============================================================================

import { useEffect, useEffectEvent } from "react";
import { invokeErrorMessage } from "../lib/skill-command-error";

/**
 * Calls `onEvent` with each payload. `onEvent` and `onFailed` may read props and state: each call
 * uses the function from the latest render. `onFailed` gets the message when the listener could
 * not be registered. A listener that registers after the unmount is removed at once.
 */
export function useBackendEvent<T>(
  subscribe: (handler: (payload: T) => void) => Promise<() => void>,
  onEvent: (payload: T) => void,
  onFailed?: (message: string) => void,
): void {
  const handleEvent = useEffectEvent(onEvent);
  const handleFailure = useEffectEvent((message: string) => onFailed?.(message));

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;
    subscribe((payload) => {
      if (mounted) handleEvent(payload);
    }).then(
      (stop) => {
        if (mounted) unlisten = stop;
        else stop();
      },
      (cause) => {
        if (mounted) handleFailure(invokeErrorMessage(cause));
      },
    );
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [subscribe]);
}
