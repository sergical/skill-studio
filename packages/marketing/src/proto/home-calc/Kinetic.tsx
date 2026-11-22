// PROTO(home-calc) variant "Kinetic": the hero reacts to the pointer's position and speed.
// A dither-dot halo springs after the pointer (mass 0.1), stretches along its velocity and
// settles around the download button; the dither brightens with pointer speed. The headline
// rises in word by word. The calculator draws each skill as a dot that recolours in a wave.
import { memo, useEffect, useRef } from "react";
import * as stylex from "@stylexjs/stylex";

import { AgentIcon } from "../../AgentIcon";
import { homeSectionStyles } from "../../home/HomeSection.stylex";
import { siteLayout } from "../../SiteChrome";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import { useReducedMotion } from "../../use-reduced-motion";
import {
  CLAUDE_LINE,
  CODEX_LINE_COPY,
  CONTEXT_WINDOWS,
  ESTIMATE_NOTE,
  MAX_PROJECT_SKILLS,
  MAX_SKILLS,
  claudeBudgetLine,
  claudeFollowUp,
  claudeSentence,
  codexBudgetLine,
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
  type Agent,
  type Calculator,
} from "./calc-shared";
import { Segmented, Slider } from "./controls";
import {
  CopyAgentPromptButton,
  DownloadButton,
  HERO_SUBLINE,
  HeroDither,
  TrustLine,
  heroGlow,
  heroPartStyles,
} from "./HeroParts";
import { SkillThrow } from "./pile/SkillThrow";
import { ProtoPage, type VariantProps } from "./ProtoPage";
import { useHeroPointer } from "./useHeroPointer";

const EASE_OUT = "cubic-bezier(0.22, 1, 0.36, 1)";
const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";
const WORDS = ["Fix", "your", "skill", "issues."];

const rise = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(0.32em)" },
});
const fadeUp = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(10px)" },
});
const draw = stylex.keyframes({
  from: { strokeDashoffset: 1 },
});
const fadeOnly = stylex.keyframes({ from: { opacity: 0 } });

function Hero({ theme }: { theme: SiteTheme }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const followerRef = useRef<HTMLDivElement>(null);
  const ctaRef = useRef<HTMLAnchorElement>(null);
  useHeroPointer({ root: rootRef, follower: followerRef, speed: layerRef, magnet: ctaRef });
  return (
    <div ref={rootRef} {...stylex.props(styles.heroWrap, heroGlow.kinetic)}>
      <HeroDither theme={theme} layerRef={layerRef} />
      <div aria-hidden="true" {...stylex.props(styles.followerLayer)}>
        <div ref={followerRef} {...stylex.props(styles.follower)} />
      </div>
      <section {...stylex.props(siteLayout.container, styles.hero)}>
        <div {...stylex.props(styles.copy)}>
          <h1 {...stylex.props(styles.title)}>
            {WORDS.map((word, index) => (
              <span key={word}>
                <span {...stylex.props(styles.word, styles.wordDelay(index * 70))}>
                  {word === "issues." ? (
                    <span {...stylex.props(styles.underlined)}>
                      {word}
                      <svg
                        aria-hidden="true"
                        viewBox="0 0 200 16"
                        preserveAspectRatio="none"
                        {...stylex.props(styles.squiggle)}
                      >
                        <path
                          d="M2 10 C 22 2, 38 2, 54 9 S 88 16, 104 9 S 138 2, 154 9 S 186 15, 198 7"
                          pathLength={1}
                          {...stylex.props(styles.squigglePath)}
                        />
                      </svg>
                    </span>
                  ) : (
                    word
                  )}
                </span>
                {index < WORDS.length - 1 ? " " : ""}
              </span>
            ))}
          </h1>
          <p {...stylex.props(styles.lede, styles.enter(300))}>{HERO_SUBLINE}</p>
          <div {...stylex.props(heroPartStyles.actions, styles.enter(380))}>
            <DownloadButton theme={theme} place="hero" buttonRef={ctaRef} />
            <CopyAgentPromptButton />
          </div>
          <div {...stylex.props(styles.fade(460))}>
            <TrustLine />
          </div>
        </div>
        <div {...stylex.props(styles.visual)}>
          <SkillThrow />
        </div>
      </section>
    </div>
  );
}

const MAX_DOTS = MAX_SKILLS + MAX_PROJECT_SKILLS;
const DOT_SLOTS = Array.from({ length: MAX_DOTS }, (_, index) => index);

type DotState = "described" | "nameOnly" | "leftOut" | "off";

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

