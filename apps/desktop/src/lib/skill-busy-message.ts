// ============================================================================
// Skill Studio - skill-busy-message
// Plain wording for a write that bounced off the write lease
// ============================================================================

/** The two texts a lease refusal reaches the UI as: the core's own, and the desktop write lease's. */
const BUSY_PATTERN = /another process holds the lease|another write is in progress/i;

export const BUSY_MESSAGE = "Another change is still running. Try again when it finishes.";
export const UPDATE_ALL_BUSY_MESSAGE = "Update all is still running. Cancel it or wait.";

let updateAllRunning = false;

/** Home's "Update all" calls this around its run, so a refused write can name the cause. */
export function setUpdateAllRunning(running: boolean): void {
  updateAllRunning = running;
}

export function isUpdateAllRunning(): boolean {
  return updateAllRunning;
}

/** Replaces a lease-refusal message with plain words; any other message passes through. */
export function plainBusyMessage(message: string, updateAllIsRunning: boolean): string {
  if (!BUSY_PATTERN.test(message)) return message;
  return updateAllIsRunning ? UPDATE_ALL_BUSY_MESSAGE : BUSY_MESSAGE;
}
