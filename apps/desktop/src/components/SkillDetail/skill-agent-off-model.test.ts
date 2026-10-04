// ============================================================================
// Skill Studio - skill-agent-off-model tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type { AgentOffCheck } from "@skill-studio/lib";
import {
  perSkillLinkDeployment,
  realCopyDeployment,
  universalDeployment,
} from "../../dev/harness/scanned-deployment";
import { skill } from "../../dev/harness/skill-fixture";
import { buildScopeGroups } from "./skill-location-status";
import {
  turnOffActionFor,
  turnOffConfirmText,
  turnOffSuccessMessage,
  turnOffView,
} from "./skill-agent-off-model";

const UNIVERSAL = "/home/.agents/skills/find-bugs";

function sharedWithClaudeLink() {
  return skill({
    name: "find-bugs",
    deployments: [
      universalDeployment({ universalPath: UNIVERSAL }),
      perSkillLinkDeployment({
        agent: "Claude Code",
        path: "/home/.claude/skills/find-bugs",
        universalPath: UNIVERSAL,
      }),
    ],
  });
}

function actionsByAgent(installed: ReturnType<typeof skill>) {
  const [global] = buildScopeGroups(installed);
  const rows = [...(global.shared ? [global.shared] : []), ...global.rows, ...global.parked];
  return Object.fromEntries(
    rows.map((row) => [`${row.kind}:${row.harness}`, turnOffActionFor(global, row)]),
  );
}

describe("turn off for one agent: confirm text", () => {
  it("confirm_text_names_the_skill_and_agent_in_the_spec_wording_or_the_user_does_not_learn_every_agent_gets_a_copy", () => {
    expect(turnOffConfirmText("find-bugs", "Codex")).toBe(
      "find-bugs is shared by every agent. To turn it off for Codex only, Skill Studio gives each agent its own copy, then parks the Codex copy. You will see one copy per agent from now on.",
    );
  });

  it("success_message_points_to_activity_for_the_undo_or_the_user_cannot_find_how_to_go_back", () => {
    expect(turnOffSuccessMessage("Codex")).toContain("Undo it from Activity");
  });
});

describe("turn off for one agent: backend check", () => {
  it("dotagents_refusal_shows_the_reason_and_offers_off_everywhere_or_the_user_hits_a_dead_end", () => {
    const check: AgentOffCheck = {
      refusal: {
        reason: 'dotagents manages this skill. Use "Off everywhere" instead.',
        off_everywhere: true,
      },
      git_tracked: null,
      project: null,
    };

    expect(turnOffView(check)).toEqual({
      kind: "refused",
      reason: 'dotagents manages this skill. Use "Off everywhere" instead.',
      offEverywhere: true,
    });
  });

  it("plugin_refusal_shows_the_reason_only_or_off_everywhere_would_park_a_plugin_copy_core_refuses_to_park", () => {
    const check: AgentOffCheck = {
      refusal: {
        reason: "a plugin copy cannot be turned off for one agent",
        off_everywhere: false,
      },
      git_tracked: null,
      project: null,
    };

    expect(turnOffView(check)).toMatchObject({ kind: "refused", offEverywhere: false });
  });

  it("no_refusal_shows_the_confirm_or_a_turn_off_the_backend_allows_would_be_blocked", () => {
    expect(turnOffView({ refusal: null, git_tracked: false, project: null })).toEqual({
      kind: "confirm",
    });
  });
});

describe("turn off for one agent: which rows offer it", () => {
  it("reader_and_link_rows_under_a_live_shared_folder_offer_it_with_the_shared_target_or_the_split_would_run_on_the_link", () => {
    const actions = actionsByAgent(sharedWithClaudeLink());

    expect(actions["link:claude-code"]).toMatchObject({
      kind: "turn-off-agent",
      agent: "claude-code",
      agentLabel: "Claude Code",
      projectPath: null,
    });
    expect(actions["reader:codex"]).toMatchObject({ kind: "turn-off-agent", agent: "codex" });
    const sharedId = buildScopeGroups(sharedWithClaudeLink())[0].shared?.deployment?.id;
    const link = actions["link:claude-code"];
    expect(link?.kind === "turn-off-agent" ? link.target.deployment_id : null).toBe(sharedId);
  });

  it("the_shared_folder_row_has_no_turn_off_for_agent_or_the_control_would_mean_every_agent", () => {
    expect(actionsByAgent(sharedWithClaudeLink())["shared:shared"]).toBeNull();
  });

  it("an_agent_with_its_own_copy_has_no_turn_off_for_agent_or_it_would_split_a_folder_it_does_not_read", () => {
    const installed = skill({
      name: "find-bugs",
      deployments: [
        universalDeployment({ universalPath: UNIVERSAL }),
        realCopyDeployment({ agent: "Claude Code", path: "/home/.claude/skills/find-bugs" }),
      ],
    });

    expect(actionsByAgent(installed)["copy:claude-code"]).toBeNull();
  });

  it("a_skill_with_no_shared_folder_has_no_turn_off_for_agent_or_there_is_nothing_to_split", () => {
    const installed = skill({
      name: "find-bugs",
      deployments: [
        realCopyDeployment({ agent: "Claude Code", path: "/home/.claude/skills/find-bugs" }),
      ],
    });

    const actions = Object.values(actionsByAgent(installed));

    expect(actions.length).toBeGreaterThan(0);
    expect(actions.every((action) => action === null)).toBe(true);
  });

  it("a_parked_shared_folder_offers_nothing_or_the_user_could_turn_off_an_agent_for_a_skill_that_is_already_off", () => {
    const installed = skill({
      name: "find-bugs",
      deployments: [
        universalDeployment({ universalPath: UNIVERSAL }, { disabled: true }),
        perSkillLinkDeployment({
          agent: "Claude Code",
          path: "/home/.claude/skills/find-bugs",
          universalPath: UNIVERSAL,
        }),
      ],
    });

    expect(actionsByAgent(installed)["link:claude-code"]).toBeNull();
  });
});
