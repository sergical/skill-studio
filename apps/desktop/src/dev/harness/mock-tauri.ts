// ============================================================================
// Skill Studio - mock-tauri
// Installs a `mockIPC` handler that answers every `invoke` the desktop
// frontend makes from an in-memory `SkillSnapshot`, so the app runs in a
// plain browser tab with no Tauri runtime behind it. Generalized from the
// single-skill mock `packages/marketing/capture/main.tsx` used to hand-roll:
// every handler here acts on whichever skill/deployment the payload names.
// ============================================================================

import type { InvokeArgs } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { z } from "zod";
import {
  agentIdFromDeploymentLabel,
  deploymentLabelFromAgentId,
  isProjectPattern,
} from "@skill-studio/lib";
import type {
  AddSkillOperationEvent,
  BackgroundStatus,
  Deployment,
  DiscoverySourceSetting,
  InstalledSkill,
  ProjectFolder,
  RemoveOutcome,
  SkillEvent,
  SkillSnapshot,
  TrackedProjects,
} from "@skill-studio/lib";
import type { EditorChoices, EditorOption } from "../../lib/skill-api";
import {
  deployment,
  fallbackSkillContent,
  HARNESS_HOME,
  HARNESS_PROJECT,
  harnessSkillContent,
  skill,
} from "./skill-fixture";
import { mockListingReport } from "./mock-listing";
import { mockPackAdd, mockPackChange, mockPackReport } from "./mock-packs";

/** Mirrors the core's `split_target_root` with default `CODEX_HOME` and OpenCode config root. */
function splitCopyPath(harness: string, skillName: string, projectPath: string | null): string {
  const global = projectPath === null;
  const root =
    harness === "opencode"
      ? global
        ? ".config/opencode/skills"
        : ".opencode/skills"
      : harness === "pi"
        ? global
          ? ".pi/agent/skills"
          : ".pi/skills"
        : `.${harness.replace(/-code$|-build$/, "")}/skills`;
  return `${projectPath ?? HARNESS_HOME}/${root}/${skillName}`;
}

/** One native confirm (`ask`) the app showed, and the answer the harness gave it. */
interface AskedDialog {
  title: string | null;
  message: string;
  answer: boolean;
}

/** What the harness exposes on `window.__harness` for an agent driving the app. */
interface HarnessControl {
  snapshot(): SkillSnapshot;
  /** Bumps `revision` and emits `skills://snapshot`, exactly like a real background rebuild. */
  publish(next: SkillSnapshot): Promise<void>;
  /** Publishes `snapshot` with one skill replaced by `transform`'s result. */
  updateSkill(name: string, transform: (skill: InstalledSkill) => InstalledSkill): Promise<void>;
  /** Replaces the background status and emits `skills://background-status`, like a worker starting or finishing. */
  publishBackgroundStatus(next: BackgroundStatus): Promise<void>;
  /** Every native confirm the app showed, oldest first. */
  asked(): AskedDialog[];
  /** Answers the next native confirm with `answer`; every other one gets OK. */
  answerNextAsk(answer: boolean): void;
  /** Replaces a SKILL.md on "disk", like an edit made outside the app. */
  writeSkillMd(path: string, content: string): void;
}

declare global {
  interface Window {
    __harness?: HarnessControl;
  }
}

/** The `AddSkillRequest`/`AddSkillsRequest` fields the harness's `add_skill` family of handlers
 * actually reads - narrower than the real DTOs so `z.parse` doesn't have to reproduce every
 * field the Add Skill sheet sends. */
const addRequestSchema = z
  .object({
    scope: z.enum(["global", "project"]),
    project_path: z.string().nullable().optional(),
    method: z.enum(["dotagents", "skills-sh", "copy"]),
    source: z
      .object({
        kind: z.enum(["github", "git", "local"]),
        skillName: z.string().nullable(),
        repo: z.string().nullable(),
        url: z.string().nullable(),
      })
      .passthrough(),
    skills: z.array(z.object({ name: z.string() }).passthrough()).optional(),
  })
  .passthrough();
type AddRequestLike = z.infer<typeof addRequestSchema>;

const SOURCE_KIND_BY_METHOD = {
  dotagents: "dotagents",
  "skills-sh": "skills-sh",
  copy: "manual",
} as const satisfies Record<AddRequestLike["method"], InstalledSkill["source_kind"]>;

const OWNER_KIND_BY_METHOD = {
  dotagents: "dotagents",
  "skills-sh": "skills-sh",
  copy: "copy",
} as const satisfies Record<AddRequestLike["method"], Deployment["owner_kind"]>;

type ObjectInvokeArgs = Exclude<InvokeArgs, number[] | ArrayBuffer | Uint8Array>;

function isObjectInvokeArgs(payload: InvokeArgs | undefined): payload is ObjectInvokeArgs {
  return (
    payload !== undefined &&
    !Array.isArray(payload) &&
    !(payload instanceof ArrayBuffer) &&
    !(payload instanceof Uint8Array)
  );
}

/** Seeded into `trackedProjects.added` below: a folder added by hand that no longer exists, so the
 * Settings "Project folders" card always has a "Folder not found" row to show. */
const ARCHIVED_CLIENT_PROJECT = `${HARNESS_HOME}/src/archived-client`;

/** The six discovery harnesses, in the display order `discovery_harnesses()` yields on the Rust
 * side - see `crates/skill-studio-host/src/discovery.rs`'s `HISTORY_SOURCES`. */
const DISCOVERY_HARNESSES = ["claude-code", "codex", "opencode", "pi", "cursor", "grok-build"];

/** Installs the mock Tauri IPC layer and returns the control the harness (or the marketing
 * capture page) drives it with. */