function agentLineFor(agent: Agent, budgets: boolean) {
  if (!budgets) return agent === "claude" ? CLAUDE_LINE : CODEX_LINE_COPY;
  return agent === "claude"
    ? "Claude Code lists skills in a share of the context window, set by skillListingBudgetFraction."
    : "Codex lists skills in 2% of the context window, unless skills.max_context_tokens sets a number.";
}

// Claude Code counts its list in characters; with `budgets` the tag shows the fraction of the
// window in tokens, so both agents read in the same unit.
function budgetTagFor(calc: Calculator, budgets: boolean) {
  if (calc.agent === "codex") return `${format(calc.codex.budget)} tokens`;
  if (budgets) return `${format(Math.round(calc.claude.budgetTokens))} tokens`;
  return `${format(calc.claude.budget)} characters`;
}

/** With `budgets`, the panel also sets Claude Code's fraction and Codex's token setting. */
export function KineticCalculator({ budgets = false }: { budgets?: boolean }) {
  const calc = useCalculator();
  const reduced = useReducedMotion();
  const numberRef = useRef<HTMLSpanElement>(null);
  const isClaude = calc.agent === "claude";
  const kept = isClaude ? calc.claude.described : calc.codex.full + calc.codex.shortened;

  // The big number gives one small pop once the value stops changing, not on every drag frame.
  const shown = `${calc.agent}:${kept}`;
  const popped = useRef(shown);
  useEffect(() => {
    if (reduced || shown === popped.current) return;
    const timer = window.setTimeout(() => {
      popped.current = shown;
      numberRef.current?.animate([{ scale: "1" }, { scale: "1.06" }, { scale: "1" }], {
        duration: 220,
        easing: EASE_OUT,
      });
    }, 140);
    return () => window.clearTimeout(timer);
  }, [shown, reduced]);

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
        {budgets
          ? " Change the window or the setting and watch which skills keep their description."
          : " Slide the window and watch which skills keep their description."}
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
          <p {...stylex.props(styles.agentLine)}>{agentLineFor(calc.agent, budgets)}</p>
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
          {budgets && (
            <div {...stylex.props(styles.budgetSetting)}>
              {isClaude ? (
                <ClaudeFractionControl calc={calc} />
              ) : (
                <CodexOverrideControl calc={calc} />
              )}
              <p {...stylex.props(styles.budgetLine)}>
                {isClaude
                  ? claudeBudgetLine(calc.claude, windowLabel)
                  : codexBudgetLine(calc.codex, windowLabel)}
              </p>
            </div>
          )}
          <SkillsControl calc={calc} />
          <ProjectSkillsControl calc={calc} />
        </div>
        <div {...stylex.props(styles.stage)}>
          <div {...stylex.props(styles.headline)}>
            <span ref={numberRef} {...stylex.props(styles.bigNumber)}>
              {format(kept)}
            </span>
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
              {budgetTagFor(calc, budgets)}
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
        </div>
      </div>
    </>
  );
}

export function Kinetic(props: VariantProps) {
  return (
    <ProtoPage {...props} hero={<Hero theme={props.theme} />} calculator={<KineticCalculator />} />
  );
}

