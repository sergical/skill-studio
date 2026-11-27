import { expect, it } from "vitest";
import { PERSONAS } from "../../dev/harness/personas";
import { agentUses } from "./AgentUses";

it("counts an agent's own uses from history, and says nothing about uses without history", () => {
  const spread = PERSONAS["all-six-spread"].build();
  const total = (harness: string) =>
    agentUses(spread, harness)?.reduce((sum, row) => sum + row.uses, 0);
  for (const harness of PERSONAS["all-six-spread"].agents)
    expect(total(harness)).toBe(
      spread.invocations.reduce((sum, s) => sum + (s.by_harness_30_days[harness] ?? 0), 0),
    );
  expect(total("opencode")).toBeGreaterThan(0);
  expect(agentUses(PERSONAS["no-history"].build(), "claude-code")).toBeNull();
});
