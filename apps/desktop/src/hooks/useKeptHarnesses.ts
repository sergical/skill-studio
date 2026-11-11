// ============================================================================
// useKeptHarnesses - the saved agent choice shared with Settings and welcome.
// ============================================================================

import { useAppStore } from "../store/appStore";

export function useKeptHarnesses(): string[] {
  const kept = useAppStore((state) => state.harnessDetection.choice?.kept);
  return kept ?? [];
}