const styles = stylex.create({
  heroWrap: { isolation: "isolate", position: "relative" },
  // Clips the follower, so its transform can never add scrollable overflow to the page.
  followerLayer: {
    inset: 0,
    overflow: "clip",
    pointerEvents: "none",
    position: "absolute",
    zIndex: -1,
  },
  // A disc of 3px dither dots, so the follower reads as part of the dither field.
  follower: {
    backgroundImage: "radial-gradient(circle, oklch(0.72 0.17 293) 0.9px, transparent 1.4px)",
    backgroundSize: "3px 3px",
    borderRadius: "50%",
    height: 132,
    left: 0,
    maskImage: "radial-gradient(closest-side, black 25%, transparent)",
    opacity: 0,
    pointerEvents: "none",
    position: "absolute",
    top: 0,
    width: 132,
    zIndex: -1,
  },
  hero: {
    alignItems: "center",
    display: "grid",
    gap: "clamp(32px,4vw,64px)",
    gridTemplateColumns: "minmax(0,.9fr) minmax(0,1.1fr)",
    minHeight: 460,
    paddingBlock: "36px 48px",
    "@media (max-width: 1080px)": { gridTemplateColumns: "1fr", justifyItems: "center" },
    "@media (max-width: 600px)": { gap: 28, paddingBlock: "28px 40px" },
  },
  copy: {
    maxWidth: 580,
    "@media (max-width: 1080px)": {
      alignItems: "center",
      display: "flex",
      flexDirection: "column",
      textAlign: "center",
      width: "100%",
    },
  },
  title: {
    fontSize: "clamp(48px,6vw,92px)",
    fontWeight: 760,
    letterSpacing: "-.035em",
    lineHeight: 0.98,
    margin: 0,
    textWrap: "balance",
  },
  word: {
    animationDuration: "560ms",
    animationFillMode: "both",
    animationName: { default: rise, "@media (prefers-reduced-motion: reduce)": "none" },
    animationTimingFunction: EASE_OUT,
    display: "inline-block",
  },
  wordDelay: (ms: number) => ({ animationDelay: `${ms}ms` }),
  underlined: { display: "inline-block", position: "relative" },
  squiggle: {
    bottom: "-0.16em",
    height: "0.2em",
    left: 0,
    overflow: "visible",
    position: "absolute",
    width: "100%",
  },
  squigglePath: {
    animationDelay: "420ms",
    animationDuration: "620ms",
    animationFillMode: "both",
    animationName: { default: draw, "@media (prefers-reduced-motion: reduce)": "none" },
    animationTimingFunction: SETTLE,
    fill: "none",
    stroke: "oklch(0.72 0.17 293)",
    strokeDasharray: 1,
    strokeLinecap: "round",
    strokeWidth: 3,
    vectorEffect: "non-scaling-stroke",
  },
  enter: (ms: number) => ({
    animationDelay: `${ms}ms`,
    animationDuration: "480ms",
    animationFillMode: "both",
    animationName: { default: fadeUp, "@media (prefers-reduced-motion: reduce)": "none" },
    animationTimingFunction: EASE_OUT,
  }),
  fade: (ms: number) => ({
    animationDelay: `${ms}ms`,
    animationDuration: "400ms",
    animationFillMode: "both",
    animationName: { default: fadeOnly, "@media (prefers-reduced-motion: reduce)": "none" },
    animationTimingFunction: "ease-out",
  }),
  lede: {
    color: siteTokens.muted,
    fontSize: 18,
    lineHeight: 1.6,
    margin: "26px 0 30px",
    maxWidth: "40ch",
    textWrap: "pretty",
    "@media (max-width: 600px)": { fontSize: 16 },
  },
  visual: {
    minWidth: 0,
    position: "relative",
    width: "100%",
    "@media (max-width: 1080px)": { maxWidth: 680 },
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
    alignSelf: "start",
    backgroundColor: siteTokens.surface,
    borderRadius: 16,
    boxShadow: `0 0 0 1px ${siteTokens.border}`,
    display: "flex",
    flexDirection: "column",
    gap: 18,
    padding: 20,
  },
  agentLine: { color: siteTokens.muted, fontSize: 13, lineHeight: 1.5, margin: "-6px 0 0" },
  budgetSetting: { display: "flex", flexDirection: "column", gap: 2 },
  budgetLine: {
    color: siteTokens.muted,
    fontSize: 13,
    fontVariantNumeric: "tabular-nums",
    lineHeight: 1.5,
    margin: 0,
    minHeight: "3em",
  },
  stage: {
    borderRadius: 16,
    boxShadow: `0 0 0 1px ${siteTokens.border}`,
    display: "flex",
    flexDirection: "column",
    gap: 20,
    minWidth: 0,
    padding: "22px 22px 20px",
  },
  headline: { alignItems: "flex-end", display: "flex", flexWrap: "wrap", gap: "6px 14px" },
  bigNumber: {
    color: siteTokens.text,
    display: "inline-block",
    fontSize: "clamp(56px, 6vw, 84px)",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 720,
    letterSpacing: "-.045em",
    lineHeight: 0.9,
    transformOrigin: "left bottom",
  },
  bigLabel: {
    color: siteTokens.muted,
    fontSize: 15,
    fontWeight: 560,
    lineHeight: 1.3,
    paddingBottom: 4,
  },
  budgetTag: {
    alignItems: "center",
    alignSelf: "flex-start",
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "dashed",
    borderWidth: 1,
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
    transitionDuration: "200ms",
    transitionProperty: "background-color, opacity, box-shadow",
    transitionTimingFunction: "ease-out",
  },
  // Same values as STATE_FILL in calc-shared.tsx; StyleX needs literals here.
  described: { backgroundColor: "oklch(0.75 0.17 150)" },
  nameOnly: { backgroundColor: "oklch(0.8 0.15 80)" },
  leftOut: { backgroundColor: "transparent", boxShadow: `inset 0 0 0 1px ${siteTokens.muted}` },
  off: { backgroundColor: siteTokens.border, opacity: 0.35 },
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
  note: { color: siteTokens.muted, fontSize: 13, lineHeight: 1.5, margin: 0, maxWidth: "68ch" },
});
