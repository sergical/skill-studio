// ============================================================================
// Skill Studio - Store install trust-prompt tests (review round 2, B1)
// ============================================================================

import { describe, expect, it, vi } from "vitest";
import { confirmStoreInstallTrust, parentProgressForPhase } from "./store-install-flow";
import type { AddSkillOperationEvent } from "@skill-studio/lib";

function event(
  operationId: string,
  phase: AddSkillOperationEvent["phase"],
): AddSkillOperationEvent {
  return { operation_id: operationId, sequence: 1, phase, message: phase };
}

describe("store install trust prompt", () => {
  it("store_install_trust_confirm_sends_both_operation_ids_and_the_repo_then_reads_the_retry_or_names_the_wrong_argument", async () => {
    const confirmTrust = vi.fn(async () => event("op-2", "queued"));
    const getOperation = vi.fn(async (id: string) => event(id, "completed"));
    const settled = await confirmStoreInstallTrust("op-1", "op-2", "octocat/skills", {
      confirmTrust,
      getOperation,
    });
    expect(confirmTrust).toHaveBeenCalledWith("op-1", "op-2", "octocat/skills");
    // Reading "op-1" here would show the paused operation, not the install that ran.
    expect(getOperation).toHaveBeenCalledWith("op-2");
    expect(settled.operation_id).toBe("op-2");
  });

  it("store_install_reaching_needs_trust_clears_the_parent_progress_or_names_the_phase_left_showing", () => {
    // Names the phase left showing: a build that leaves the `InstallProgressModal`
    // tracking would return "keep" here, not "clear" - the parent's spinner then sits
    // over the trust prompt with no way to dismiss it.
    expect(parentProgressForPhase("needs-trust")).toBe("clear");
  });

  it("store_install_declined_at_the_trust_prompt_clears_the_parent_progress_or_names_the_phase_left_showing", () => {
    // Decline settles the paused operation on `cancelled`; that phase must also
    // clear the parent's modal.
    expect(parentProgressForPhase("cancelled")).toBe("clear");
  });
});
