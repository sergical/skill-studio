// ============================================================================
// Skill Studio - useAddSkillOperation
// Subscribe before start, then catch up with get-status after remount
// ============================================================================

import { useEffect, useRef } from "react";
import type { RefObject } from "react";
import { onAddSkillOperation } from "../lib/skill-api";
import type { AddSkillOperationEvent } from "@skill-studio/lib";
import { selectNewerAddSkillOperationEvent } from "@skill-studio/lib";

interface AddSkillOperationListen {
  isCancelled: () => boolean;
  listen: typeof onAddSkillOperation;
  onEvent: (event: AddSkillOperationEvent) => void;
}

/**
 * Register the Add Skill listener and return before any start or status
 * read, so the sheet can subscribe with a known id first.
 */
export async function listenForAddSkillOperation({
  isCancelled,
  listen,
  onEvent,
}: AddSkillOperationListen): Promise<(() => void) | undefined> {
  const unlisten = await listen((candidate) => {
    if (!isCancelled()) onEvent(candidate);
  });
  if (isCancelled()) {
    unlisten();
    return undefined;
  }
  return unlisten;
}

/**
 * Where a component keeps the stop function of the Add Skill listener it has open. The listener
 * starts in a submit handler, so this hook only makes sure it stops when the component unmounts.
 */
export function useAddSkillListenerRef(): RefObject<(() => void) | undefined> {
  const unlistenRef = useRef<(() => void) | undefined>(undefined);

  useEffect(() => () => unlistenRef.current?.(), []);

  return unlistenRef;
}

/** Fold an incoming event into local state, dropping stale sequences. */
export function applyAddSkillOperationEvent(
  current: AddSkillOperationEvent | undefined,
  incoming: AddSkillOperationEvent,
  operationId: string | undefined,
): AddSkillOperationEvent | undefined {
  if (!operationId) return current;
  return selectNewerAddSkillOperationEvent(current, incoming, operationId);
}
