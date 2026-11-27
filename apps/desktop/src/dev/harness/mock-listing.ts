// ============================================================================
// Skill Studio - Dev Harness Listing
// The harness answer for `skill_listing`: two listing estimates and a Cursor folder scan.
// Claude keeps descriptions in skill
// order while they fit 1,000,000 tokens x 3 characters x the fraction, the
// shape of `skill_studio_core::listing_visibility`; enough for the view's flow.
// Each agent lists only the copies in the folders it reads, as the core does.
// ============================================================================

import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type {
  HarnessListing,
  InstalledSkill,
  ListingEntry,
  ListingReport,
  SkillSnapshot,
} from "@skill-studio/lib";

const CONTEXT_TOKENS = 1_000_000;

/** The deployment labels of the global folders each estimated agent reads. */
const READS = new Map([
  ["claude-code", ["Claude Code"]],
  ["codex", ["shared", "Codex"]],
]);

function entries(snapshot: SkillSnapshot, budget: number, harness: string): ListingEntry[] {
  const reads = READS.get(harness) ?? ["shared"];
  const copyOf = (skill: InstalledSkill) =>
    // The core skips a copy whose link points nowhere, so a broken copy is not listed.
    skill.deployments.find(
      (d) => d.scope === "global" && reads.includes(d.agent) && !d.symlink_is_broken,
    );
  const skills = snapshot.skills.filter((skill) => copyOf(skill));
  let left = budget - skills.reduce((sum, s) => sum + s.name.length + 3, 0);
  return skills.map((skill) => {
    const full = skill.description?.length ?? 0;
    const hidden = skill.parked || skill.invocation === "user-only";
    const fits = !hidden && full + 2 <= left;
    if (fits) left -= full + 2;
    return {
      name: skill.name,
      path: copyOf(skill)?.path ?? null,
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
  configuredTokens: string | null = null,
): HarnessListing {
  const list = entries(snapshot, budget, harness);
  if (harness === "codex") {
    list.push({
      name: "skill-installer",
      path: "~/.codex/skills/.system/skill-installer",
      state: "name_only",
      hidden_reason: null,
      shown_description_chars: 0,
      full_description_chars: 0,
      usage_score: null,
      observed: "name_only",
      scanned: false,
      observed_description_chars: 0,
      invocation_mismatch: null,
    });
  }
  return {
    harness,
    evidence: "preview",
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
            current: String(budget / CONTEXT_TOKENS / 3),
            file: "~/.claude/settings.json",
            chezmoi_source: "~/.local/share/chezmoi/private_dot_claude/private_settings.json.tmpl",
            snippet: `"skillListingBudgetFraction": ${budget / CONTEXT_TOKENS / 3}`,
          }
        : {
            key: "skills.max_context_tokens",
            current: configuredTokens,
            edit_revision: configuredTokens ?? "default",
            file: "~/.codex/config.toml",
            snippet: `[skills]\nmax_context_tokens = ${budget}`,
            chezmoi_source: null,
          },
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
              "Every skill is listed, and descriptions are cut to about 48 characters. 16,602 more tokens would keep every description whole. 2,226 tokens are left before Codex drops all descriptions, about 130 more skills. The setting is capped at 10,000 tokens.",
          }
        : null,
    entries: list,
  };
}

/** A small default budget so a few fixture skills lose their description. */
export function mockListingReport(
  snapshot: SkillSnapshot,
  request: { claude_budget_fraction?: number | null; codex_max_context_tokens?: number | null },
  configuredTokens: string | null = null,
  agents?: string[],
): ListingReport {
  const fraction = request.claude_budget_fraction ?? 0.0001;
  const report: ListingReport = {
    listings: [
      row("claude-code", Math.floor(CONTEXT_TOKENS * 3 * fraction), "characters", snapshot),
      row(
        "codex",
        request.codex_max_context_tokens ?? Number(configuredTokens ?? 5440),
        "tokens",
        snapshot,
        configuredTokens,
      ),
      {
        ...row("cursor", Number.MAX_SAFE_INTEGER, "characters", snapshot),
        evidence: "discovery",
        setting: null,
        observed_session: null,
        budget: null,
        context_window_tokens: null,
        budget_source: "no known limit",
        context_source: "not used",
        entries: snapshot.skills.flatMap((skill) => {
          const d = skill.deployments.find(
            (copy) =>
              copy.scope === "global" &&
              ["Cursor", "shared", "Claude Code", "Codex"].includes(copy.agent),
          );
          if (!d) return [];
          const chars = skill.description?.length ?? 0;
          return [
            {
              name: skill.name,
              path: d.path,
              state: skill.parked ? ("hidden" as const) : ("described" as const),
              hidden_reason: skill.parked ? "parked" : null,
              scanned: true,
              observed: null,
              observed_description_chars: null,
              full_description_chars: chars,
              shown_description_chars: skill.parked ? 0 : chars,
              usage_score: null,
              invocation_mismatch: null,
            },
          ];
        }),
      },
    ],
  };
  // A persona with fewer agents gets only their rows, as the real report lists installed agents.
  // Agents with no estimate in this mock (OpenCode, pi, Grok Build) get a folder-scan row like
  // Cursor's, listing the same global skills, so their counts are not zero.
  if (!agents) return report;
  const cursor = report.listings.find((row) => row.harness === "cursor");
  const discovered = agents
    .filter((agent) => !report.listings.some((row) => row.harness === agent))
    .flatMap((agent) => {
      if (!cursor) return [];
      // Each agent reads its own folders and the shared one, not Cursor's.
      const label = deploymentLabelFromAgentId(agent);
      const reads = new Set(
        snapshot.skills
          .filter((skill) =>
            skill.deployments.some(
              (d) =>
                d.scope === "global" &&
                !d.symlink_is_broken &&
                (d.agent === label || d.agent === "shared"),
            ),
          )
          .map((skill) => skill.name),
      );
      return [
        { ...cursor, harness: agent, entries: cursor.entries.filter((e) => reads.has(e.name)) },
      ];
    });
  return {
    ...report,
    listings: [...report.listings, ...discovered].filter((row) => agents.includes(row.harness)),
  };
}
