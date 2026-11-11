// ============================================================================
// Skill Studio - Store install trust-prompt tests (review round 2, B1)
// ============================================================================

import { describe, expect, it } from "vitest";
import { parentProgressForPhase } from "./store-install-flow";

describe("store install trust prompt", () => {
  it("store_install_reaching_needs_trust_clears_the_parent_progress_or_names_the_phase_left_showing", () => {
    // Names the phase left showing: a build that leaves the `InstallProgressModal`
    // tracking would return "keep" here, not "clear" - the parent's spinner then sits
    // over the trust prompt with no way to dismiss it.
    expect(parentProgressForPhase("needs-trust")).toBe("clear");
  });

  it("store_install_declined_at_the_trust_prompt_clears_the_parent_progress_or_names_the_phase_left_showing", () => {
    // Decline settles the paused operation on `cancelled` (see the decline test
    // above); that phase must also clear the parent's modal.
    expect(parentProgressForPhase("cancelled")).toBe("clear");
  });
});
