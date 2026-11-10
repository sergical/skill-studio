// ============================================================================
// Skill Studio - skill-api tests
// skill-api.ts is the one file that names a Tauri command; a wrapper's shape
// or command string only changes with a test here. These tests pin the
// wrapper-to-command map against `src-tauri/src/lib.rs`'s registration list
// read straight off disk, guard the leftover-name allowlist unit 4.4 (#175)
// left in place, and keep the dev harness mock answering every kept wrapper.
// ============================================================================

import { describe, expect, it } from "vitest";
import skillApiSource from "./skill-api.ts?raw";
import mockTauriSource from "../dev/harness/mock-tauri.ts?raw";
// `?raw` reads straight off disk, the same as the two source imports above - no `node:fs`, so
// this stays typed without a Node types dependency this frontend workspace doesn't otherwise need.
import libRsSource from "../../src-tauri/src/lib.rs?raw";

/**
 * One exported wrapper's shape:
 * - "command": routes through `callCommand`, an IPC round trip.
 * - "event": subscribes via `listen`, never calls `invoke`.
 * - "none": not IPC at all (`invokeErrorMessage` is a pure string helper).
 *
 */
type WrapperEntry =
  | { kind: "command"; command: string }
  | { kind: "event"; event: string }
  | { kind: "none" };

const EXPECTED_WRAPPER_COMMANDS = {
  invokeErrorMessage: { kind: "none" },
  previewSkillFrontmatterRepair: {
    kind: "command",
    command: "preview_skill_frontmatter_repair",
  },
  applySkillFrontmatterRepair: {
    kind: "command",
    command: "apply_skill_frontmatter_repair",
  },
  fixSkill: { kind: "command", command: "fix_skill" },
  openConflictPaths: { kind: "command", command: "open_conflict_paths" },
  searchSkills: { kind: "command", command: "search_skills" },
  getPopularSkills: { kind: "command", command: "get_popular_skills" },
  getSkillDetails: { kind: "command", command: "get_skill_details" },
  getInstallCounts: { kind: "command", command: "get_install_counts" },
  getInstalledSkills: {
    kind: "command",
    command: "get_installed_skills",
  },
  getTrackedProjects: {
    kind: "command",
    command: "get_tracked_projects",
  },
  registerSkillProjects: {
    kind: "command",
    command: "register_skill_projects",
  },
  unregisterSkillProject: {
    kind: "command",
    command: "unregister_skill_project",
  },
  removeSkillProject: {
    kind: "command",
    command: "remove_skill_project",
  },
  getDiscoverySources: {
    kind: "command",
    command: "get_discovery_sources",
  },
  setDiscoverySource: {
    kind: "command",
    command: "set_discovery_source",
  },
  detectHarnesses: { kind: "command", command: "detect_harnesses" },
  getHarnessesChoice: {
    kind: "command",
    command: "get_harnesses_choice",
  },
  saveHarnessesChoice: {
    kind: "command",
    command: "save_harnesses_choice",
  },
  listProjectFolders: {
    kind: "command",
    command: "list_project_folders",
  },
  // Deferred from the shell's own UI (no Project Folders "Import" affordance calls it yet), but
  // the command is fully wired - see unit 4.4's four-name allowlist test below.
  importTrackedProjects: {
    kind: "command",
    command: "import_tracked_projects",
  },
  parkRecoveryNotices: { kind: "command", command: "park_recovery_notices" },
  acknowledgeParkRecovery: { kind: "command", command: "acknowledge_park_recovery" },
  removeSkill: { kind: "command", command: "remove_skill" },
  updateSkill: { kind: "command", command: "update_skill" },
  cancelUpdateAll: { kind: "command", command: "cancel_update_all" },
  updateAllSkillsWithProgress: { kind: "event", event: "skills://update-all-progress" },
  readInstalledSkillMd: {
    kind: "command",
    command: "read_installed_skill_md",
  },
  writeInstalledSkillMdIfUnchanged: {
    kind: "command",
    command: "write_installed_skill_md_if_unchanged",
  },
  openSkillPath: { kind: "command", command: "open_skill_path" },
  getEditorChoices: { kind: "command", command: "get_editor_choices" },
  setPreferredEditor: {
    kind: "command",
    command: "set_preferred_editor",
  },
  dataFolderStatus: { kind: "command", command: "data_folder_status" },
  getTelemetryEnabled: {
    kind: "command",
    command: "get_telemetry_enabled",
  },
  setTelemetryEnabled: {
    kind: "command",
    command: "set_telemetry_enabled",
  },
  forkSkill: { kind: "command", command: "fork_skill" },
  pullForkUpstream: { kind: "command", command: "pull_fork_upstream" },
  unforkSkill: { kind: "command", command: "unfork_skill" },
  addSkill: { kind: "command", command: "add_skill" },
  startAddSkillOperation: {
    kind: "command",
    command: "start_add_skill_operation",
  },
  startAddSkillsOperation: {
    kind: "command",
    command: "start_add_skills_operation",
  },
  getAddSkillOperation: {
    kind: "command",
    command: "get_add_skill_operation",
  },
  cancelAddSkillOperation: {
    kind: "command",
    command: "cancel_add_skill_operation",
  },
  confirmAddSkillTrust: {
    kind: "command",
    command: "confirm_add_skill_trust",
  },
  onAddSkillOperation: { kind: "event", event: "skills://add-skill-operation" },
  listGithubSkills: { kind: "command", command: "list_github_skills" },
  getAddMethodDefaults: {
    kind: "command",
    command: "get_add_method_defaults",
  },
  parkSkill: { kind: "command", command: "park_skill" },
  unparkSkill: { kind: "command", command: "unpark_skill" },
  parkCheck: { kind: "command", command: "park_check" },
  discardSkillCopy: { kind: "command", command: "discard_skill_copy" },
  unlistParkedDotagents: {
    kind: "command",
    command: "unlist_parked_dotagents",
  },
  splitSkill: { kind: "command", command: "split_skill" },
  splitSkillTargets: {
    kind: "command",
    command: "split_skill_targets",
  },
  turnOffForAgent: {
    kind: "command",
    command: "turn_off_for_agent",
  },
  turnOffCheck: { kind: "command", command: "turn_off_check" },
  restoreMovedDeployment: {
    kind: "command",
    command: "restore_moved_deployment",
  },
  setSkillsInvocation: {
    kind: "command",
    command: "set_skills_invocation",
  },
  parkSkills: { kind: "command", command: "park_skills" },
  skillLocalEdits: {
    kind: "command",
    command: "skill_local_edits",
  },
  unparkSkills: { kind: "command", command: "unpark_skills" },
  setPluginEnabled: { kind: "command", command: "set_plugin_enabled" },
  uninstallPlugin: { kind: "command", command: "uninstall_plugin" },
  updatePlugin: { kind: "command", command: "update_plugin" },
  listSkillEvents: { kind: "command", command: "list_skill_events" },
  restoreSkillEvent: {
    kind: "command",
    command: "restore_skill_event",
  },
  materializeHarnessRoot: {
    kind: "command",
    command: "materialize_harness_root",
  },
  materializeHarnessRootThenDisable: {
    kind: "command",
    command: "materialize_harness_root_then_disable",
  },
  makeSkillIndependentCopy: {
    kind: "command",
    command: "make_skill_independent_copy",
  },
  repairSkillLink: { kind: "command", command: "repair_skill_link" },
  checkSkillUpdatesNow: { kind: "command", command: "check_skill_updates_now" },
  getSkillSnapshot: { kind: "command", command: "get_skill_snapshot" },
  requestSkillRescan: {
    kind: "command",
    command: "request_skill_rescan",
  },
  rescanSkillsNow: {
    kind: "command",
    command: "rescan_skills_now",
  },
  onSkillSnapshot: { kind: "event", event: "skills://snapshot" },
  appVersion: { kind: "command", command: "app_version" },
  checkForUpdate: { kind: "command", command: "check_for_update" },
  getUpdateStatus: { kind: "command", command: "get_update_status" },
  installUpdate: { kind: "command", command: "install_update" },
  onUpdateStatus: { kind: "event", event: "skills://update-status" },
} satisfies Record<string, WrapperEntry>;

