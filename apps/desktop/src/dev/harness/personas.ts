// ============================================================================
// Skill Studio - Dev Harness Personas
// `?persona=<id>` swaps the harness estate for one of twenty very different
// setups, from an empty first run to 250 skills on six agents, so every
// screen can be checked against states a real user can be in. Each persona
// also names the one thing Home's prompt should put first for it (`expectTop`).
// ============================================================================

import { collectDashboardIssues } from "@skill-studio/lib";
import type { InstalledSkill, SkillSnapshot } from "@skill-studio/lib";
import { withScannerIdentity } from "./scanned-deployment";
import { buildHarnessSnapshot } from "./skill-fixture";

export interface Persona {
  label: string;
  /** Agents the persona has installed, the first-run screen's detected rows. */
  agents: string[];
  /** Claude Code's skill-space fraction in the listing mock; smaller fits fewer descriptions. */
  claudeFraction?: number;
  /** Words the first line of Home's prompt should contain, or null when nothing needs the user. */
  expectTop: string | null;
  build: () => SkillSnapshot;
}

const ALL = ["claude-code", "codex", "opencode", "pi", "cursor", "grok-build"];

/** `count` healthy, current skills from the fixture, then `edit` applied to each. */
function estate(count: number, edit: (skill: InstalledSkill, index: number) => InstalledSkill) {
  const base = buildHarnessSnapshot(count + 60);
  const skills = base.skills
    .filter(
      (skill) =>
        !skill.parked &&
        !skill.update_owner_ids.length &&
        collectDashboardIssues([skill]).length === 0,
    )
    .slice(0, count)
    .map(edit);
  return { ...base, skills, update_check: { ...base.update_check, updates_available: 0 } };
}

/** How a persona's history is spread: which agents recorded it (skills take turns), which days
 * (`from` days ago, over `span` days), and whether the first skill takes most of the uses. */
interface UsePattern {
  agents?: string[];
  from?: number;
  span?: number;
  dominant?: boolean;
}

/** Gives the first `used` skills a history of `perSkill` uses each, spread as `pattern` says,
 * and every other skill none. */
