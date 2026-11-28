// ============================================================================
// useNow - The current time for text that ages on screen, such as "5m ago".
// It is the one interval a component may hold.
// ============================================================================

import { useEffect, useState } from "react";

/** `Date.now()`, read again every `intervalMs` while the component is mounted. */
export function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return now;
}
