// @vitest-environment happy-dom
// ============================================================================
// Skill Studio - Feature flags tests
// ============================================================================

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isFeatureEnabled } from "./feature-flags";
import type { FeatureFlag } from "./feature-flags";

// Compile-time guard: if "skill-packs" ever rejoins `FeatureFlag`, `Extract`
// yields "skill-packs" instead of `never` and this assignment stops
// typechecking - `npm run typecheck` then fails the same way a runtime
// assertion would. Packs are deferred with no runtime flag (unit 4.3).
type _NoSkillPacksFlag =
  Extract<FeatureFlag, "skill-packs"> extends never
    ? true
    : ["skill-packs must not be a FeatureFlag - see unit 4.3"];
const _noSkillPacksFlag: _NoSkillPacksFlag = true;
void _noSkillPacksFlag;

beforeEach(() => {
  localStorage.clear();
  vi.stubEnv("VITE_SKILL_ASSISTANT", "");
});
afterEach(() => vi.unstubAllEnvs());

describe("isFeatureEnabled", () => {
  it("allows explicit local opt-in", () => {
    localStorage.setItem("feature:skill-assistant", "on");
    expect(isFeatureEnabled("skill-assistant")).toBe(true);
  });
  it("allows a development-only opt-in, with local off taking precedence", () => {
    vi.stubEnv("DEV", true);
    vi.stubEnv("VITE_SKILL_ASSISTANT", "1");
    expect(isFeatureEnabled("skill-assistant")).toBe(true);
    localStorage.setItem("feature:skill-assistant", "off");
    expect(isFeatureEnabled("skill-assistant")).toBe(false);
  });
  it("ignores the development opt-in in a production build", () => {
    vi.stubEnv("DEV", false);
    vi.stubEnv("VITE_SKILL_ASSISTANT", "1");
    expect(isFeatureEnabled("skill-assistant")).toBe(false);
  });
  it("reads the skill-assistant default off with no localStorage override", () => {
    expect(isFeatureEnabled("skill-assistant")).toBe(false);
  });
});
