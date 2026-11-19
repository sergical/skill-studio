// ============================================================================
// Skill Studio - Dev Harness Packs
// The harness answers for `pack_list`, `pack_add`, `pack_update` and
// `pack_remove`: packs grouped from the fixture snapshot, with the same skip
// rules and summary lines as `skill_studio_core::pack_action`.
// ============================================================================

import type {
  InstalledSkill,
  Pack,
  PackActionReport,
  PackReport,
  PackSkill,
  PackStep,
  RootScope,
  SkillSnapshot,
} from "@skill-studio/lib";
import { deployment, HARNESS_HOME, skill } from "./skill-fixture";

/** Skills each git source ships upstream that the fixture does not install. */
const UPSTREAM_ONLY = new Map([["getsentry/skills", ["sentry-setup-logging"]]]);

/** Fixture skills whose folders differ from what their lock recorded. */
const EDITED_SKILLS = new Set(["api-client-codegen"]);

const KIND_BY_SOURCE = new Map<string, Pack["kind"]>([
  ["dotagents", "dotagents"],
  ["skills-sh", "skills_sh"],
  ["plugin", "plugin"],
]);

/** A mocked pack action: the report, and the snapshot after a real run. */
interface MockPackResult {
  report: PackActionReport;
  next: SkillSnapshot;
}

type StepAction = PackStep["action"];

function packSource(item: InstalledSkill): string {
  const plugin = item.deployments.find((d) => d.plugin)?.plugin;
  return plugin ? `${plugin.name}@${plugin.marketplace}` : item.source;
}

function scopeKey(scope: RootScope): string {
  return scope.scope === "global" ? "global" : scope.project;
}

function packSkills(item: InstalledSkill): PackSkill[] {
  const byScope = new Map<string, PackSkill>();
  for (const d of item.deployments) {
    const scope: RootScope =
      d.scope === "project" && d.project_path
        ? { scope: "project", project: d.project_path }
        : { scope: "global" };
    const row = byScope.get(scopeKey(scope)) ?? {
      name: item.name,
      scope,
      path: d.path,
      agents: [],
      edited: EDITED_SKILLS.has(item.name),
      behind: null,
    };
    row.agents.push(d.agent);
    byScope.set(scopeKey(scope), row);
  }
  return [...byScope.values()];
}

/** The packs `pack_list` would report for `snapshot`. */
export function mockPackReport(snapshot: SkillSnapshot, remote: boolean): PackReport {
  const packs = new Map<string, Pack>();
  for (const item of snapshot.skills) {
    const kind = KIND_BY_SOURCE.get(item.source_kind);
    // `pack_list` leaves out skills copied from a local folder. The scan says
    // "manual" for a dotagents `path:` folder, and also for a dotagents source
    // on a git host other than GitHub, which the fixture does not have.
    const source = packSource(item);
    if (!kind || item.parked || source === "manual" || item.source_type === "local") continue;
    const pack = packs.get(source) ?? {
      source,
      kind,
      git_url: kind === "plugin" ? null : `https://github.com/${source}.git`,
      ref_pin: null,
      update_commands: kind === "plugin" ? [`claude plugin update ${source}`] : [],
      scopes: [],
      skills: [],
      new_upstream: [],
      edited_count: 0,
      behind_count: 0,
      remote: { state: "not_checked" },
      original: null,
      summary: "",
    };
    for (const row of packSkills(item)) {
      row.behind = remote && kind !== "plugin" ? item.has_update : null;
      pack.skills.push(row);
      if (!pack.scopes.some((s) => scopeKey(s) === scopeKey(row.scope)))
        pack.scopes.push(row.scope);
    }
    packs.set(source, pack);
  }
  const list = [...packs.values()].sort((a, b) => a.source.localeCompare(b.source));
  for (const pack of list) {
    const installed = new Set(pack.skills.map((s) => s.name));
    pack.edited_count = pack.skills.filter((s) => s.edited).length;
    pack.behind_count = pack.skills.filter((s) => s.behind).length;
    if (remote) {
      pack.remote =
        pack.kind === "plugin"
          ? { state: "no_git_source" }
          : { state: "checked", commit: "abc1234" };
      pack.new_upstream = (UPSTREAM_ONLY.get(pack.source) ?? []).filter((n) => !installed.has(n));
    }
    const parts = [plural(new Set(pack.skills.map((s) => s.name)).size, "skill")];
    if (pack.edited_count) parts.push(`${pack.edited_count} edited locally`);
    if (remote && pack.kind !== "plugin") parts.push(`${pack.behind_count} behind upstream`);
    if (pack.new_upstream.length) parts.push(`new upstream: ${pack.new_upstream.join(", ")}`);
    pack.summary = parts.join("; ");
  }
  return { packs: list, summary: `${plural(list.length, "pack")}.` };
}