export function installMockTauri(initial: SkillSnapshot): HarnessControl {
  let currentSnapshot = initial;
  // Mirrors the event store: newest first, as `list_skill_events` returns it. A remove keeps the
  // skill it took out, so restoring that event puts the same skill back.
  const events: SkillEvent[] = [];
  const removedByEvent = new Map<string, InstalledSkill>();
  // What each parked folder was before it moved, keyed by the parked deployment's id, so unpark
  // puts back the same folder and the links that pointed at it.
  const parkedFrom = new Map<string, Deployment[]>();
  const askedDialogs: AskedDialog[] = [];
  const askAnswers: boolean[] = [];
  let telemetryEnabled = false;
  const addOperations = new Map<string, AddSkillOperationEvent>();
  const pausedForTrust = new Map<string, { request: AddRequestLike; batch: boolean }>();
  // Mirrors `trusted_dotagents_sources`: every repository a fixture skill came from was trusted
  // when it was added.
  const trustedSources = new Set<string>();
  for (const item of initial.skills) trustedSources.add(item.source.toLowerCase());
  // Mirrors the backend's `~/.agents/skill-studio.json` `projects` key - the register/unregister/
  // import handlers below mutate it with the same track/untrack rules as `TrackedProjects` in
  // `crates/skill-studio-core`, and `get_tracked_projects` reads it back.
  let trackedProjects: TrackedProjects = { added: [ARCHIVED_CLIENT_PROJECT], excluded: [] };
  const discoverySources = new Map<string, boolean>(
    DISCOVERY_HARNESSES.map((harness) => [harness, true]),
  );
  // Mirrors `preferred_editor` in `~/.agents/skill-studio.json` - an app name, a `.app` path,
  // or "$EDITOR", or `null` for the system default. See `skill_editor::EditorChoices`.
  let editorPreference: string | null = null;
  const mcpConnections = new Set<string>();
  const KNOWN_EDITOR_APPS: EditorOption[] = [
    { app_name: "Cursor", label: "Cursor" },
    { app_name: "Visual Studio Code", label: "Visual Studio Code" },
    { app_name: "IntelliJ IDEA Ultimate Edition", label: "IntelliJ IDEA Ultimate Edition" },
  ];

  function editorChoices(): EditorChoices {
    const apps = [...KNOWN_EDITOR_APPS];
    if (
      editorPreference?.endsWith(".app") &&
      !apps.some((app) => app.app_name === editorPreference)
    ) {
      const fileName = editorPreference.split("/").pop() ?? editorPreference;
      apps.push({ app_name: editorPreference, label: fileName.replace(/\.app$/, "") });
    }
    return {
      automatic_label: "Automatic (Cursor)",
      apps,
      terminal: { app_name: "$EDITOR", label: "nvim" },
      selected: editorPreference,
    };
  }

  function snapshotTrackedProjects(): TrackedProjects {
    return { added: [...trackedProjects.added], excluded: [...trackedProjects.excluded] };
  }

  /** Appends each path to `added` if not already there; removes it from `excluded`. */
  function trackProjects(paths: string[]): void {
    const excludedSet = new Set(trackedProjects.excluded);
    paths.forEach((path) => excludedSet.delete(path));
    trackedProjects = {
      added: [...new Set([...trackedProjects.added, ...paths])],
      excluded: [...excludedSet],
    };
  }

  /** Removes the path from `added`; appends it to `excluded` if not already there. */
  function untrackProject(path: string): void {
    trackedProjects = {
      added: trackedProjects.added.filter((p) => p !== path),
      excluded: [...new Set([...trackedProjects.excluded, path])],
    };
  }

  /** Removes the path from `added` only, recording no exclusion - mirrors `TrackedProjects::forget`. */
  function forgetProject(path: string): void {
    trackedProjects = {
      ...trackedProjects,
      added: trackedProjects.added.filter((p) => p !== path),
    };
  }

  function discoverySourceSettings(): DiscoverySourceSetting[] {
    return DISCOVERY_HARNESSES.map((harness) => ({
      harness,
      enabled: discoverySources.get(harness) ?? false,
    }));
  }

  /** Expands a leading `~` against the harness's home, mirroring `tracked_projects`'s own
   * expansion, so a pattern's parent can be compared against snapshot project paths. */
  function expandMockHome(path: string): string {
    if (path === "~") return HARNESS_HOME;
    if (path.startsWith("~/")) return `${HARNESS_HOME}${path.slice(1)}`;
    return path;
  }

  /** The folder a `*` pattern's children live in. */
  function patternParent(pattern: string): string {
    const withoutStar = pattern.slice(0, -1).replace(/\/$/, "") || "/";
    return expandMockHome(withoutStar);
  }

  /** Mirrors `skill_project_folders::project_folders`: the fixture snapshot's projects, labelled
   * `discovered` unless the user added them by hand, one row per `*` pattern in `added` instead of
   * one row per folder it matches (a matched child is hidden unless it's also a plain added
   * entry), followed by any plain added folder missing from the snapshot's project list (not
   * found on disk). */
  function projectFolders(): ProjectFolder[] {
    const addedSet = new Set(trackedProjects.added);
    const patterns = trackedProjects.added.filter(isProjectPattern);
    const plainAddedSet = new Set(trackedProjects.added.filter((path) => !isProjectPattern(path)));
    const patternEntries = patterns.map((pattern) => ({ pattern, parent: patternParent(pattern) }));
    const patternParents = new Set(patternEntries.map((entry) => entry.parent));

    const rows: ProjectFolder[] = [];
    for (const path of currentSnapshot.projects) {
      const parent = path.slice(0, path.lastIndexOf("/"));
      if (patternParents.has(parent) && !plainAddedSet.has(path)) continue;
      rows.push({
        path,
        source: addedSet.has(path) ? "added" : "discovered",
        missing: false,
        matches: null,
      });
    }
    for (const { pattern, parent } of patternEntries) {
      const matches = currentSnapshot.projects.filter(
        (path) => path.slice(0, path.lastIndexOf("/")) === parent,
      ).length;
      rows.push({ path: pattern, source: "added", missing: false, matches });
    }

    const listedPaths = new Set(rows.map((row) => row.path));
    for (const path of plainAddedSet) {
      if (!listedPaths.has(path))
        rows.push({ path, source: "added", missing: true, matches: null });
    }
    return rows;
  }

  let currentBackgroundStatus: BackgroundStatus = { revision: 0, tasks: [] };

  async function publishBackgroundStatus(next: BackgroundStatus): Promise<void> {
    currentBackgroundStatus = next;
    await emit("skills://background-status", currentBackgroundStatus);
  }

  async function publish(next: SkillSnapshot): Promise<void> {
    currentSnapshot = {
      ...next,
      revision: currentSnapshot.revision + 1,
      scanned_at: new Date().toISOString(),
    };
    await emit("skills://snapshot", currentSnapshot);
  }

  function recordEvent(kind: string, skillName: string, restorable: boolean): SkillEvent {
    const event: SkillEvent = {
      id: `evt:v1/harness/${events.length + 1}`,
      ts: new Date().toISOString(),
      kind,
      skill: skillName,
      harness: null,
      scope: "global",
      project_path: null,
      status: "done",
      restorable,
      force_restorable: false,
      reverted_by: null,
    };
    events.unshift(event);
    return event;
  }

  /** Mirrors the core's park: each target folder moves under `~/.agents/skills-parked/` (a project
   * copy under `projects/<project-key>/`, so global and project copies never share a slot), and the
   * links that pointed at it go with it. The skill is parked once no live copy is left. */
  function parkCopies(item: InstalledSkill, targetIds: Set<string>): InstalledSkill {
    const parkedAt = new Date().toISOString();
    const moved = item.deployments.filter((entry) => targetIds.has(entry.id));
    const kept = item.deployments.filter(
      (entry) =>
        !targetIds.has(entry.id) &&
        !(entry.backing.kind === "linked-to" && targetIds.has(entry.backing.deployment_id)),
    );
    const parked = moved.map((entry) => {
      const agent = agentIdFromDeploymentLabel(entry.agent);
      const kind = agent === "shared" ? "universal" : (agent ?? entry.agent);
      const slot = entry.project_path
        ? `projects/${entry.project_path.replace(/[^A-Za-z0-9_-]/g, "-")}/${kind}`
        : kind;
      const copy = deployment({
        agent: "parked",
        scope: "parked",
        path: `${HARNESS_HOME}/.agents/skills-parked/${slot}/${item.name}`,
        parked_origin: { kind, scope: entry.scope, project_path: entry.project_path ?? null },
      });
      parkedFrom.set(
        copy.id,
        item.deployments.filter(
          (each) =>
            each.id === entry.id ||
            (each.backing.kind === "linked-to" && each.backing.deployment_id === entry.id),
        ),
      );
      return copy;
    });
    const deployments = [...kept, ...parked];
    const allParked = deployments.every((entry) => entry.scope === "parked");
    return { ...item, deployments, parked: allParked, parked_at: allParked ? parkedAt : null };
  }

  /** Reverses `parkCopies`. A target may name the parked folder or the folder it was before. A
   * fixture skill parked before the harness started returns to the Universal folder it came from. */
  function unparkCopies(item: InstalledSkill, targetIds: Set<string>): InstalledSkill {
    const deployments = item.deployments.flatMap((entry) => {
      if (entry.scope !== "parked") return [entry];
      const before = parkedFrom.get(entry.id);
      const named = targetIds.has(entry.id) || before?.some((each) => targetIds.has(each.id));
      if (!named) return [entry];
      parkedFrom.delete(entry.id);
      return (
        before ?? [
          deployment({
            agent: "shared",
            scope: "global",
            path: `${HARNESS_HOME}/.agents/skills/${item.name}`,
          }),
        ]
      );
    });
    return { ...item, deployments, parked: false, parked_at: null };
  }

  /** The SKILL.md text the app reads at `path`: the harness's copy, else the fixture fallback. */
  function skillMdOnDisk(path: string): string {
    const name = path.split("/").filter(Boolean).pop() ?? "skill";
    return harnessSkillContent.get(path) ?? fallbackSkillContent(name);
  }

  function updateSkill(
    name: string,
    transform: (item: InstalledSkill) => InstalledSkill,
  ): Promise<void> {
    return publish({
      ...currentSnapshot,
      skills: currentSnapshot.skills.map((item) => (item.name === name ? transform(item) : item)),
    });
  }

  /** Resolves a `LifecycleTarget`-shaped payload (`deployment_id` or `owner_id`) to the skill it
   * names - every deployment/owner id this fixture mints ends in `/<name>`. */
  function skillNameForTarget(target: {
    deployment_id?: string | null;
    owner_id?: string | null;
  }): string {
    if (target.deployment_id) {
      const found = currentSnapshot.skills.find((item) =>
        item.deployments.some((d) => d.id === target.deployment_id),
      );
      if (found) return found.name;
    }
    const key = target.deployment_id ?? target.owner_id ?? "";
    const name = key.split("/").pop();
    if (!name) throw new Error(`harness: target names no skill: ${JSON.stringify(target)}`);
    return name;
  }

  /** Mirrors `normalize_dotagents_source_identity`: an add from a GitHub or git repository the
   * user has not trusted pauses until they confirm it, for every method. Local folders are never
   * gated. */
  function untrustedIdentity(request: AddRequestLike): string | null {
    const { kind, repo, url } = request.source;
    const raw = kind === "github" ? repo : kind === "git" ? url?.trim().replace(/\/+$/, "") : null;
    const identity = raw
      ?.trim()
      .replace(/\.git$/, "")
      .toLowerCase();
    return identity && !trustedSources.has(identity) ? identity : null;
  }

  async function emitOperation(event: AddSkillOperationEvent): Promise<AddSkillOperationEvent> {
    addOperations.set(event.operation_id, event);
    await emit("skills://add-skill-operation", event);
    return event;
  }

  /** Adds every skill the request names: a batch, each listed skill; a single add, the one its
   * source names. */
  async function addSkills(request: AddRequestLike) {
    const names = request.skills?.map((entry) => entry.name) ?? [
      request.source.skillName ?? request.source.repo?.split("/").pop() ?? "added-skill",
    ];
    const added = names.map((skillName) =>
      buildAddedSkill(request, skillName, "A skill added through the harness fixture."),
    );
    await publish({ ...currentSnapshot, skills: [...currentSnapshot.skills, ...added] });
    return added.map((each) => ({
      name: each.name,
      tool: "dotagents",
      command: `dotagents add ${request.source.repo ?? each.name}`,
      deployments_created: each.deployments.map((entry) => entry.path),
      warning: null,
    }));
  }

  async function completeAdd(
    operationId: string,
    request: AddRequestLike,
    batch: boolean,
    retryOf?: string,
  ): Promise<AddSkillOperationEvent> {
    const results = await addSkills(request);
    const completed: AddSkillOperationEvent = {
      operation_id: operationId,
      sequence: 1,
      phase: "completed",
      message: `Added ${results[0].name}`,
      ...(batch
        ? { outcomes: results.map((each) => ({ name: each.name, result: each, error: null })) }
        : { result: results[0] }),
    };
    if (retryOf) completed.retry_of = retryOf;
    return emitOperation(completed);
  }

  /** The folders a park target names: its deployment, or every folder of the skill for an owner
   * target, which names the skill and not one folder. */
  function targetIdsOf(item: InstalledSkill, deploymentId: string | null | undefined): Set<string> {
    if (deploymentId) return new Set([deploymentId]);
    const ids = new Set<string>();
    for (const entry of item.deployments) {
      if (entry.backing.kind !== "linked-to") ids.add(entry.id);
    }
    return ids;
  }

  function buildAddedSkill(
    request: AddRequestLike,
    name: string,
    description: string,
  ): InstalledSkill {
    const isProject = request.scope === "project";
    const project = isProject ? (request.project_path ?? HARNESS_PROJECT) : null;
    const base = project
      ? `${project}/.agents/skills/${name}`
      : `${HARNESS_HOME}/.agents/skills/${name}`;
    const deployments: Deployment[] = [
      deployment({
        agent: "shared",
        scope: isProject ? "project" : "global",
        project_path: project ?? undefined,
        path: base,
        owner_kind: OWNER_KIND_BY_METHOD[request.method],
        content_hash: `${name}-v1`,
      }),
      deployment({
        agent: "Claude Code",
        scope: isProject ? "project" : "global",
        project_path: project ?? undefined,
        path: project
          ? `${project}/.claude/skills/${name}`
          : `${HARNESS_HOME}/.claude/skills/${name}`,
        is_symlink: true,
        symlink_target: base,
        resolved_path: base,
        owner_kind: OWNER_KIND_BY_METHOD[request.method],
        content_hash: `${name}-v1`,
      }),
    ];
    harnessSkillContent.set(
      `${base}/SKILL.md`,
      `---\nname: ${name}\ndescription: ${description}\n---\n\n# ${name}\n\n${description}\n`,
    );
    return skill({
      name,
      source: request.source.repo ?? request.source.url ?? "manual",
      source_kind: SOURCE_KIND_BY_METHOD[request.method],
      description,
      installed_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      modified_at: new Date().toISOString(),
      deployments,
    });
  }

  mockWindows("main");
  // `?latency=<ms>` holds every answer that long, so busy states show as they do with real disk work.
  const latency = Number(new URLSearchParams(location.search).get("latency") ?? 0);
  mockIPC(
    async (command, rawPayload) => {
      if (latency > 0) await new Promise((resolve) => setTimeout(resolve, latency));
      const payload = isObjectInvokeArgs(rawPayload) ? rawPayload : {};
      switch (command) {
        case "check_skill_updates_now":
          currentSnapshot = {
            ...currentSnapshot,
            update_check: { ...currentSnapshot.update_check, checked_at: new Date().toISOString() },
          };
          return currentSnapshot.update_check;
        case "get_background_status":
          return currentBackgroundStatus;
        // `?fullscreen=1` shows the layout of a window in macOS full screen.
        case "plugin:window|is_fullscreen":
          return new URLSearchParams(location.search).has("fullscreen");
        case "get_skill_snapshot":
          return currentSnapshot;
        case "get_installed_skills":
          return currentSnapshot.skills;
        case "request_skill_rescan":
          await publish(currentSnapshot);
          return undefined;
        case "rescan_skills_now":
          await publish(currentSnapshot);
          return currentSnapshot;
        case "get_tracked_projects":
          return snapshotTrackedProjects();
        case "register_skill_projects": {
          const typed = z.array(z.string()).parse(payload.paths);
          for (const path of typed) {
            const segments = path.split("/");
            const strayStar = segments.some(
              (segment, i) =>
                segment.includes("*") && !(i === segments.length - 1 && segment === "*"),
            );
            if (strayStar) {
              throw new Error("Only the last part of a path can be *, as in ~/src/*.");
            }
          }
          // A pattern is saved exactly as typed; a plain path is expanded, matching the backend's
          // `entry_to_save`.
          const paths = typed.map((path) => (isProjectPattern(path) ? path : expandMockHome(path)));
          trackProjects(paths);
          return snapshotTrackedProjects();
        }
        case "unregister_skill_project": {
          const path = z.string().parse(payload.path);
          untrackProject(path);
          return snapshotTrackedProjects();
        }
        case "import_tracked_projects": {
          const added = z.array(z.string()).parse(payload.added);
          const excluded = z.array(z.string()).parse(payload.excluded);
          trackProjects(added);
          excluded.forEach((path) => untrackProject(path));
          return snapshotTrackedProjects();
        }
        case "remove_skill_project": {
          const path = z.string().parse(payload.path);
          forgetProject(path);
          return snapshotTrackedProjects();
        }
        case "get_discovery_sources":
          return discoverySourceSettings();
        case "set_discovery_source": {
          const harness = z.string().parse(payload.harness);
          const enabled = z.boolean().parse(payload.enabled);
          if (!discoverySources.has(harness)) {
            throw new Error(`Unknown discovery source: ${harness}`);
          }
          discoverySources.set(harness, enabled);
          return discoverySourceSettings();
        }
        case "list_project_folders":
          return projectFolders();
        case "detect_harnesses":
          return {
            harnesses: DISCOVERY_HARNESSES.map((id) => ({
              id,
              display_name: deploymentLabelFromAgentId(id),
              state: discoverySources.get(id) ? "configured" : "not_found",
              executable: null,
              executable_on_path: null,
              version: { value: null, evidence: { source: "mock", confidence: "unknown" } },
              install_method: { value: null, evidence: { source: "mock", confidence: "unknown" } },
              configured: discoverySources.get(id) ?? false,
              used: false,
            })),
          };
        case "get_harnesses_choice":
          return null;
        case "set_harness_enabled":
          return { kept: [], search_project_folders: true, saved_at: new Date().toISOString() };
        case "save_harnesses_choice":
          return undefined;
        case "get_mcp_connections":
          return [...mcpConnections];
        case "connect_mcp":
          mcpConnections.add(z.string().parse(payload.harness));
          return undefined;
        case "open_skill_path":
        case "unfork_skill":
          return undefined;
        case "app_version":
          return {
            version: "0.1.0",
            commit: "dev",
            notes: "### Added\n\n- The dev harness mock for `app_version`.",
          };
        case "check_for_update":
        case "get_update_status":
          return { status: "up-to-date" };
        case "install_update":
          return undefined;
        case "get_editor_choices":
          return editorChoices();
        case "set_preferred_editor":
          editorPreference = payload.appName == null ? null : String(payload.appName);
          return undefined;
        case "get_telemetry_enabled":
          return telemetryEnabled;
        case "set_telemetry_enabled":
          telemetryEnabled = z.boolean().parse(payload.enabled);
          return telemetryEnabled;
        case "get_webview_telemetry":
          return false;
        case "send_webview_envelope":
          return 0;

        case "read_installed_skill_md":
          return skillMdOnDisk(String(payload.path));
        case "write_installed_skill_md_if_unchanged": {
          const { path, expectedContent, content } = z
            .object({ path: z.string(), expectedContent: z.string(), content: z.string() })
            .parse(payload);
          // The same compare-and-swap refusal `skill_md_write.rs` returns.
          if (skillMdOnDisk(path) !== expectedContent) {
            throw new Error(
              "SKILL.md changed on disk since it was loaded. Reload the file and run the audit again.",
            );
          }
          harnessSkillContent.set(path, content);
          return undefined;
        }

        case "preview_skill_frontmatter_repair":
          return {
            deployment_id: "mock-deployment",
            path: "/mock/SKILL.md",
            scope: "global",
            reason: "Frontmatter is missing the required `name` field.",
            expected_content_fingerprint: "mock-fingerprint",
            proposal_id: "mock-proposal",
            original_content: "---\ndescription: mock\n---\n",
            proposed_content: "---\nname: mock-skill\ndescription: mock\n---\n",
            allowed_apply_modes: ["apply-fix"],
          };
        case "apply_skill_frontmatter_repair":
          return undefined;
        case "fix_skill":
          return { skill: String(payload.skill ?? ""), applied: [], unrepaired: [], conflicts: [] };
        case "open_conflict_paths":
          return undefined;

        case "list_skill_events": {
          const { limit, skill: only } = z
            .object({ limit: z.number().nullish(), skill: z.string().nullish() })
            .parse(payload);
          return events.filter((event) => !only || event.skill === only).slice(0, limit ?? 200);
        }
        case "get_add_method_defaults":
          return {
            dotagents_installed: true,
            has_skill_lock: true,
            installed_harnesses: ["claude-code", "codex", "opencode", "pi", "cursor"],
            claude_reads_shared_folder: false,
          };
        case "list_github_skills":
          // `getsentry/skills` holds `sentry-triage`, already installed in the
          // fixture, beside two that are not.
          if (payload.repo === "getsentry/skills") {
            return {
              repo: "getsentry/skills",
              git_ref: "main",
              commit: "fixture",
              skills: ["sentry-triage", "sentry-setup", "sentry-review"].map((name) => ({
                name,
                path: `skills/${name}`,
              })),
              truncated: false,
            };
          }
          return {
            repo: "anthropics/skills",
            git_ref: "main",
            commit: "fixture",
            skills: [{ name: "frontend-design", path: "frontend-design" }],
            truncated: false,
          };
        case "update_plugin":
          return { outcome: "updated", message: null };
        case "set_plugin_enabled":
        case "uninstall_plugin":
          return undefined;
        case "search_skills":
        case "get_popular_skills":
          return { has_more: false, skills: [] };
        case "get_skill_details":
          return {
            id: String(payload.skillId ?? ""),
            slug: "",
            source: "",
            hash: "",
            installs: 0,
            skill_md: null,
          };
        case "get_install_counts":
          return [];

        case "skill_listing": {
          const request = z
            .object({
              claude_budget_fraction: z.number().nullish(),
              codex_max_context_tokens: z.number().nullish(),
            })
            .parse(payload.request);
          return mockListingReport(currentSnapshot, request);
        }
        case "pack_list": {
          const { remote } = z.object({ remote: z.boolean() }).parse(payload.request);
          return mockPackReport(currentSnapshot, remote);
        }
        case "pack_add": {
          const request = z
            .object({
              source: z.string(),
              skills: z.array(z.string()),
              harnesses: z.array(z.string()),
              dry_run: z.boolean(),
            })
            .parse(payload.request);
          const { report, next } = mockPackAdd(
            currentSnapshot,
            request.source,
            request.skills,
            request.harnesses,
            request.dry_run,
          );
          if (!request.dry_run) await publish(next);
          return report;
        }
        case "pack_export": {
          // Mirrors the core's report shape and texts (pack_export.rs); it
          // writes nothing, as the harness has no disk.
          const request = z
            .object({
              dir: z.string(),
              skills: z.array(z.string()),
              source: z.string().nullish(),
              project_path: z.string().nullish(),
              dry_run: z.boolean(),
            })
            .parse(payload.request);
          // The core takes each copy from the request's project, else the home.
          const project = request.project_path ?? null;
          const missing = request.skills.find((name) =>
            currentSnapshot.skills.some(
              (skill) =>
                skill.name === name &&
                !skill.deployments.some((d) =>
                  project === null ? d.scope === "global" : d.project_path === project,
                ),
            ),
          );
          if (missing) throw new Error(`${missing} has no copy in this scope`);
          const unknown = request.skills.find(
            (name) => !currentSnapshot.skills.some((skill) => skill.name === name),
          );
          if (unknown) throw new Error(`no installed skill is named ${unknown}`);
          const source = request.source ?? "'<owner>/<repo>'";
          const verb = request.dry_run ? "Would write" : "Wrote";
          const count = request.skills.length;
          return {
            dry_run: request.dry_run,
            dir: request.dir,
            skills: request.skills.map((name) => ({
              skill: name,
              from: `${HARNESS_HOME}/.agents/skills/${name}`,
              files: 1,
            })),
            files: [...request.skills.map((name) => `skills/${name}/SKILL.md`), "README.md"],
            install_command: `npx skills add ${source}${request.skills.map((name) => ` --skill ${name}`).join("")}`,
            summary: `${verb} ${count} skill${count === 1 ? "" : "s"} and a README to ${request.dir}.`,
          };
        }
        case "pack_update":
        case "pack_remove": {
          const request = z
            .object({ source: z.string().nullish(), dry_run: z.boolean() })
            .parse(payload.request);
          const { report, next } = mockPackChange(
            currentSnapshot,
            command === "pack_update" ? "update" : "remove",
            request.source ?? null,
            request.dry_run,
          );
          if (!request.dry_run) await publish(next);
          return report;
        }
        case "park_skill": {
          const { deployment_id, owner_id } = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget({ deployment_id, owner_id });
          await updateSkill(name, (item) => parkCopies(item, targetIdsOf(item, deployment_id)));
          // A park has no inverse in the event store; unpark reverses it.
          recordEvent("park", name, false);
          return undefined;
        }
        case "park_check":
          return { git_tracked: false };
        case "discard_skill_copy": {
          const { deployment_id, owner_id } = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget({ deployment_id, owner_id });
          await updateSkill(name, (item) => ({
            ...item,
            deployments: item.deployments.filter((d) => d.id !== deployment_id),
          }));
          return undefined;
        }
        case "unlist_parked_dotagents": {
          const { deployment_id, owner_id } = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget({ deployment_id, owner_id });
          await updateSkill(name, (item) => ({
            ...item,
            deployments: item.deployments.map((d) =>
              d.id === deployment_id ? { ...d, owner_kind: "manual" } : d,
            ),
          }));
          return undefined;
        }
        case "unpark_skill": {
          const { deployment_id, owner_id } = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget({ deployment_id, owner_id });
          await updateSkill(name, (item) => unparkCopies(item, targetIdsOf(item, deployment_id)));
          recordEvent("unpark", name, false);
          return undefined;
        }

        case "split_skill_targets": {
          const { skillName, projectPath, harnesses } = z
            .object({
              skillName: z.string(),
              projectPath: z.string().nullish(),
              harnesses: z.array(z.string()),
            })
            .parse(payload);
          return harnesses.map((harness) => ({
            harness,
            path: splitCopyPath(harness, skillName, projectPath ?? null),
          }));
        }
        case "split_skill": {
          const { deployment_id } = z.object({ deployment_id: z.string() }).parse(payload.target);
          const harnesses = z.array(z.string()).parse(payload.harnesses);
          const name = skillNameForTarget({ deployment_id });
          const universal = currentSnapshot.skills
            .flatMap((item) => item.deployments)
            .find((d) => d.id === deployment_id);
          if (!universal) throw new Error(`harness: no deployment ${deployment_id}`);
          const copies = harnesses.map((harness) => ({
            harness,
            path: splitCopyPath(harness, name, universal.project_path ?? null),
          }));
          await updateSkill(name, (item) => {
            const kept = item.deployments.filter(
              (d) =>
                d.id !== deployment_id &&
                !(d.backing.kind === "linked-to" && d.backing.deployment_id === deployment_id),
            );
            const written = copies.map((copy) =>
              deployment({
                agent: deploymentLabelFromAgentId(copy.harness),
                scope: universal.scope,
                path: copy.path,
                project_path: universal.project_path,
              }),
            );
            return { ...item, deployments: [...kept, ...written] };
          });
          return {
            event_id: `harness-split-${name}`,
            deployment_id,
            skill: name,
            copies,
            removed_links: [],
            quarantine_path: `${HARNESS_HOME}/.agents/skills/.skill-studio-quarantine/${name}`,
            update_note:
              "npx skills update only updates the Universal copy, so these copies no longer get its updates.",
          };
        }

        case "turn_off_check":
          return { refusal: null, git_tracked: false, project: null };
        case "turn_off_for_agent":
          throw new Error("harness: turn_off_for_agent is not simulated");

        case "restore_moved_deployment": {
          const { deployment_id, owner_id } = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget({ deployment_id, owner_id });
          await updateSkill(name, (item) => ({
            ...item,
            deployments: item.deployments.map((entry) =>
              entry.id === deployment_id ? { ...entry, disabled: false, disabled_by: null } : entry,
            ),
          }));
          return undefined;
        }
        case "set_skills_invocation": {
          const targets = z
            .array(z.object({ name: z.string(), path: z.string() }))
            .parse(payload.targets);
          const policy =
            payload.policy === "user-only" || payload.policy === "model-only"
              ? payload.policy
              : "both";
          for (const target of targets) {
            // react-doctor-disable-next-line react-doctor/async-await-in-loop -- each update publishes the snapshot the next one reads
            await updateSkill(target.name, (item) => ({
              ...item,
              invocation: policy,
              deployments: item.deployments.map((entry) =>
                entry.path === target.path.replace(/\/SKILL\.md$/, "") || entry.path === target.path
                  ? { ...entry, invocation: policy }
                  : entry,
              ),
            }));
          }
          return targets.map(() => ({ error: null }));
        }
        case "skill_local_edits": {
          const targets = z.array(z.unknown()).parse(payload.targets);
          return targets.map(() => ({ edited: false, checked: true }));
        }
        case "park_skills":
        case "unpark_skills": {
          const parked = command === "park_skills";
          const targets = z
            .array(
              z.object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() }),
            )
            .parse(payload.targets);
          for (const target of targets) {
            const name = skillNameForTarget(target);
            // react-doctor-disable-next-line react-doctor/async-await-in-loop -- each update publishes the snapshot the next one reads
            await updateSkill(name, (item) =>
              (parked ? parkCopies : unparkCopies)(item, targetIdsOf(item, target.deployment_id)),
            );
            recordEvent(parked ? "park" : "unpark", name, false);
          }
          return targets.map(() => ({ error: null }));
        }
        case "make_skill_independent_copy":
        case "materialize_harness_root":
        case "materialize_harness_root_then_disable": {
          return undefined;
        }

        case "remove_skill": {
          const target = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget(target);
          const removed = currentSnapshot.skills.find((s) => s.name === name);
          const deploymentId =
            target.deployment_id ?? removed?.deployments[0]?.id ?? `dep:v1/mock/${name}`;
          await publish({
            ...currentSnapshot,
            skills: currentSnapshot.skills.filter((s) => s.name !== name),
          });
          const event = recordEvent("remove", name, true);
          if (removed) removedByEvent.set(event.id, removed);
          return {
            event_id: event.id,
            deployment_id: deploymentId,
            skill: name,
            tree_hash_before: "mock-tree-hash",
            quarantine_path: null,
          } satisfies RemoveOutcome;
        }
        case "update_skill": {
          const target = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget(target);
          const deployment = currentSnapshot.skills.find((s) => s.name === name)?.deployments[0];
          await updateSkill(name, (item) => ({
            ...item,
            has_update: false,
            update_owner_ids: [],
            update_owners: [],
            update_commit: null,
            update_commit_at: null,
          }));
          return {
            event_id: `fixture-update-${name}`,
            skill: name,
            deployment_path: deployment?.path ?? `${HARNESS_HOME}/.agents/skills/${name}`,
            tree_hash_before: "fixture-before",
            tree_hash_after: "fixture-after",
          };
        }
        case "update_all_skills": {
          const targets = z
            .array(
              z.object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() }),
            )
            .parse(payload.targets);
          const skillsByName = new Map(currentSnapshot.skills.map((s) => [s.name, s]));
          const items = [];
          for (const target of targets) {
            const name = skillNameForTarget(target);
            const deployment = skillsByName.get(name)?.deployments[0];
            // react-doctor-disable-next-line react-doctor/async-await-in-loop -- fixture applies each update sequentially, mirroring the real op's per-request loop
            await updateSkill(name, (item) => ({
              ...item,
              has_update: false,
              update_owner_ids: [],
              update_owners: [],
              update_commit: null,
              update_commit_at: null,
            }));
            items.push({
              skill: name,
              outcome: {
                event_id: `fixture-update-${name}`,
                skill: name,
                deployment_path: deployment?.path ?? `${HARNESS_HOME}/.agents/skills/${name}`,
                tree_hash_before: "fixture-before",
                tree_hash_after: "fixture-after",
              },
            });
          }
          return { items, errors: {}, not_run: [] };
        }

        case "cancel_update_all":
          return null;

        case "fork_skill": {
          const target = z
            .object({ deployment_id: z.string().nullish(), owner_id: z.string().nullish() })
            .parse(payload.target);
          const name = skillNameForTarget(target);
          const skillItem = currentSnapshot.skills.find((s) => s.name === name);
          return {
            forked_at: new Date().toISOString(),
            origin_tool: "dotagents",
            origin_source: skillItem?.source ?? "manual",
            repo: skillItem?.source ?? "manual",
            path: skillItem?.deployments[0]?.path ?? `${HARNESS_HOME}/.agents/skills/${name}`,
            base_commit: "fixture",
          };
        }
        case "pull_fork_upstream":
          return {
            added: [],
            removed: [],
            merged: [],
            conflicts: [],
            unchanged: 0,
            from_commit: "fixture",
            to_commit: "fixture",
            message: "Already up to date",
          };
        case "add_skill": {
          const request = addRequestSchema.parse(payload.request);
          return (await addSkills(request))[0];
        }
        case "start_add_skill_operation":
        case "start_add_skills_operation": {
          const request = addRequestSchema.parse(payload.request);
          const operationId = z.string().parse(payload.operationId);
          const identity = untrustedIdentity(request);
          if (identity) {
            pausedForTrust.set(operationId, {
              request,
              batch: command === "start_add_skills_operation",
            });
            return emitOperation({
              operation_id: operationId,
              sequence: 1,
              phase: "needs-trust",
              message:
                "This repository is not trusted. Confirm Trust repository and retry to install it.",
              untrusted_source: { identity },
            });
          }
          return completeAdd(operationId, request, command === "start_add_skills_operation");
        }
        case "get_add_skill_operation": {
          const operationId = z.string().parse(payload.operationId);
          const operation = addOperations.get(operationId);
          if (!operation) throw new Error(`harness: unknown add-skill operation ${operationId}`);
          return operation;
        }
        case "cancel_add_skill_operation": {
          const operationId = z.string().parse(payload.operationId);
          const operation = addOperations.get(operationId);
          if (!operation) throw new Error(`harness: unknown add-skill operation ${operationId}`);
          if (!pausedForTrust.delete(operationId)) return operation;
          return emitOperation({
            operation_id: operationId,
            sequence: operation.sequence + 1,
            phase: "cancelled",
            message: "Cancelled",
          });
        }
        case "confirm_add_skill_trust": {
          const { operationId, retryOperationId, identity } = z
            .object({ operationId: z.string(), retryOperationId: z.string(), identity: z.string() })
            .parse(payload);
          const paused = pausedForTrust.get(operationId);
          if (!paused)
            throw new Error(`harness: operation ${operationId} is not waiting for trust`);
          if (identity !== untrustedIdentity(paused.request)) {
            throw new Error(
              `harness: ${identity} is not the source operation ${operationId} needs`,
            );
          }
          pausedForTrust.delete(operationId);
          trustedSources.add(identity);
          return completeAdd(retryOperationId, paused.request, paused.batch, operationId);
        }

        case "repair_skill_link": {
          const { path, target } = z
            .object({
              path: z.string(),
              action: z.enum(["remove", "relink"]),
              target: z.string().nullish(),
            })
            .parse(payload);
          const name = path.split("/skills/").pop() ?? "";
          await updateSkill(name, (item) => {
            const content = target ? harnessSkillContent.get(`${target}/SKILL.md`) : undefined;
            if (target && content) harnessSkillContent.set(`${path}/SKILL.md`, content);
            return {
              ...item,
              deployments: item.deployments.map((entry) =>
                entry.path === path
                  ? {
                      ...entry,
                      symlink_is_broken: false,
                      symlink_target: target ?? entry.symlink_target,
                      resolved_path: target ?? entry.resolved_path,
                    }
                  : entry,
              ),
            };
          });
          return undefined;
        }
        case "restore_skill_event": {
          const eventId = z.string().parse(payload.eventId);
          const event = events.find((each) => each.id === eventId);
          if (!event?.restorable) throw new Error(`harness: event ${eventId} cannot be restored`);
          const removed = removedByEvent.get(eventId);
          await publish(
            removed
              ? { ...currentSnapshot, skills: [...currentSnapshot.skills, removed] }
              : currentSnapshot,
          );
          const restore = recordEvent("restore", event.skill, false);
          event.restorable = false;
          event.reverted_by = restore.id;
          return undefined;
        }

        case "plugin:path|home_dir":
        case "plugin:path|resolve_directory":
          return HARNESS_HOME;
        case "plugin:dialog|message": {
          // `ask()` sends `buttons` and treats the label it gets back as the answer: the OK label
          // means yes, any other label means no.
          const { message, title, buttons } = z
            .object({
              message: z.string(),
              title: z.string().nullish(),
              buttons: z
                .union([
                  z
                    .object({ OkCancelCustom: z.tuple([z.string(), z.string()]) })
                    .transform((custom) => custom.OkCancelCustom),
                  z.literal("YesNo").transform((): [string, string] => ["Yes", "No"]),
                ])
                .nullish(),
            })
            .parse(payload);
          const [ok, cancel] = buttons ?? ["Yes", "No"];
          const answer = askAnswers.shift() ?? true;
          askedDialogs.push({ title: title ?? null, message, answer });
          return answer ? ok : cancel;
        }
        case "plugin:dialog|save":
          return `${HARNESS_HOME}/my-skills`;
        case "plugin:dialog|open": {
          const parsedFilters = z
            .object({ filters: z.array(z.object({ extensions: z.array(z.string()) })).optional() })
            .safeParse(payload);
          const wantsApp =
            parsedFilters.success &&
            (parsedFilters.data.filters ?? []).some((filter) => filter.extensions.includes("app"));
          return wantsApp ? "/Applications/Zed Preview.app" : null;
        }
        case "plugin:opener|open_url":
        case "plugin:opener|open_path":
        case "plugin:shell|open":
          return undefined;
        case "data_folder_status":
          // The harness never runs a real data folder migration, so it is
          // always clear - App.tsx's first-run gate must never block here.
          return null;

        default:
          // eslint-disable-next-line no-console
          console.warn("[harness] unhandled invoke", command, payload);
          throw new Error(`harness: unhandled command ${command}`);
      }
    },
    { shouldMockEvents: true },
  );

  return {
    snapshot: () => currentSnapshot,
    publish,
    updateSkill,
    publishBackgroundStatus,
    asked: () => [...askedDialogs],
    answerNextAsk: (answer) => {
      askAnswers.push(answer);
    },
    writeSkillMd: (path, content) => {
      harnessSkillContent.set(path, content);
    },
  };
}
