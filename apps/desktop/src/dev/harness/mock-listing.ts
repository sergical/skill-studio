// ============================================================================
// Skill Studio - Dev Harness Listing
// The harness answer for `skill_listing`: the fixture's skills in one Claude
// Code listing and one Codex listing. Claude keeps descriptions in skill
// order while they fit 1,000,000 tokens x 3 characters x the fraction, the
// shape of `skill_studio_core::listing_visibility`; enough for the view's flow.
// ============================================================================

import type { HarnessListing, ListingEntry, ListingReport, SkillSnapshot } from "@skill-studio/lib";

const CONTEXT_TOKENS = 1_000_000;

function entries(snapshot: SkillSnapshot, budget: number): ListingEntry[] {
  const skills = snapshot.skills;
  let left = budget - skills.reduce((sum, s) => sum + s.name.length + 3, 0);
  return skills.map((skill) => {
    const full = skill.description?.length ?? 0;
    const hidden = skill.parked || skill.invocation === "user-only";
    const fits = !hidden && full + 2 <= left;
    if (fits) left -= full + 2;
    return {
      name: skill.name,
      path: skill.deployments[0]?.path ?? null,
      state: hidden ? "hidden" : fits ? "described" : "name_only",
      hidden_reason: skill.parked ? "parked" : hidden ? "manual-only" : null,
      shown_description_chars: fits ? full : 0,
      full_description_chars: full,
      usage_score: 0,
      observed: hidden ? "hidden" : fits ? "described" : "name_only",
      scanned: true,
      observed_description_chars: null,
      invocation_mismatch: null,
    };
  });
}

function row(
  harness: string,
  budget: number,
  unit: string,
  snapshot: SkillSnapshot,
): HarnessListing {
  const list = entries(snapshot, budget);
  return {
    harness,
    scope: { scope: "global" },
    budget,
    budget_unit: unit,
    full_size: list.reduce((sum, e) => sum + e.name.length + e.full_description_chars + 4, 0),
    budget_source: "fixture",
    context_window_tokens: CONTEXT_TOKENS,
    context_source: "fixture catalog",
    uses_default_context_window: false,
    notes: [],
    setting:
      harness === "claude-code"
        ? {
            key: "skillListingBudgetFraction",
            current: "0.0001",
            file: "~/.claude/settings.json",
            chezmoi_source: "~/.local/share/chezmoi/private_dot_claude/private_settings.json.tmpl",
            snippet: `"skillListingBudgetFraction": ${budget / CONTEXT_TOKENS / 3}`,
          }
        : null,
    observed_session: null,
    thresholds:
      harness === "codex"
        ? {
            tier: "descriptions_shortened",
            full_cost: 22042,
            names_only_cost: 3214,
            roots_table_cost: 212,
            average_line_cost: 17,
            average_description_chars: 48,
            cap: 10000,
            summary:
              "Every skill is listed, and descriptions are cut to about 48 characters. 16602 more tokens would keep every description whole. 2226 tokens are left before Codex drops all descriptions, about 130 more skills. The setting is capped at 10000 tokens.",
          }
        : null,
    entries: list,
  };
}

/** A small default budget so a few fixture skills lose their description. */
export function mockListingReport(
  snapshot: SkillSnapshot,
  request: { claude_budget_fraction?: number | null; codex_max_context_tokens?: number | null },
): ListingReport {
  const fraction = request.claude_budget_fraction ?? 0.0001;
  return {
    listings: [
      row("claude-code", Math.floor(CONTEXT_TOKENS * 3 * fraction), "characters", snapshot),
      row("codex", request.codex_max_context_tokens ?? 5440, "tokens", snapshot),
    ],
  };
}