function plural(count: number, one: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${one}s`;
}

function skipReason(pack: Pack, row: PackSkill, action: StepAction, snapshot: SkillSnapshot) {
  if (pack.kind === "plugin") {
    return action === "remove"
      ? "a plugin; remove it in the agent"
      : `a plugin; the agent updates it with ${pack.update_commands.join(" or ")}`;
  }
  if (action === "remove") {
    const item = snapshot.skills.find((s) => s.name === row.name);
    const hasUniversal = item?.deployments.some(
      (d) => d.agent === "shared" && (d.project_path ?? "global") === scopeKey(row.scope),
    );
    if (!hasUniversal) return "no copy in .agents/skills to remove";
  }
  return row.edited ? "has local edits, which the owner tool would overwrite or delete" : null;
}

function report(dryRun: boolean, action: StepAction, steps: PackStep[]): PackActionReport {
  const count = (state: PackStep["state"]["state"]) =>
    steps.filter((s) => s.state.state === state).length;
  const verb = { install: "install", update: "update", remove: "remove", switch_source: "switch" }[
    action
  ];
  const past = {
    install: "Installed",
    update: "Updated",
    remove: "Removed",
    switch_source: "Switched",
  }[action];
  const summary = dryRun
    ? `Would ${verb} ${plural(count("planned"), "skill")}; ${count("skipped")} skipped.`
    : `${past} ${plural(count("done"), "skill")}; ${count("skipped")} skipped; ${count("failed")} failed.`;
  return { dry_run: dryRun, steps, summary };
}

function packStep(
  pack: Pack,
  row: PackSkill,
  action: StepAction,
  skip: string | null,
  dryRun: boolean,
) {
  const state: PackStep["state"] = skip
    ? { state: "skipped", reason: skip }
    : dryRun
      ? { state: "planned" }
      : { state: "done", event_id: `evt:v1/mock/${action}/${row.name}` };
  return { source: pack.source, skill: row.name, scope: row.scope, action, state };
}

/** `pack_update` or `pack_remove` on `snapshot`: the report, and the next snapshot for a real run. */
export function mockPackChange(
  snapshot: SkillSnapshot,
  action: "update" | "remove",
  source: string | null,
  dryRun: boolean,
): MockPackResult {
  const packs = mockPackReport(snapshot, false).packs.filter((p) => !source || p.source === source);
  if (source && packs.length === 0) {
    throw new Error(`no installed pack has source ${source}; pack list shows the sources`);
  }
  const steps = packs.flatMap((pack) =>
    pack.skills.map((row) =>
      packStep(pack, row, action, skipReason(pack, row, action, snapshot), dryRun),
    ),
  );
  const done = steps.filter((s) => s.state.state === "done");
  const now = new Date().toISOString();
  const skills = snapshot.skills.flatMap((item): InstalledSkill[] => {
    const mine = done.filter((s) => s.skill === item.name);
    if (mine.length === 0) return [item];
    if (action === "update") return [{ ...item, has_update: false, updated_at: now }];
    const removed = new Set(mine.map((s) => scopeKey(s.scope)));
    const deployments = item.deployments.filter((d) => !removed.has(d.project_path ?? "global"));
    return deployments.length ? [{ ...item, deployments }] : [];
  });
  return { report: report(dryRun, action, steps), next: { ...snapshot, skills } };
}

/** The shared copy, plus the Claude Code link `npx skills add --agent
 * claude-code` makes. The other first-class agents read the shared folder. */
function packAddDeployments(name: string, harnesses: string[]) {
  const shared = `${HARNESS_HOME}/.agents/skills/${name}`;
  const rows = [
    deployment({
      agent: "shared",
      scope: "global",
      path: shared,
      owner_kind: "skills-sh",
      content_hash: `${name}-v1`,
    }),
  ];
  if (harnesses.includes("claude-code")) {
    rows.push(
      deployment({
        agent: "Claude Code",
        scope: "global",
        path: `${HARNESS_HOME}/.claude/skills/${name}`,
        is_symlink: true,
        symlink_target: shared,
        resolved_path: shared,
        owner_kind: "skills-sh",
        content_hash: `${name}-v1`,
      }),
    );
  }
  return rows;
}

/** `pack_add` on `snapshot`: installs `skills`, or every upstream skill not installed yet. */
export function mockPackAdd(
  snapshot: SkillSnapshot,
  source: string,
  names: string[],
  harnesses: string[],
  dryRun: boolean,
): MockPackResult {
  const installed = new Set(snapshot.skills.map((s) => s.name));
  const wanted = names.length
    ? names
    : (UPSTREAM_ONLY.get(source) ?? []).filter((n) => !installed.has(n));
  if (wanted.length === 0) throw new Error(`${source} has no skills that are not installed`);
  const steps: PackStep[] = wanted.map((name) => ({
    source,
    skill: name,
    scope: { scope: "global" },
    action: "install",
    state: dryRun
      ? { state: "planned" }
      : { state: "done", event_id: `evt:v1/mock/install/${name}` },
  }));
  const added = dryRun
    ? []
    : wanted.map((name) =>
        skill({
          name,
          source,
          source_url: `https://github.com/${source}`,
          source_kind: "skills-sh",
          description: `Installed from the ${source} pack.`,
          deployments: packAddDeployments(name, harnesses),
        }),
      );
  return {
    report: report(dryRun, "install", steps),
    next: { ...snapshot, skills: [...snapshot.skills, ...added] },
  };
}
