import { useAppStore } from "../store/appStore";
import type { HarnessDetectionState } from "../store/appStore";
import { invokeErrorMessage, setHarnessEnabled } from "../lib/skill-api";

export function useDetectedHarnesses(): HarnessDetectionState {
  return useAppStore((state) => state.harnessDetection);
}

export function setHarnessDetection(update: Partial<HarnessDetectionState>): void {
  useAppStore.getState().setHarnessDetection(update);
}

export async function changeHarnessEnabled(id: string, enabled: boolean): Promise<void> {
  setHarnessDetection({ saving: true, error: null });
  try {
    const choice = await setHarnessEnabled(id, enabled);
    setHarnessDetection({ choice });
  } catch (cause) {
    setHarnessDetection({ error: invokeErrorMessage(cause) });
  } finally {
    setHarnessDetection({ saving: false });
  }
}
