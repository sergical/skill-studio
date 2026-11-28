import { memo } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, Copy } from "lucide-react";

import { AgentIcon } from "../../AgentIcon";
import { homeSectionStyles } from "../HomeSection.stylex";
import { useCopyFlash } from "../useCopyFlash";
import { siteTokens } from "../../SiteTheme.stylex";
import {
  CONTEXT_WINDOWS,
  ESTIMATE_NOTE,
  MAX_PROJECT_SKILLS,
  MAX_SKILLS,
  claudeFollowUp,
  claudeSentence,
  codexFollowUp,
  codexSentence,
  format,
} from "./budget";
import {
  AGENT_OPTIONS,
  ClaudeFractionControl,
  CodexOverrideControl,
  ProjectSkillsControl,
  SkillsControl,
  useCalculator,
  type Calculator,
} from "./calc-shared";
import { Segmented, Slider } from "./controls";

const DIGITS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as const;
const LISTING_COMMAND = "npx skill-studio@latest listing";
const MAX_DOTS = MAX_SKILLS + MAX_PROJECT_SKILLS;
const DOT_SLOTS = Array.from({ length: MAX_DOTS }, (_, index) => index);

type DotState = "described" | "nameOnly" | "leftOut" | "off";

/**
 * Vertical digit reel keyed by place value from the right, so 98 -> 145 rolls the ones and tens
 * slots cleanly without scale bounce or horizontal shift.
 */
function RollingNumber({ value }: { value: number }) {
  const formatted = format(value);
  const chars = formatted.split("");
  return (
    <span {...stylex.props(styles.bigNumber)}>
      <span {...stylex.props(styles.visuallyHidden)}>{formatted}</span>
      <span aria-hidden="true" {...stylex.props(styles.rollingRow)}>
        {chars.map((char, index) => {
          const place = chars.length - 1 - index;
          const digit = Number(char);
          if (Number.isNaN(digit)) {
            return (
              <span key={`sep-${place}`} {...stylex.props(styles.digitSep)}>
                {char}
              </span>
            );
          }
          return (
            <span key={`d-${place}`} {...stylex.props(styles.digitMask)}>
              <span {...stylex.props(styles.digitReel, styles.digitAt(digit))}>
                {DIGITS.map((d) => (
                  <span key={d} {...stylex.props(styles.digitCell)}>
                    {d}
                  </span>
                ))}
              </span>
            </span>
          );
        })}
      </span>
    </span>
  );
}

// Colours change in a wave from the first dot to the last, capped so a big list still settles fast.
const Dot = memo(function Dot({
  state,
  index,
  project,
}: {
  state: DotState;
  index: number;
  project: boolean;
}) {
  return (
    <span
      style={{ transitionDelay: `${Math.min(index * 1.2, 320)}ms` }}
      {...stylex.props(styles.dot, styles[state], project && styles.dotProject)}
    />
  );
});

// Claude Code counts its list in characters; show the fraction of the window in tokens so both
// agents read in the same unit.
function budgetTagFor(calc: Calculator) {
  if (calc.agent === "codex") return `${format(calc.codex.budget)} tokens`;
  return `${format(Math.round(calc.claude.budgetTokens))} tokens`;
}

