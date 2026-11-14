// ============================================================================
// SkillRowCells - the shared cells of a skill row on Home and Skills: the row
// slot (state icon or checkbox), the name, and the note (reason and tokens).
// ============================================================================

import type { ComponentProps, ReactNode } from "react";
import {
  CircleAlert,
  CircleArrowDown,
  CirclePause,
  OctagonAlert,
  Sparkles,
  TriangleAlert,
  UserRound,
} from "lucide-react";
import { formatTokens } from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { CheckboxControl } from "../ui/CheckboxControl";
import { RichTooltip } from "../ui/RichTooltip";
import type { RowLevel, RowState } from "./skill-row-state";

const LEVEL_TEXT = {
  error: "text-error",
  warning: "text-warning",
  info: "text-accent",
  muted: "text-text-tertiary",
} satisfies Record<RowLevel, string>;

function glyphFor(state: RowState, size: number): ReactNode {
  switch (state.kind) {
    case "violation":
      return <OctagonAlert size={size} aria-hidden />;
    case "rollup":
    case "issue":
      return state.level === "error" ? (
        <CircleAlert size={size} aria-hidden />
      ) : (
        <TriangleAlert size={size} aria-hidden />
      );
    case "update":
      return <CircleArrowDown size={size} aria-hidden />;
    case "parked":
      return <CirclePause size={size} aria-hidden />;
  }
}

/** The state icon in its level colour, with an `sr-only` label so the state reads without
 * sight of the colour. */
export function RowGlyph({ state, size: sizeProp }: { state: RowState; size?: number }) {
  return (
    <span className={`inline-flex items-center ${LEVEL_TEXT[state.level]}`}>
      {glyphFor(state, sizeProp ?? 14)}
      <span className="sr-only">{state.label}</span>
    </span>
  );
}

/** The Tokens cell's tooltip: the two costs a skill's Tokens column tracks - the prompt line
 * every harness loads on every turn, and the full SKILL.md paid only when the skill runs. */
function TokensTooltip({ skill }: { skill: InstalledSkill }) {
  return (
    <div className="grid grid-cols-1 gap-1 text-small">
      <div className="flex flex-col gap-0.5">
        <span className="tabular-nums text-text-primary">
          {skill.description_tokens.toLocaleString()} tokens · prompt cost
        </span>
        <span className="text-caption text-text-tertiary">
          name + description, loaded every turn by every agent that reaches it
        </span>
      </div>
      <div className="flex flex-col gap-0.5">
        <span className="tabular-nums text-text-primary">
          {skill.skill_md_tokens.toLocaleString()} tokens · full SKILL.md
        </span>
        <span className="text-caption text-text-tertiary">loaded when the skill is used</span>
      </div>
    </div>
  );
}

const INVOCATION_HEADER = {
  "user-only": "You run it",
  "model-only": "The model runs it",
} satisfies Record<"user-only" | "model-only", string>;

/** Whether a skill is user-invoked only or model-invoked only; "both" needs no mark, so this
 * renders nothing for the common case and the row stays quiet. */
function InvocationMark({ skill }: { skill: InstalledSkill }) {
  if (skill.invocation === "both") return null;
  const header = INVOCATION_HEADER[skill.invocation];
  return (
    <RichTooltip content={<span className="text-small">{header}</span>}>
      <span
        role="img"
        aria-label={header}
        className="inline-flex size-4 shrink-0 items-center justify-center text-text-tertiary"
      >
        {skill.invocation === "user-only" ? <UserRound size={12} /> : <Sparkles size={12} />}
      </span>
    </RichTooltip>
  );
}

/** The skill's name: a truncated label plus its `InvocationMark`. */
export function SkillNameCell({ skill }: { skill: InstalledSkill }) {
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <span className="min-w-0 truncate text-body text-text-primary">{skill.name}</span>
      <InvocationMark skill={skill} />
    </span>
  );
}

/** Both token numbers for one skill: the prompt cost first, then the full SKILL.md count. */
function TokenPairCell({ skill }: { skill: InstalledSkill }) {
  const promptText = skill.description_tokens > 0 ? formatTokens(skill.description_tokens) : "–";
  return (
    <RichTooltip content={<TokensTooltip skill={skill} />}>
      <span className="inline-flex shrink-0 items-baseline justify-end gap-1.5 tabular-nums text-small">
        <span className="text-text-tertiary">{promptText}</span>
        <span className="text-text-primary">{formatTokens(skill.skill_md_tokens)}</span>
      </span>
    </RichTooltip>
  );
}

type CheckboxChange = ComponentProps<typeof CheckboxControl>["onCheckedChange"];

interface SlotCellProps {
  skill: InstalledSkill;
  state: RowState | null;
  checked: boolean;
  /** Whether any row on the page is checked: a row with no state then shows its checkbox. */
  hasSelection: boolean;
  onCheckedChange: CheckboxChange;
}

/** The row's first cell: the state icon at rest, the selection checkbox in the same place on
 * hover, focus, or once checked. */
export function SlotCell({ skill, state, checked, hasSelection, onCheckedChange }: SlotCellProps) {
  const checkboxShown = checked || (hasSelection && state === null);
  return (
    <span className="relative inline-flex size-3.5 items-center justify-center">
      {state && !checkboxShown && (
        <span className="pointer-events-none absolute inset-0 inline-flex items-center justify-center group-hover/row:opacity-0 group-focus-within/row:opacity-0">
          <RowGlyph state={state} />
        </span>
      )}
      <span
        className={`inline-flex items-center justify-center ${
          checkboxShown
            ? ""
            : "opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <CheckboxControl
          checked={checked}
          onCheckedChange={onCheckedChange}
          ariaLabel={`Select ${skill.name}`}
        />
      </span>
    </span>
  );
}

/** The row's note: why the row is here, then its token pair at the right edge. */
export function NoteCell({ skill, note }: { skill: InstalledSkill; note?: ReactNode }) {
  return (
    <span className="flex min-w-0 items-center justify-between gap-2">
      <span className="min-w-0 truncate text-small text-text-secondary select-text">{note}</span>
      <TokenPairCell skill={skill} />
    </span>
  );
}
