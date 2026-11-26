// ============================================================================
// Skill Studio - Dev Harness Personas
// `?persona=<id>` swaps the harness estate for one of twenty very different
// setups, from an empty first run to 250 skills on six agents, so every
// screen can be checked against states a real user can be in. Each persona
// also names the one thing Home should put first for it (`expectTop`).
// ============================================================================

import { collectDashboardIssues } from "@skill-studio/lib";
import type { InstalledSkill, SkillSnapshot } from "@skill-studio/lib";
import { buildHarnessSnapshot } from "./skill-fixture";

export interface Persona {
  label: string;
  /** Agents the persona has installed, the first-run screen's detected rows. */
  agents: string[];
  /** Claude Code's skill-space fraction in the listing mock; smaller fits fewer descriptions. */
  claudeFraction?: number;
  /** Words the first Home to-do should contain, or null when nothing needs the user. */
  expectTop: string | null;
  build: () => SkillSnapshot;
}

const ALL = ["claude-code", "codex", "opencode", "pi", "cursor", "grok-build"];

/** `count` healthy, current skills from the fixture, then `edit` applied to each. */
function estate(count: number, edit: (skill: InstalledSkill, index: number) => InstalledSkill) {
  const base = buildHarnessSnapshot(count + 60);
  const skills = base.skills
    .filter((skill) => !skill.parked && !skill.update_owner_ids.length)
    .filter((skill) => collectDashboardIssues([skill]).length === 0)
    .slice(0, count)
    .map(edit);
  return { ...base, skills, update_check: { ...base.update_check, updates_available: 0 } };
}

/** Gives the first `used` skills a 30-day history on `agent`, `perSkill` uses each spread over a
 * week, and every other skill none. */
function withUses(snapshot: SkillSnapshot, used: number, perSkill = 3, agent = "claude-code") {
  const now = Math.floor(Date.parse(snapshot.scanned_at) / 3_600_000);
  const invocations = snapshot.skills.slice(0, used).map((skill, index) => {
    const hours = Array.from({ length: 7 }, (_, day) => now - day * 24 - (index % 12));
    const by_hour = hours.map((hour, day) => ({
      hour,
      harness: agent,
      trigger: "agent" as const,
      project_path: null,
      count: Math.ceil(perSkill / 7) + (day === 0 ? perSkill % 7 : 0),
    }));
    const total = by_hour.reduce((sum, hour) => sum + hour.count, 0);
    const by_day = Object.fromEntries(
      hours.map((hour, day) => [
        new Date(hour * 3_600_000).toISOString().slice(0, 10),
        by_hour[day].count,
      ]),
    );
    return {
      skill: skill.name,
      last_24_hours: by_hour[0].count,
      last_7_days: total,
      last_14_days: total,
      last_30_days: total,
      last_used: new Date(hours[0] * 3_600_000).toISOString(),
      by_project_30_days: {},
      by_day,
      by_harness_30_days: { [agent]: total },
      by_trigger_30_days: { user: 0, agent: total, file_read: 0 },
      by_hour,
    };
  });
  return { ...snapshot, invocations };
}

const broken = (skill: InstalledSkill): InstalledSkill => ({
  ...skill,
  deployments: skill.deployments.map((d, index) =>
    index === 0 ? { ...d, is_symlink: true, symlink_is_broken: true } : d,
  ),
});

const updateTemplate = buildHarnessSnapshot().skills.find((skill) => skill.update_owner_ids.length);
const outdated = (skill: InstalledSkill): InstalledSkill => ({
  ...skill,
  has_update: true,
  update_owner_ids: updateTemplate?.update_owner_ids ?? ["fixture"],
  update_owners: updateTemplate?.update_owners ?? [],
  update_commit: updateTemplate?.update_commit ?? null,
});

const youOnly = (skill: InstalledSkill): InstalledSkill => ({
  ...skill,
  invocation: "user-only",
  deployments: skill.deployments.map((d) => ({ ...d, invocation: "user-only" })),
});

const plugin = (skill: InstalledSkill): InstalledSkill => ({
  ...skill,
  source_kind: "plugin",
  deployments: skill.deployments.map((d) => ({ ...d, scope: "plugin", agent: "Claude Code" })),
});

function updates(snapshot: SkillSnapshot): SkillSnapshot {
  const count = snapshot.skills.filter((skill) => skill.has_update).length;
  return { ...snapshot, update_check: { ...snapshot.update_check, updates_available: count } };
}