export function KineticCalculator() {
  const calc = useCalculator();
  const { copied, copy } = useCopyFlash();
  const isClaude = calc.agent === "claude";
  const kept = isClaude ? calc.claude.described : calc.total - calc.codex.leftOut;

  const stateAt = (index: number): DotState => {
    if (index >= calc.total) return "off";
    if (isClaude) return index < calc.claude.described ? "described" : "nameOnly";
    if (calc.codex.full > 0) return "described";
    if (calc.codex.shortened > 0) return "nameOnly";
    return index < calc.total - calc.codex.leftOut ? "nameOnly" : "leftOut";
  };

  const windowLabel = CONTEXT_WINDOWS[calc.windowIndex].label;

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.calcTitle)}>How many skills fit?</h2>
      <p {...stylex.props(styles.calcLede)}>
        Claude Code and Codex give the model a list of your skills, and the list has a size limit.
        Change the window or the budget and watch which skills keep their description.
      </p>
      <div {...stylex.props(styles.calc)}>
        <div {...stylex.props(styles.panel)}>
          <Segmented
            label="Agent"
            options={AGENT_OPTIONS}
            value={calc.agent}
            onChange={calc.setAgent}
            sliding
            equalWidth
          />
          <Slider
            label="Context window"
            value={calc.windowIndex}
            valueText={windowLabel}
            min={0}
            max={CONTEXT_WINDOWS.length - 1}
            onChange={calc.setWindowIndex}
            springy
            marks={CONTEXT_WINDOWS.map((option) => option.label)}
          />
          {isClaude ? <ClaudeFractionControl calc={calc} /> : <CodexOverrideControl calc={calc} />}
          <SkillsControl calc={calc} />
          <ProjectSkillsControl calc={calc} />
        </div>
        <div {...stylex.props(styles.stage)}>
          <div {...stylex.props(styles.headline)}>
            <RollingNumber value={kept} />
            <span {...stylex.props(styles.bigLabel)}>
              of {format(calc.total)}
              <br />
              {isClaude
                ? "keep their description"
                : calc.codex.full
                  ? "fit in full"
                  : "stay in the list"}
            </span>
            <span {...stylex.props(styles.budgetTag)}>
              <AgentIcon agent={calc.agent} size={14} />
              {budgetTagFor(calc)}
            </span>
          </div>
          <div aria-hidden="true" {...stylex.props(styles.field)}>
            {DOT_SLOTS.map((index) => (
              <Dot
                key={index}
                index={index}
                state={stateAt(index)}
                project={index >= calc.skills && index < calc.total}
              />
            ))}
          </div>
          <p role="status" {...stylex.props(styles.result)}>
            {isClaude ? claudeSentence(calc.claude) : codexSentence(calc.codex)}{" "}
            <span {...stylex.props(styles.resultFollow)}>
              {isClaude ? claudeFollowUp(calc.claude) : codexFollowUp(calc.codex)}
            </span>
          </p>
          <ul aria-hidden="true" {...stylex.props(styles.legend)}>
            <li {...stylex.props(styles.legendItem)}>
              <span {...stylex.props(styles.key, styles.described)} />
              {isClaude ? "Described" : "Full"}
            </li>
            <li {...stylex.props(styles.legendItem)}>
              <span {...stylex.props(styles.key, styles.nameOnly)} />
              {isClaude ? "Name only" : "Shortened"}
            </li>
            {!isClaude && (
              <li {...stylex.props(styles.legendItem)}>
                <span {...stylex.props(styles.key, styles.leftOut)} />
                Left out
              </li>
            )}
            {calc.withProject && (
              <li {...stylex.props(styles.legendItem)}>
                <span {...stylex.props(styles.key, styles.described, styles.dotProject)} />
                Project skill
              </li>
            )}
          </ul>
          <p {...stylex.props(styles.note)}>{ESTIMATE_NOTE}</p>
          <div {...stylex.props(styles.ctaBar)}>
            <span {...stylex.props(styles.ctaLabel)}>Measure your real list</span>
            <div {...stylex.props(styles.ctaActions)}>
              <button
                type="button"
                onClick={() => copy(LISTING_COMMAND)}
                aria-label={copied ? "Copied command" : `Copy ${LISTING_COMMAND}`}
                {...stylex.props(styles.cmdButton, copied && styles.cmdButtonCopied)}
              >
                <code {...stylex.props(homeSectionStyles.code, styles.cmdText)}>
                  {LISTING_COMMAND}
                </code>
                <span
                  aria-hidden="true"
                  {...stylex.props(styles.cmdIcon, copied && styles.cmdIconCopied)}
                >
                  {copied ? <Check size={14} /> : <Copy size={14} />}
                </span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

