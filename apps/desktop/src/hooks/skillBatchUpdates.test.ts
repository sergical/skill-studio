// ============================================================================
// skillBatchUpdates.test - Home "Update all" Cancel, through a mocked IPC bridge
// ============================================================================

import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { Deployment, InstalledSkill, UpdateOutcome } from "@skill-studio/lib";
import { updateAllTitle } from "../components/Home/home-inbox-data";
import { requestUpdateAllStop, runHomeUpdateAll } from "./skillBatchUpdates";
import { newUpdateAllControl } from "./skillBatchUpdates";

// Plain Node environment: `mockIPC` needs a `window` to hang its bridge off and `recordIpcCall`
// needs a frame scheduler, so both are added to `globalThis`.
Object.assign(globalThis, {
  window: globalThis,
  requestAnimationFrame: (callback: () => void) => setTimeout(callback, 0),
});

const calls: string[] = [];
const payloads: { command: string; payload: unknown }[] = [];

/** Records every command and answers it through `respond`; event plumbing is answered by `mockIPC` itself. */
type IpcReply = object | null;

function mockCommands(respond: (command: string) => IpcReply | Promise<IpcReply>) {
  calls.length = 0;
  payloads.length = 0;
  mockIPC(
    (command, payload) => {
      calls.push(command);
      payloads.push({ command, payload });
      return respond(command);
    },
    { shouldMockEvents: true },
  );
}

afterEach(() => clearMocks());

const deployment: Deployment = {
  id: "dep:v1/global/1/universal/forked/-/1",
  destination: "universal",
  owner_kind: "fork",
  owner_id: null,
  mutability: "mutable",
  backing: { kind: "canonical" },
  agent: "shared",
  scope: "global",
  path: "/home/.agents/skills/forked",
  is_symlink: false,
  symlink_is_broken: false,
  content_hash: "x",
  disabled: false,
  codex_implicit_invocation: null,
  disabled_by: null,
  invocation: "both",
  spec_violations: [],
  shared_via_whole_dir_link: false,
};

function outdated(name: string, kind: "fork" | "skills-sh"): InstalledSkill {
  const ownerId = `owner:v1/global/${name}`;
  return {
    name,
    source: `owner/${name}`,
    source_type: "github",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: true,
    source_kind: kind,
    deployments: [deployment],
    has_spec: true,
    spec_violations: [],
    skill_md_tokens: 0,
    description_tokens: 0,
    folder_bytes: 0,
    file_count: 0,
    content_hash: "",
    content_hashes: [],
    frontmatter_fields: {},
    folder_truncated: false,
    parked: false,
    invocation: "both",
    update_owner_ids: [ownerId],
    update_owners: [{ owner_id: ownerId, latest_commit: "next", latest_commit_at: null }],
    description: null,
    fork: null,
    parked_at: null,
    skill_path: null,
    source_url: null,
    update_commit: null,
    update_commit_at: null,
    updated_at: null,
  };
}

const pullResult = {
  from_commit: "aaa",
  to_commit: "bbb",
  merged: [],
  conflicts: [],
  added: [],
  removed: [],
  unchanged: 0,
  message: null,
};

function outcomeFor(skill: string): UpdateOutcome {
  return {
    event_id: `evt-${skill}`,
    skill,
    deployment_path: `/home/.agents/skills/${skill}`,
    tree_hash_before: "a",
    tree_hash_after: "b",
  };
}

describe("Home Update all cancel", () => {
  it("cancel_during_the_fork_phase_calls_cancel_update_all_and_skips_the_owner_batch_or_runs_every_phase_anyway", async () => {
    const control = newUpdateAllControl();
    mockCommands(async (command) => {
      if (command === "pull_fork_upstream") {
        await requestUpdateAllStop(control);
        return pullResult;
      }
      return null;
    });

    const tally = await runHomeUpdateAll(
      [outdated("forked", "fork"), outdated("alpha", "skills-sh"), outdated("beta", "skills-sh")],
      () => {},
      undefined,
      control,
    );

    expect(calls).toEqual(["pull_fork_upstream", "cancel_update_all"]);
    expect(tally.stopped).toBe(true);
    expect(updateAllTitle(tally)).toBe("Stopped. Updated 1 of 3 skills.");
  });

  it("an_update_all_without_cancel_runs_every_phase_and_keeps_the_plain_title_or_stops_early", async () => {
    mockCommands((command) => {
      if (command === "pull_fork_upstream") return pullResult;
      if (command === "update_all_skills") {
        return {
          items: [{ skill: "alpha", outcome: outcomeFor("alpha") }],
          errors: {},
          not_run: [],
        };
      }
      return null;
    });

    const tally = await runHomeUpdateAll(
      [outdated("forked", "fork"), outdated("alpha", "skills-sh")],
      () => {},
      undefined,
      newUpdateAllControl(),
    );

    expect(calls).toEqual(["pull_fork_upstream", "update_all_skills"]);
    expect(tally.stopped).toBeUndefined();
    expect(updateAllTitle(tally)).toBe("Updated 2 of 2 skills");
  });

  it("skills_the_backend_reports_as_not_run_count_as_not_updated_or_the_toast_overstates_the_batch", async () => {
    mockCommands((command) => {
      if (command !== "update_all_skills") return null;
      return {
        items: [{ skill: "alpha", outcome: outcomeFor("alpha") }],
        errors: {},
        not_run: ["beta", "gamma"],
      };
    });

    const tally = await runHomeUpdateAll(
      ["alpha", "beta", "gamma"].map((name) => outdated(name, "skills-sh")),
      () => {},
    );

    expect(updateAllTitle(tally)).toBe("Stopped. Updated 1 of 3 skills.");
  });

  it("a_cancel_before_the_backend_starts_names_the_same_batch_as_the_update_or_the_late_start_runs_everything", async () => {
    const control = newUpdateAllControl();
    mockCommands((command) => {
      if (command === "pull_fork_upstream") return pullResult;
      return null;
    });

    await requestUpdateAllStop(control);
    await runHomeUpdateAll([outdated("alpha", "skills-sh")], () => {}, undefined, control);

    const cancel = payloads.find((call) => call.command === "cancel_update_all");
    expect(cancel?.payload).toMatchObject({ batchId: control.batchId });
    expect(calls).not.toContain("update_all_skills");
  });
});
