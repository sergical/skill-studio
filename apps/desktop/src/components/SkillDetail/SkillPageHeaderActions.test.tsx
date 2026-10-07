import { createElement, createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SkillPageHeaderActions } from "./SkillPageHeaderActions";
import type { SkillPageActions } from "./skill-page-actions";

const noop = () => undefined;

function headerMarkup(parkLabel: string) {
  const actions: SkillPageActions = {
    path: "/Users/me/.agents/skills/demo",
    copied: false,
    reveal: noop,
    openEditor: noop,
    copyPath: noop,
    primaryAction: { label: "Update", run: noop, busy: false },
    parkAction: { label: parkLabel, run: noop, busy: false },
    forkAction: null,
    removeActions: [],
    removeBlockedReason: null,
    updateDialog: null,
  };
  return renderToStaticMarkup(
    createElement(SkillPageHeaderActions, {
      actions,
      assistantEnabled: false,
      isAssistantOpen: false,
      onOpenAssistant: noop,
      assistantTriggerRef: createRef<HTMLButtonElement>(),
    }),
  );
}

describe("skill page header: Park lives in the ⋯ menu, not beside Update", () => {
  // Flow: open a skill page. Expect one Park control, in ⋯, because the
  // Locations switches already park per agent. Failure: a header Park button
  // that reads as a second, competing off switch.
  it.each(["Park for every agent", "Turn on for every agent"])(
    "renders no header button for %s while the menu is closed",
    (label) => {
      const markup = headerMarkup(label);
      expect(markup).toContain("Update");
      expect(markup).toContain('aria-label="More actions"');
      expect(markup).not.toContain(label);
      expect(markup).not.toMatch(/>Park</);
    },
  );
});
