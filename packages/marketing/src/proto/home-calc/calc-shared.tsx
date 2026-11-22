// PROTO(home-calc): calculator state and the controls every variant shares.
import { useMemo, useState } from "react";
import * as stylex from "@stylexjs/stylex";

import { AgentIcon } from "../../AgentIcon";
import { siteTokens } from "../../SiteTheme.stylex";
import {
  CONTEXT_WINDOWS,
  DEFAULT_PROJECT_SKILLS,
  DEFAULT_SKILLS,
  DEFAULT_WINDOW,
  MAX_PROJECT_SKILLS,
  MAX_SKILLS,
  MIN_SKILLS,
  estimateClaude,
  estimateCodex,
  format,
} from "./budget";
import { Segmented, Slider, Switch } from "./controls";

export type Agent = "claude" | "codex";

export const STATE_FILL = {
  described: "oklch(0.75 0.17 150)",
  nameOnly: "oklch(0.8 0.15 80)",
} as const;

export function useCalculator() {
  const [agent, setAgent] = useState<Agent>("claude");
  const [windowIndex, setWindowIndex] = useState<number>(DEFAULT_WINDOW);
  const [skills, setSkills] = useState(DEFAULT_SKILLS);
  const [withProject, setWithProject] = useState(false);
  const [project, setProject] = useState(DEFAULT_PROJECT_SKILLS);
  const projectCount = withProject ? project : 0;
  const total = skills + projectCount;
  const contextWindow = CONTEXT_WINDOWS[windowIndex];
  const claude = useMemo(
    () => estimateClaude(contextWindow.tokens, total),
    [contextWindow.tokens, total],
  );
  const codex = useMemo(
    () => estimateCodex(contextWindow.tokens, total),
    [contextWindow.tokens, total],
  );
  return {
    agent,
    setAgent,
    windowIndex,
    setWindowIndex,
    contextWindow,
    skills,
    setSkills,
    withProject,
    setWithProject,
    project,
    setProject,
    projectCount,
    total,
    claude,
    codex,
  };
}

export type Calculator = ReturnType<typeof useCalculator>;

export const AGENT_OPTIONS = [
  {
    value: "claude" as const,
    label: (
      <>
        <AgentIcon agent="claude" size={16} />
        Claude Code
      </>
    ),
  },
  {
    value: "codex" as const,
    label: (
      <>
        <AgentIcon agent="codex" size={16} />
        Codex
      </>
    ),
  },
];

export const WINDOW_OPTIONS = CONTEXT_WINDOWS.map((option, index) => ({
  value: index,
  label: option.label,
}));

export function WindowControl({
  calc,
  size,
  sliding,
}: {
  calc: Calculator;
  size?: "regular" | "compact";
  sliding?: boolean;
}) {
  return (
    <div {...stylex.props(styles.field)}>
      <span {...stylex.props(styles.fieldLabel)} id="context-window-label">
        Context window
      </span>
      <Segmented
        label="Context window"
        options={WINDOW_OPTIONS}
        value={calc.windowIndex}
        onChange={calc.setWindowIndex}
        size={size}
        sliding={sliding}
      />
    </div>
  );
}

export function SkillsControl({ calc }: { calc: Calculator }) {
  return (
    <Slider
      label="Your skills"
      value={calc.skills}
      valueText={`${format(calc.skills)} skills`}
      min={MIN_SKILLS}
      max={MAX_SKILLS}
      step={5}
      onChange={calc.setSkills}
    />
  );
}

/** Project skills share the list with your own, so they can tip it over the budget. */
export function ProjectSkillsControl({ calc }: { calc: Calculator }) {
  return (
    <div {...stylex.props(styles.project)}>
      <Switch
        label="Plus project skills"
        checked={calc.withProject}
        onChange={calc.setWithProject}
      />
      {calc.withProject && (
        <Slider
          label="Project skills"
          value={calc.project}
          valueText={`+${format(calc.project)}`}
          min={5}
          max={MAX_PROJECT_SKILLS}
          step={5}
          onChange={calc.setProject}
        />
      )}
    </div>
  );
}

const styles = stylex.create({
  field: { display: "flex", flexDirection: "column", gap: 8 },
  fieldLabel: { color: siteTokens.text, fontSize: 14, fontWeight: 600 },
  project: { display: "flex", flexDirection: "column", gap: 4 },
});
