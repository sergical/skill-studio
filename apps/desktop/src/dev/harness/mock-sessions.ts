// ============================================================================
// Skill Studio - Dev Harness Sessions
// The harness answer for `skill_sessions`: three Claude Code sessions over
// the fixture's skills. The first skill is used while listed by name only,
// the second is listed in every session and never used, and the third is a
// possible miss. Enough for the Sessions page's flow.
// ============================================================================

import type { SkillSessionRollup, SkillSessionsReport, SkillSnapshot } from "@skill-studio/lib";

export function mockSkillSessionsReport(snapshot: SkillSnapshot): SkillSessionsReport {
  const [used, idle, missed] = snapshot.skills.map((s) => s.name);
  const rollup = (
    name: string,
    counts: Partial<SkillSessionRollup>,
    flags: SkillSessionRollup["flags"],
  ): SkillSessionRollup => ({
    harness: "claude-code",
    name,
    path: null,
    described: 0,
    truncated: 0,
    name_only: 0,
    not_listed: 0,
    used: 0,
    used_name_only: 0,
    used_not_listed: 0,
    missed: 0,
    now: "described",
    reach: null,
    flags,
    ...counts,
  });
  const session = (index: number) => ({
    harness: "claude-code",
    id: `s${index}`,
    transcript: `/home/.claude/projects/-work/s${index}.jsonl`,
    project_path: "/work/app",
    at: `2026-11-1${index}T10:00:00Z`,
    model: "claude-opus-5-5",
    prompt: index === 3 ? `Make ${missed} work here` : `Session ${index}`,
    listing_recorded: true,
    listed: 3,
    described: 2,
    name_only: 1,
    entries: [
      { skill: used, listed_as: "name_only" as const },
      { skill: idle, listed_as: "described" as const },
      { skill: missed, listed_as: "described" as const },
    ],
    used:
      index === 1
        ? [{ skill: used, listed_as: "name_only" as const, trigger: "agent" as const }]
        : [],
    missed:
      index === 3
        ? [{ skill: missed, listed_as: "described" as const, score: 12.5, words: ["work"] }]
        : [],
  });
  return {
    history_days: 30,
    skills: [
      rollup(used, { name_only: 3, used: 1, used_name_only: 1, now: "name_only" }, [
        "used_while_name_only",
        "not_described_now",
      ]),
      rollup(idle, { described: 3 }, ["listed_never_used"]),
      rollup(missed, { described: 3, missed: 1 }, ["possibly_missed"]),
    ],
    sessions: [session(3), session(2), session(1)],
    agents: [
      {
        harness: "claude-code",
        records: "listing_and_uses",
        note: "Claude Code records each session's skill listing and its skill uses.",
        doc: "https://code.claude.com/docs/en/skills",
        sessions_read: 3,
        sessions_with_listing: 3,
        sessions_with_use: 1,
        sessions_with_miss: 1,
      },
      {
        harness: "cursor",
        records: "uses_only",
        note: "Cursor records skill uses but not what it listed for the model.",
        doc: "https://cursor.com/docs/skills",
        sessions_read: 0,
        sessions_with_listing: 0,
        sessions_with_use: 0,
        sessions_with_miss: 0,
      },
    ],
  };
}