const styles = stylex.create({
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
  calcTitle: { marginBottom: 14 },
  calcLede: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.6,
    margin: "0 0 36px",
    maxWidth: "58ch",
    textWrap: "pretty",
  },
  calc: {
    display: "grid",
    gap: 20,
    gridTemplateColumns: "340px minmax(0, 1fr)",
    "@media (max-width: 900px)": { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  panel: {
    backgroundColor: siteTokens.surface,
    borderRadius: 16,
    boxShadow: `0 0 0 1px ${siteTokens.border}`,
    display: "flex",
    flexDirection: "column",
    gap: 18,
    padding: 20,
  },
  stage: {
    borderRadius: 16,
    boxShadow: `0 0 0 1px ${siteTokens.border}`,
    display: "flex",
    flexDirection: "column",
    gap: 18,
    minWidth: 0,
    padding: "22px 22px 18px",
  },
  headline: { alignItems: "flex-end", display: "flex", flexWrap: "wrap", gap: "6px 14px" },
  bigNumber: {
    color: siteTokens.text,
    display: "inline-block",
    fontSize: "clamp(56px, 6vw, 84px)",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 720,
    letterSpacing: "-.045em",
    lineHeight: 1,
  },
  rollingRow: {
    display: "inline-flex",
    alignItems: "flex-end",
  },
  digitMask: {
    display: "inline-block",
    height: "1em",
    lineHeight: 1,
    overflow: "hidden",
  },
  digitReel: {
    display: "flex",
    flexDirection: "column",
    transition: {
      default: "transform 240ms cubic-bezier(0.23, 1, 0.32, 1)",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    willChange: "transform",
  },
  digitAt: (digit: number) => ({ transform: `translateY(${-digit * 10}%)` }),
  digitCell: {
    display: "block",
    height: "1em",
    lineHeight: 1,
    textAlign: "center",
  },
  digitSep: {
    display: "inline-block",
    lineHeight: 1,
  },
  bigLabel: {
    color: siteTokens.muted,
    fontSize: 15,
    fontWeight: 560,
    lineHeight: 1.3,
    paddingBottom: 6,
  },
  budgetTag: {
    alignItems: "center",
    alignSelf: "flex-start",
    backgroundColor: siteTokens.surface,
    borderRadius: 999,
    boxShadow: `inset 0 0 0 1px ${siteTokens.border}`,
    color: siteTokens.muted,
    display: "inline-flex",
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    gap: 6,
    height: 28,
    marginLeft: "auto",
    paddingInline: 10,
  },
  field: {
    display: "grid",
    gap: 4,
    gridTemplateColumns: "repeat(auto-fill, minmax(10px, 1fr))",
  },
  dot: {
    aspectRatio: "1",
    borderRadius: 3,
    transitionDuration: { default: "200ms", "@media (prefers-reduced-motion: reduce)": "0ms" },
    transitionProperty: "background-color, opacity, transform, box-shadow",
    transitionTimingFunction: "ease-out",
  },
  described: { backgroundColor: siteTokens.success },
  nameOnly: { backgroundColor: siteTokens.warning },
  leftOut: { backgroundColor: "transparent", boxShadow: `inset 0 0 0 1px ${siteTokens.muted}` },
  // Quiet capacity pegs keep the stage height fixed without visually competing with active skills.
  off: {
    backgroundColor: siteTokens.muted,
    borderRadius: 999,
    opacity: 0.2,
    transform: "scale(0.32)",
  },
  dotProject: { boxShadow: `inset 0 0 0 2px ${siteTokens.background}` },
  // Room for three lines, so a longer sentence does not push the legend down.
  result: {
    minHeight: "3.6em",
    color: siteTokens.text,
    fontSize: "clamp(22px, 2.2vw, 30px)",
    fontWeight: 650,
    letterSpacing: "-.025em",
    lineHeight: 1.2,
    margin: 0,
    maxWidth: "34ch",
    textWrap: "balance",
  },
  resultFollow: { color: siteTokens.muted, fontWeight: 560 },
  legend: {
    color: siteTokens.muted,
    display: "flex",
    flexWrap: "wrap",
    fontSize: 12,
    gap: "6px 16px",
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  legendItem: { alignItems: "center", display: "inline-flex", gap: 6 },
  key: { borderRadius: 3, display: "inline-block", height: 10, width: 10 },
  note: { color: siteTokens.muted, fontSize: 12.5, lineHeight: 1.5, margin: 0, maxWidth: "68ch" },
  ctaBar: {
    alignItems: "center",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: "10px 16px",
    justifyContent: "space-between",
    marginTop: "auto",
    paddingTop: 14,
  },
  ctaLabel: {
    color: siteTokens.text,
    fontSize: 13.5,
    fontWeight: 600,
    whiteSpace: "nowrap",
  },
  ctaActions: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: "8px 16px",
    minWidth: 0,
  },
  cmdButton: {
    alignItems: "center",
    backgroundColor: siteTokens.surface,
    borderColor: { default: siteTokens.border, ":hover": siteTokens.muted },
    borderRadius: 9,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    gap: 10,
    maxWidth: "100%",
    minHeight: 38,
    minWidth: 0,
    padding: "0 10px 0 12px",
    position: "relative",
    transition: "border-color 150ms ease-out, color 150ms ease-out, transform 150ms ease-out",
    "::before": { content: '""', inset: "-3px 0", position: "absolute" },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  cmdButtonCopied: {
    borderColor: { default: siteTokens.success, ":hover": siteTokens.success },
    color: siteTokens.success,
  },
  cmdText: {
    fontSize: 12.5,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  cmdIcon: {
    color: siteTokens.muted,
    display: "inline-flex",
    flexShrink: 0,
  },
  cmdIconCopied: {
    color: siteTokens.success,
  },
});
