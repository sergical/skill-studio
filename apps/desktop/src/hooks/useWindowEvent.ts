// ============================================================================
// useWindowEvent - Listens to one `window` event while the component is
// mounted, for page-wide keys such as Escape.
// ============================================================================

import { useEffect, useEffectEvent } from "react";

/** `onEvent` may read props and state: each event calls the function from the latest render. */
export function useWindowEvent<K extends keyof WindowEventMap>(
  type: K,
  onEvent: (event: WindowEventMap[K]) => void,
): void {
  const handleEvent = useEffectEvent(onEvent);

  useEffect(() => {
    const listener = (event: WindowEventMap[K]) => handleEvent(event);
    window.addEventListener(type, listener);
    return () => window.removeEventListener(type, listener);
  }, [type]);
}