/** Every `export (async )?function <name>` in `skill-api.ts`, source-order - the wrapper set the
 * pinning test below must exactly cover, neither more nor fewer than `EXPECTED_WRAPPER_COMMANDS`. */
function exportedWrapperNames(source: string): string[] {
  return [...source.matchAll(/^export (?:async )?function (\w+)/gm)].map((m) => m[1]);
}

/** One wrapper's body: from its `export function` line to the next top-level `export function`
 * (or EOF) - not a brace-matcher, but every wrapper here is a flat few lines, so the slice never
 * needs to look past the next sibling's start. */
function wrapperBody(source: string, name: string, allNames: string[]): string {
  const start = source.search(new RegExp(`^export (?:async )?function ${name}\\b`, "m"));
  if (start < 0) throw new Error(`wrapperBody: ${name} not found in source`);
  const rest = source.slice(start + 1);
  const nextStarts = allNames
    .filter((other) => other !== name)
    .map((other) => rest.search(new RegExp(`^export (?:async )?function ${other}\\b`, "m")))
    .filter((i) => i >= 0);
  const end = nextStarts.length > 0 ? Math.min(...nextStarts) : rest.length;
  return rest.slice(0, end);
}

/** Derives one wrapper's `WrapperEntry` from its body - the same shapes `EXPECTED_WRAPPER_COMMANDS`
 * hand-pins, read back out of the real source instead of trusted by hand. `fullSource` resolves a
 * `listen` call keyed by a module-level constant (`ADD_SKILL_OPERATION_EVENT`), declared outside
 * the wrapper's own body. */