const same = (skill: InstalledSkill) => skill;

export const PERSONAS = {
  empty: {
    label: "Brand new: no skills",
    agents: ["claude-code"],
    expectTop: null,
    build: () => ({ ...buildHarnessSnapshot(), skills: [], invocations: [] }),
  },
  "first-week": {
    label: "Three skills, no history yet",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: null,
    build: () => withUses(estate(3, same), 0),
  },
  tidy: {
    label: "Twelve skills, all used, all fit",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: null,
    build: () => withUses(estate(12, same), 12),
  },
  hoarder: {
    label: "250 skills, ten used",
    agents: ["claude-code"],
    claudeFraction: 0.0002,
    expectTop: "names of",
    build: () => withUses(estate(250, same), 10),
  },
  "broken-heavy": {
    label: "Thirty skills, eight broken",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "broken",
    build: () =>
      withUses(
        estate(30, (s, i) => (i < 8 ? broken(s) : s)),
        20,
      ),
  },
  "update-backlog": {
    label: "Forty skills, 25 updates",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "updates",
    build: () =>
      updates(
        withUses(
          estate(40, (s, i) => (i < 25 ? outdated(s) : s)),
          30,
        ),
      ),
  },
  "codex-only": {
    label: "Codex only, descriptions cut",
    agents: ["codex"],
    expectTop: "Codex",
    build: () => withUses(estate(60, same), 15, 3, "codex"),
  },
  "six-agents": {
    label: "Every agent, the default estate",
    agents: ALL,
    expectTop: "broken",
    build: () => buildHarnessSnapshot(),
  },
  "plugin-heavy": {
    label: "Most skills come from plugins",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "no use",
    build: () =>
      withUses(
        estate(30, (s, i) => (i < 24 ? plugin(s) : s)),
        6,
      ),
  },
  "no-history": {
    label: "Skills, but no readable history",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: null,
    build: () => withUses(estate(25, same), 0),
  },
  "heavy-user": {
    label: "Thirty skills used hundreds of times",
    agents: ["claude-code", "codex"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () => withUses(estate(30, same), 30, 40),
  },
  "you-only": {
    label: "Most skills set to You run it",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "no use",
    build: () =>
      withUses(
        estate(40, (s, i) => (i < 32 ? youOnly(s) : s)),
        20,
      ),
  },
  "names-only": {
    label: "Claude Code fits almost nothing",
    agents: ["claude-code"],
    claudeFraction: 0.00005,
    expectTop: "names of",
    build: () => withUses(estate(40, same), 25),
  },
  "unused-pile": {
    label: "Sixty skills, five used, all fit",
    agents: ["claude-code"],
    claudeFraction: 0.05,
    expectTop: "no use",
    build: () => withUses(estate(60, same), 5),
  },
  "cursor-only": {
    label: "Cursor only, no limit to estimate",
    agents: ["cursor"],
    expectTop: null,
    build: () => withUses(estate(20, same), 20, 3, "cursor"),
  },
  "two-agents": {
    label: "Claude Code and Codex, medium estate",
    agents: ["claude-code", "codex"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () => withUses(estate(45, same), 20),
  },
  "everything-wrong": {
    label: "Broken, outdated and over the limit",
    agents: ["claude-code", "codex"],
    claudeFraction: 0.0002,
    expectTop: "broken",
    build: () =>
      updates(
        withUses(
          estate(120, (s, i) => (i < 4 ? broken(s) : i < 30 ? outdated(s) : s)),
          8,
        ),
      ),
  },
  "pi-only": {
    label: "pi only",
    agents: ["pi"],
    expectTop: null,
    build: () => withUses(estate(15, same), 15, 3, "pi"),
  },
  "single-broken": {
    label: "One broken skill, otherwise fine",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "broken",
    build: () =>
      withUses(
        estate(10, (s, i) => (i === 0 ? broken(s) : s)),
        10,
      ),
  },
  "big-tidy": {
    label: "150 skills, generous space, all used",
    agents: ["claude-code"],
    claudeFraction: 0.05,
    expectTop: null,
    build: () => withUses(estate(150, same), 150),
  },
} satisfies Record<string, Persona>;

export function personaFromUrl(): (Persona & { id: string }) | null {
  const id = new URLSearchParams(location.search).get("persona");
  const entry = Object.entries(PERSONAS).find(([key]) => key === id);
  return entry ? { id: entry[0], ...entry[1] } : null;
}
