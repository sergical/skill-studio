// PROTO(home-calc) variant "Baseline Plus": today's structure and craft with the content
// brief applied. Motion added: the dither glows toward the hovered download button, and the
// dither layer drifts a little with the pointer through two CSS variables. No follower.
import { useRef } from "react";
import * as stylex from "@stylexjs/stylex";

import { homeSectionStyles } from "../../home/HomeSection.stylex";
import { siteLayout } from "../../SiteChrome";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import {
  CLAUDE_LINE,
  CODEX_LINE_COPY,
  ESTIMATE_NOTE,
  claudeFollowUp,
  claudeSentence,
  codexFollowUp,
  codexSentence,
  format,
} from "./budget";
import {
  AGENT_OPTIONS,
  ProjectSkillsControl,
  STATE_FILL,
  SkillsControl,
  WindowControl,
  useCalculator,
  type Agent,
} from "./calc-shared";
import { Segmented } from "./controls";
import {
  CopyAgentPromptButton,
  DownloadButton,
  HERO_SUBLINE,
  HERO_TITLE,
  HeroDither,
  TrustLine,
  heroGlow,
  heroPartStyles,
} from "./HeroParts";
import { SkillThrow } from "./pile/SkillThrow";
import { ProtoPage, type VariantProps } from "./ProtoPage";
import { useHeroPointer } from "./useHeroPointer";

const STUDY_URL = "https://arxiv.org/abs/2610.18522";

function Hero({ theme }: { theme: SiteTheme }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  useHeroPointer({ root: rootRef, vars: layerRef });
  return (
    <div ref={rootRef} {...stylex.props(styles.heroWrap, heroGlow.baseline)}>
      <HeroDither theme={theme} layerRef={layerRef} />
      <section {...stylex.props(siteLayout.container, styles.hero)}>
        <div {...stylex.props(styles.copy)}>
          <h1 {...stylex.props(styles.title)}>{HERO_TITLE}</h1>
          <p {...stylex.props(styles.lede)}>{HERO_SUBLINE}</p>
          <div {...stylex.props(heroPartStyles.actions)}>
            <DownloadButton theme={theme} place="hero" />
            <CopyAgentPromptButton />
          </div>
          <TrustLine />
        </div>
        <div {...stylex.props(styles.visual)}>
          <SkillThrow />
        </div>
      </section>
    </div>
  );
}

interface Tier {
  key: string;
  label: string;
  count: number;
  fill?: string;
}

function tiersFor(agent: Agent, calc: ReturnType<typeof useCalculator>): Tier[] {
  if (agent === "claude") {
    return [
      {
        key: "described",
        label: "Described",
        count: calc.claude.described,
        fill: STATE_FILL.described,
      },
      {
        key: "name-only",
        label: "Name only",
        count: calc.claude.nameOnly,
        fill: STATE_FILL.nameOnly,
      },
    ];
  }
  return [
    { key: "full", label: "Full description", count: calc.codex.full, fill: STATE_FILL.described },
    {
      key: "shortened",
      label: "Shortened",
      count: calc.codex.shortened,
      fill: STATE_FILL.nameOnly,
    },
    { key: "left-out", label: "Left out", count: calc.codex.leftOut },
  ];
}

function Calculator() {
  const calc = useCalculator();
  const isClaude = calc.agent === "claude";
  const tiers = tiersFor(calc.agent, calc).filter((tier) => tier.count > 0);
  const budget = isClaude
    ? `${format(calc.claude.budget)} characters`
    : `${format(calc.codex.budget)} tokens`;
  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.calcTitle)}>
        How many skills keep their description?
      </h2>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div {...stylex.props(styles.intro)}>
          <p {...stylex.props(styles.lead)}>
            Each agent gives its model a list of skills. Claude Code and Codex limit the size of the
            list. When your skills do not fit, descriptions are shortened or removed first.
          </p>
          <p {...stylex.props(styles.note)}>
            A long list also hurts. In the study{" "}
            <a href={STUDY_URL} target="_blank" rel="noreferrer" {...stylex.props(styles.link)}>
              Lost in Implicit Routing
            </a>
            , agents found the right skill in 73.6% of runs. With 1,000 unrelated skills installed,
            they found it in 27.4%.
          </p>
        </div>
        <figure {...stylex.props(styles.figure)}>
          <div {...stylex.props(styles.controls)}>
            <div {...stylex.props(styles.controlRow)}>
              <div {...stylex.props(styles.agentField)}>
                <span {...stylex.props(styles.fieldLabel)}>Agent</span>
                <Segmented
                  label="Agent"
                  options={AGENT_OPTIONS}
                  value={calc.agent}
                  onChange={calc.setAgent}
                />
              </div>
              <WindowControl calc={calc} />
            </div>
            <div {...stylex.props(styles.sliderRow)}>
              <SkillsControl calc={calc} />
              <ProjectSkillsControl calc={calc} />
            </div>
          </div>
          <figcaption {...stylex.props(styles.caption)}>
            {isClaude ? CLAUDE_LINE : CODEX_LINE_COPY} At {calc.contextWindow.label}, that is{" "}
            {budget}.
          </figcaption>
          {/* The counts are read in the sentence below, so the bar is for the eye only. */}
          <div aria-hidden="true" {...stylex.props(styles.bar)}>
            {tiers.map((tier) => (
              <span
                key={tier.key}
                {...stylex.props(styles.segment, styles.segmentSize(tier.count))}
              >
                <span {...stylex.props(styles.count)}>{format(tier.count)}</span>
                <span
                  {...stylex.props(
                    styles.fill,
                    tier.fill ? styles.fillColor(tier.fill) : styles.fillOutline,
                  )}
                />
                <span {...stylex.props(styles.segmentLabel)}>{tier.label}</span>
              </span>
            ))}
          </div>
          <p role="status" {...stylex.props(styles.result)}>
            <span {...stylex.props(styles.resultLead)}>
              {isClaude ? claudeSentence(calc.claude) : codexSentence(calc.codex)}
            </span>{" "}
            <span {...stylex.props(styles.resultFollow)}>
              {isClaude ? claudeFollowUp(calc.claude) : codexFollowUp(calc.codex)}
            </span>
          </p>
          <p {...stylex.props(styles.proof)}>{ESTIMATE_NOTE}</p>
        </figure>
      </div>
    </>
  );
}