function withUses(
  snapshot: SkillSnapshot,
  used: number,
  perSkill = 3,
  agentOrPattern: string | UsePattern = "claude-code",
) {
  const pattern: UsePattern =
    agentOrPattern instanceof Object ? agentOrPattern : { agents: [agentOrPattern] };
  const agents = pattern.agents ?? ["claude-code"];
  const from = pattern.from ?? 0;
  const span = pattern.span ?? 7;
  const now = Math.floor(Date.parse(snapshot.scanned_at) / 3_600_000);
  const invocations = snapshot.skills.slice(0, used).map((skill, index) => {
    const agent = agents[index % agents.length];
    const uses = pattern.dominant && index === 0 ? perSkill * 20 : perSkill;
    const hours = Array.from({ length: span }, (_, day) => now - (from + day) * 24 - (index % 12));
    const by_hour = hours.flatMap((hour, day) => {
      const count = Math.floor(uses / span) + (day < uses % span ? 1 : 0);
      return count > 0
        ? [{ hour, harness: agent, trigger: "agent" as const, project_path: null, count }]
        : [];
    });
    const total = by_hour.reduce((sum, hour) => sum + hour.count, 0);
    const within = (days: number) =>
      by_hour.filter((hour) => now - hour.hour < days * 24).reduce((t, h) => t + h.count, 0);
    const by_day = Object.fromEntries(
      by_hour.map((hour) => [
        new Date(hour.hour * 3_600_000).toISOString().slice(0, 10),
        hour.count,
      ]),
    );
    return {
      skill: skill.name,
      last_24_hours: within(1),
      last_7_days: within(7),
      last_14_days: within(14),
      last_30_days: total,
      last_used: by_hour[0] ? new Date(by_hour[0].hour * 3_600_000).toISOString() : null,
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
/** Each skill gets its own owner, so updating one clears that skill and no other. */
const outdated = (skill: InstalledSkill): InstalledSkill => {
  const ownerId = `owner:v1/dotagents/${skill.name}`;
  const template = updateTemplate?.update_owners[0];
  return {
    ...skill,
    has_update: true,
    update_owner_ids: [ownerId],
    update_owners: template ? [{ ...template, owner_id: ownerId }] : [],
    update_commit: updateTemplate?.update_commit ?? null,
  };
};

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

/** Moves a skill's shared-folder copies into one agent's own folder, e.g. `~/.claude/skills/<name>`. */
const inAgentFolder =
  (agent: string, dir: string) =>
  (skill: InstalledSkill): InstalledSkill => ({
    ...skill,
    deployments: skill.deployments.map((d) =>
      d.agent === "shared"
        ? withScannerIdentity({
            ...d,
            agent,
            path: d.path.replace("/.agents/skills/", `/${dir}/skills/`),
          })
        : d,
    ),
  });

// A Claude Code user's own copies sit in Claude Code's folder, which its history covers.
const inClaudeCode = inAgentFolder("Claude Code", ".claude");
const inCursor = inAgentFolder("Cursor", ".cursor");

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
    expectTop: "by name only",
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
    expectTop: "not used in 30 days",
    build: () =>
      withUses(
        estate(30, (s, i) => (i < 24 ? plugin(s) : inClaudeCode(s))),
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
    expectTop: "not used in 30 days",
    build: () =>
      withUses(
        estate(40, (s, i) => (i < 32 ? youOnly(s) : inClaudeCode(s))),
        20,
      ),
  },
  "names-only": {
    label: "Claude Code fits almost nothing",
    agents: ["claude-code"],
    claudeFraction: 0.00005,
    expectTop: "by name only",
    build: () => withUses(estate(40, same), 25),
  },
  "unused-pile": {
    label: "Sixty skills, five used, all fit",
    agents: ["claude-code"],
    claudeFraction: 0.05,
    expectTop: "not used in 30 days",
    build: () => withUses(estate(60, same), 5),
  },
  "claude-history-cursor-skill": {
    label: "Claude Code history, one skill only Cursor reads",
    agents: ["claude-code", "cursor"],
    claudeFraction: 0.01,
    expectTop: "not used in 30 days",
    build: () => {
      const base = estate(60, same);
      // Described skills whose one copy is in the shared folder, so each moves whole into one
      // agent's folder.
      const skills = base.skills
        .filter(
          (s) =>
            s.description &&
            s.deployments.every((d) => d.agent === "shared" && d.scope === "global"),
        )
        .slice(0, 15)
        .map((s, i) => (i === 14 ? inCursor(s) : inClaudeCode(s)));
      return withUses({ ...base, skills }, 5);
    },
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
  "one-dominant": {
    label: "One skill does nearly all the work",
    agents: ["claude-code", "codex"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () =>
      withUses(estate(40, same), 12, 3, { agents: ["claude-code", "codex"], dominant: true }),
  },
  "quiet-this-week": {
    label: "Used last week, nothing this week",
    agents: ["claude-code"],
    claudeFraction: 0.01,
    expectTop: "not used in 30 days",
    build: () => withUses(estate(30, same), 12, 6, { from: 8, span: 6 }),
  },
  "two-weeks-steady": {
    label: "Two steady weeks on three agents",
    agents: ["claude-code", "codex", "pi"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () =>
      withUses(estate(60, same), 40, 14, { agents: ["claude-code", "codex", "pi"], span: 14 }),
  },
  "all-six-spread": {
    label: "All six agents, use spread over them",
    agents: ALL,
    claudeFraction: 0.002,
    expectTop: "by name only",
    build: () => withUses(estate(90, same), 48, 6, { agents: ALL, span: 14 }),
  },
  "grok-only": {
    label: "Grok Build only",
    agents: ["grok-build"],
    expectTop: null,
    build: () => withUses(estate(12, same), 0),
  },
  "opencode-active": {
    label: "OpenCode alone, used every day",
    agents: ["opencode"],
    expectTop: "not used in 30 days",
    build: () => withUses(estate(30, same), 12, 7, { agents: ["opencode"], span: 14 }),
  },
  "mixed-history": {
    label: "Four agents with very different history",
    agents: ["claude-code", "codex", "cursor", "grok-build"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () => {
      const base = estate(40, same);
      const claude = withUses(base, 20, 20, { agents: ["claude-code"], span: 14 });
      const codex = withUses({ ...base, skills: base.skills.slice(20) }, 6, 4, {
        agents: ["codex"],
        span: 2,
      });
      return { ...base, invocations: [...claude.invocations, ...codex.invocations] };
    },
  },
  "one-broken-copy": {
    label: "Three agents share skills, one copy broken",
    agents: ["claude-code", "codex", "pi"],
    claudeFraction: 0.01,
    expectTop: "broken",
    build: () =>
      withUses(
        estate(20, (s, i) => (i === 0 ? broken(s) : s)),
        15,
        5,
        {
          agents: ["claude-code", "codex", "pi"],
          span: 14,
        },
      ),
  },
  "long-names": {
    label: "Five agents, long skill names",
    agents: ["claude-code", "codex", "opencode", "pi", "cursor"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () =>
      withUses(
        estate(30, (s, i) =>
          i < 8 ? { ...s, name: `${s.name}-for-the-internal-platform-migration-${i}` } : s,
        ),
        20,
        6,
        { agents: ["claude-code", "codex", "opencode", "pi", "cursor"], span: 14 },
      ),
  },
  comeback: {
    label: "A quiet week, then a busy one",
    agents: ["claude-code", "codex"],
    claudeFraction: 0.01,
    expectTop: "Codex",
    build: () => withUses(estate(40, same), 25, 12, { agents: ["claude-code", "codex"], span: 5 }),
  },
} satisfies Record<string, Persona>;

export function personaFromUrl(): (Persona & { id: string }) | null {
  const id = new URLSearchParams(location.search).get("persona");
  const entry = Object.entries(PERSONAS).find(([key]) => key === id);
  return entry ? { id: entry[0], ...entry[1] } : null;
}