function deriveWrapperEntry(body: string, fullSource: string): WrapperEntry {
  const commandMatch = body.match(/callCommand(?:<[^>]*>)?\(\s*"([a-z_]+)"/);
  if (commandMatch) return { kind: "command", command: commandMatch[1] };
  const literalEvent = body.match(/listen<[^>]*>\(\s*"([\w:./-]+)"/);
  if (literalEvent) return { kind: "event", event: literalEvent[1] };
  const constEvent = body.match(/listen<[^>]*>\(\s*([A-Z_][A-Z0-9_]*)/);
  if (constEvent) {
    const decl = fullSource.match(new RegExp(`${constEvent[1]}\\s*=\\s*"([\\w:./-]+)"`));
    if (decl) return { kind: "event", event: decl[1] };
  }
  return { kind: "none" };
}

/** `skills::mod::command_name,` -> `command_name` for every line inside `generate_handler![...]`
 * in `src-tauri/src/lib.rs`, read straight off disk - the ops surface the frontend may call. One
 * Rust path per line; takes the segment after the last `::`. */
function registeredCommandNames(libRs: string): Set<string> {
  const start = libRs.indexOf("generate_handler![");
  if (start < 0) {
    throw new Error("registeredCommandNames: could not find generate_handler![...] in lib.rs");
  }
  // A `]\s*\)` close, not a literal indentation string - rustfmt reformatting the
  // block's indentation must not break this parse.
  const closeMatch = /\]\s*\)/.exec(libRs.slice(start));
  if (!closeMatch) {
    throw new Error("registeredCommandNames: could not find generate_handler![...] in lib.rs");
  }
  const end = start + closeMatch.index;
  const body = libRs.slice(start, end);
  const names = new Set<string>();
  for (const line of body.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.length === 0 || trimmed.startsWith("//")) continue;
    const m = trimmed.match(/^([\w:]+),$/);
    if (!m) continue;
    const segments = m[1].split("::");
    names.add(segments[segments.length - 1]);
  }
  return names;
}

describe("skill-api wrapper set", () => {
  it("skill_api_wrapper_set_matches_ops_commands_or_names_the_drift", () => {
    const wrapperNames = exportedWrapperNames(skillApiSource);
    const expectedNames = Object.keys(EXPECTED_WRAPPER_COMMANDS);

    // Every exported wrapper is pinned, and the pin list names nothing extra.
    expect(wrapperNames.sort()).toEqual([...expectedNames].sort());

    const registered = registeredCommandNames(libRsSource);

    for (const [name, expected] of Object.entries(EXPECTED_WRAPPER_COMMANDS)) {
      const derived = deriveWrapperEntry(
        wrapperBody(skillApiSource, name, wrapperNames),
        skillApiSource,
      );
      expect(derived, `${name}: derived ${JSON.stringify(derived)}`).toEqual(expected);
      if (expected.kind === "command") {
        const isRegistered = registered.has(expected.command);
        expect(
          isRegistered,
          `${name} -> "${expected.command}" is missing from generate_handler!`,
        ).toBe(true);
      }
    }
  });

  it("set_deployment_enabled_and_trial_wrappers_are_removed_or_names_the_leftover", () => {
    // setDeploymentEnabled: unit 4.4 (#175) removed it end to end - park is the generic off
    // switch now, see docs/action-map/frontend-keep.md.
    expect(skillApiSource).not.toMatch(/\bsetDeploymentEnabled\b/);

    // keepSkillTrial, restoreTrashedSkill, onTrialExpired: #278 removed the trial feature end
    // to end, folded into park's existing quarantine cap - see docs/action-map/frontend-keep.md.
    for (const removed of ["keepSkillTrial", "restoreTrashedSkill", "onTrialExpired"]) {
      expect(skillApiSource).not.toMatch(new RegExp(`\\b${removed}\\b`));
    }

    // importTrackedProjects is the one remaining allowlist entry, not an oversight: registered
    // and reachable from `getTrackedProjects`' consumers' own import flow, just not the
    // setDeploymentEnabled-adjacent code unit 4.4 touched.
    expect(skillApiSource, "importTrackedProjects should still be exported").toMatch(
      /export (async )?function importTrackedProjects\b/,
    );
  });

  it("dev_harness_mock_answers_every_command_the_kept_wrappers_call_or_names_the_gap", () => {
    const mockedCases = new Set(
      [...mockTauriSource.matchAll(/case "([a-z_]+)":/g)].map((m) => m[1]),
    );
    const missing = Object.values(EXPECTED_WRAPPER_COMMANDS).flatMap((entry) =>
      entry.kind === "command" && !mockedCases.has(entry.command) ? [entry.command] : [],
    );
    expect(missing, `dev harness mock-tauri.ts has no case for: ${missing.join(", ")}`).toEqual([]);
  });
});