export function BaselinePlus(props: VariantProps) {
  return <ProtoPage {...props} hero={<Hero theme={props.theme} />} calculator={<Calculator />} />;
}

const STACKED = "@media (max-width: 760px)";

const styles = stylex.create({
  heroWrap: { isolation: "isolate", position: "relative" },
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
    maxWidth: 560,
    "@media (max-width: 1080px)": {
      alignItems: "center",
      display: "flex",
      flexDirection: "column",
      textAlign: "center",
      width: "100%",
    },
  },
  title: {
    fontSize: "clamp(44px,5.4vw,80px)",
    fontWeight: 700,
    letterSpacing: "-.025em",
    lineHeight: 1,
    margin: 0,
    textWrap: "balance",
  },
  lede: {
    color: siteTokens.muted,
    fontSize: 18,
    lineHeight: 1.6,
    margin: "24px 0 30px",
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
  calcTitle: { marginBottom: 36 },
  intro: { display: "flex", flexDirection: "column", gap: 16, [STACKED]: { marginBottom: 28 } },
  lead: { color: siteTokens.text, fontSize: 17, lineHeight: 1.55, margin: 0, textWrap: "pretty" },
  note: { color: siteTokens.muted, fontSize: 14, lineHeight: 1.55, margin: 0, textWrap: "pretty" },
  link: {
    color: siteTokens.text,
    textDecorationColor: siteTokens.muted,
    textDecorationLine: "underline",
    textDecorationSkipInk: "auto",
    textDecorationThickness: "0.09em",
    textUnderlineOffset: "0.2em",
    transition: "text-decoration-color 150ms ease-out",
    ":hover": { textDecorationColor: "currentColor" },
  },
  figure: { margin: 0, minWidth: 0 },
  controls: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 18,
    marginBottom: 22,
    paddingBottom: 22,
  },
  controlRow: { display: "flex", flexWrap: "wrap", gap: "16px 28px" },
  agentField: { display: "flex", flexDirection: "column", gap: 8 },
  fieldLabel: { color: siteTokens.text, fontSize: 14, fontWeight: 600 },
  sliderRow: {
    alignItems: "start",
    display: "grid",
    gap: "8px 28px",
    gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)",
    [STACKED]: { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  caption: { color: siteTokens.muted, fontSize: 13, lineHeight: 1.5, marginBottom: 14 },
  bar: { display: "flex", gap: 3 },
  segment: { display: "flex", flexDirection: "column", gap: 10, minWidth: 64 },
  segmentSize: (count: number) => ({ flexBasis: 0, flexGrow: count }),
  count: {
    color: siteTokens.text,
    fontSize: 44,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 640,
    letterSpacing: "-.03em",
    lineHeight: 1.05,
    [STACKED]: { fontSize: 32 },
  },
  fill: { borderRadius: 3, display: "block", height: 14 },
  fillColor: (color: string) => ({ backgroundColor: color }),
  fillOutline: {
    borderColor: siteTokens.muted,
    borderStyle: "dashed",
    borderWidth: 1,
  },
  segmentLabel: { color: siteTokens.muted, fontSize: 13, whiteSpace: "nowrap" },
  // Room for three lines, so a longer sentence does not push the note down.
  result: {
    minHeight: "3.6em",
    color: siteTokens.text,
    fontSize: "clamp(24px, 2.4vw, 32px)",
    fontWeight: 640,
    letterSpacing: "-.025em",
    lineHeight: 1.2,
    margin: "26px 0 0",
    maxWidth: "30ch",
    textWrap: "balance",
  },
  resultLead: { color: siteTokens.text },
  resultFollow: { color: siteTokens.muted, fontWeight: 560 },
  proof: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.55,
    margin: "22px 0 0",
    maxWidth: "68ch",
    paddingTop: 14,
    textWrap: "pretty",
  },
});
