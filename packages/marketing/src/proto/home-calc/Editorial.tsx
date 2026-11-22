// PROTO(home-calc) variant "Editorial": type carries the page. A display headline across the
// container, the pile beside the copy below it, and one motion only: the dither glows toward
// the hovered download button. The calculator is a dense ledger of both agents at once,
// summed up in one sentence set large.
import * as stylex from "@stylexjs/stylex";

import { AgentIcon } from "../../AgentIcon";
import { homeSectionStyles } from "../../home/HomeSection.stylex";
import { siteLayout } from "../../SiteChrome";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import {
  CLAUDE_LINE,
  CODEX_LINE_COPY,
  ESTIMATE_NOTE,
  format,
  type ClaudeEstimate,
  type CodexEstimate,
} from "./budget";
import {
  ProjectSkillsControl,
  STATE_FILL,
  SkillsControl,
  WindowControl,
  useCalculator,
} from "./calc-shared";
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

function Hero({ theme }: { theme: SiteTheme }) {
  return (
    <div {...stylex.props(styles.heroWrap, heroGlow.editorial)}>
      <HeroDither theme={theme} />
      <section {...stylex.props(siteLayout.container, styles.hero)}>
        <h1 {...stylex.props(styles.title)}>
          Fix your <span {...stylex.props(styles.titleBreak)}>skill issues.</span>
        </h1>
        <div {...stylex.props(styles.row)}>
          <div {...stylex.props(styles.copy)}>
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
        </div>
      </section>
    </div>
  );
}

function Meter({ parts }: { parts: Array<{ count: number; fill?: string }> }) {
  return (
    <div aria-hidden="true" {...stylex.props(styles.meter)}>
      {parts
        .filter((part) => part.count > 0)
        .map((part, index) => (
          <span
            key={index}
            {...stylex.props(
              styles.meterPart,
              styles.grow(part.count),
              part.fill ? styles.fill(part.fill) : styles.outline,
            )}
          />
        ))}
    </div>
  );
}

function Row({ label, value, fill }: { label: string; value: number; fill?: string }) {
  return (
    <div {...stylex.props(styles.row2)}>
      <dt {...stylex.props(styles.term)}>
        <span
          aria-hidden="true"
          {...stylex.props(styles.key, fill ? styles.fill(fill) : styles.outline)}
        />
        {label}
      </dt>
      <dd {...stylex.props(styles.value)}>{format(value)}</dd>
    </div>
  );
}

function ClaudeColumn({ result }: { result: ClaudeEstimate }) {
  return (
    <div {...stylex.props(styles.column)}>
      <h3 {...stylex.props(styles.agent)}>
        <AgentIcon agent="claude" size={18} />
        Claude Code
      </h3>
      <p {...stylex.props(styles.agentLine)}>{CLAUDE_LINE}</p>
      <p {...stylex.props(styles.budget)}>
        <span {...stylex.props(styles.budgetFigure)}>{format(result.budget)}</span> characters for{" "}
        {format(result.needed)} needed
      </p>
      <Meter
        parts={[
          { count: result.described, fill: STATE_FILL.described },
          { count: result.nameOnly, fill: STATE_FILL.nameOnly },
        ]}
      />
      <dl {...stylex.props(styles.rows)}>
        <Row label="Described" value={result.described} fill={STATE_FILL.described} />
        <Row label="Name only" value={result.nameOnly} fill={STATE_FILL.nameOnly} />
      </dl>
    </div>
  );
}

function CodexColumn({ result }: { result: CodexEstimate }) {
  return (
    <div {...stylex.props(styles.column, styles.columnSecond)}>
      <h3 {...stylex.props(styles.agent)}>
        <AgentIcon agent="codex" size={18} />
        Codex
      </h3>
      <p {...stylex.props(styles.agentLine)}>{CODEX_LINE_COPY}</p>
      <p {...stylex.props(styles.budget)}>
        <span {...stylex.props(styles.budgetFigure)}>{format(result.budget)}</span> tokens for{" "}
        {format(result.needed)} needed
      </p>
      <Meter
        parts={[
          { count: result.full, fill: STATE_FILL.described },
          { count: result.shortened, fill: STATE_FILL.nameOnly },
          { count: result.leftOut },
        ]}
      />
      <dl {...stylex.props(styles.rows)}>
        <Row label="Full description" value={result.full} fill={STATE_FILL.described} />
        <Row label="Shortened" value={result.shortened} fill={STATE_FILL.nameOnly} />
        <Row label="Left out" value={result.leftOut} />
      </dl>
    </div>
  );
}

function Num({ children }: { children: number }) {
  return <strong {...stylex.props(styles.num)}>{format(children)}</strong>;
}

function Summary({
  contextLabel,
  claude,
  codex,
}: {
  contextLabel: string;
  claude: ClaudeEstimate;
  codex: CodexEstimate;
}) {
  const claudePart =
    claude.nameOnly === 0 ? (
      <>
        Claude Code describes all <Num>{claude.total}</Num> skills
      </>
    ) : (
      <>
        Claude Code describes <Num>{claude.described}</Num> of <Num>{claude.total}</Num> skills
      </>
    );
  const codexPart =
    codex.full === codex.total ? (
      <>Codex fits every description</>
    ) : codex.shortened > 0 ? (
      <>
        Codex cuts every description to <Num>{codex.kept}</Num> characters
      </>
    ) : (
      <>
        Codex leaves <Num>{codex.leftOut}</Num> skills out
      </>
    );
  return (
    <p role="status" {...stylex.props(styles.summary)}>
      <span {...stylex.props(styles.summaryWindow)}>At {contextLabel},</span> {claudePart}.{" "}
      {codexPart}.
    </p>
  );
}

function Calculator() {
  const calc = useCalculator();
  return (
    <>
      <div {...stylex.props(styles.calcHead)}>
        <h2 {...stylex.props(homeSectionStyles.title, styles.calcTitle)}>
          How much of your list does the model read?
        </h2>
        <p {...stylex.props(styles.calcLede)}>
          Claude Code and Codex fit your skills into a fixed share of the context window. When they
          do not fit, descriptions go first.
        </p>
      </div>
      <div {...stylex.props(styles.controls)}>
        <WindowControl calc={calc} size="compact" />
        <SkillsControl calc={calc} />
        <ProjectSkillsControl calc={calc} />
      </div>
      <div {...stylex.props(styles.ledger)}>
        <ClaudeColumn result={calc.claude} />
        <CodexColumn result={calc.codex} />
      </div>
      <Summary contextLabel={calc.contextWindow.label} claude={calc.claude} codex={calc.codex} />
      <p {...stylex.props(styles.note)}>{ESTIMATE_NOTE}</p>
    </>
  );
}

export function Editorial(props: VariantProps) {
  return (
    <ProtoPage {...props} display hero={<Hero theme={props.theme} />} calculator={<Calculator />} />
  );
}

const STACKED = "@media (max-width: 760px)";

const styles = stylex.create({
  heroWrap: { isolation: "isolate", position: "relative" },
  hero: {
    paddingBlock: "44px 48px",
    "@media (max-width: 600px)": { paddingBlock: "28px 40px" },
  },
  title: {
    fontSize: "clamp(54px, 10.4vw, 156px)",
    fontWeight: 700,
    letterSpacing: "-.055em",
    lineHeight: 0.88,
    margin: "0 0 clamp(28px, 3vw, 44px)",
    "@media (max-width: 1080px)": { textAlign: "center" },
  },
  titleBreak: { display: "block" },
  row: {
    alignItems: "start",
    display: "grid",
    gap: "clamp(32px,4vw,64px)",
    gridTemplateColumns: "minmax(0,.8fr) minmax(0,1.2fr)",
    "@media (max-width: 1080px)": { gridTemplateColumns: "1fr", justifyItems: "center" },
  },
  copy: {
    maxWidth: 520,
    paddingTop: 8,
    "@media (max-width: 1080px)": {
      alignItems: "center",
      display: "flex",
      flexDirection: "column",
      textAlign: "center",
      width: "100%",
    },
  },
  lede: {
    color: siteTokens.text,
    fontSize: "clamp(19px, 1.6vw, 23px)",
    fontWeight: 520,
    letterSpacing: "-.012em",
    lineHeight: 1.4,
    margin: "0 0 30px",
    maxWidth: "30ch",
    textWrap: "pretty",
  },
  visual: {
    minWidth: 0,
    position: "relative",
    width: "100%",
    "@media (max-width: 1080px)": { maxWidth: 680 },
  },
  calcHead: { marginBottom: 32, maxWidth: 820 },
  calcTitle: {
    fontSize: "clamp(40px, 4.4vw, 60px)",
    letterSpacing: "-.045em",
    lineHeight: 1,
    marginBottom: 18,
  },
  calcLede: {
    color: siteTokens.muted,
    fontSize: 18,
    lineHeight: 1.55,
    margin: 0,
    maxWidth: "56ch",
    textWrap: "pretty",
  },
  controls: {
    alignItems: "end",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: "16px 32px",
    gridTemplateColumns: "auto minmax(0, 1fr) minmax(0, 1fr)",
    paddingBlock: 18,
    "@media (max-width: 1000px)": { gridTemplateColumns: "minmax(0, 1fr)", alignItems: "start" },
  },
  ledger: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    [STACKED]: { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  column: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 12,
    padding: "22px 28px 22px 0",
  },
  columnSecond: {
    borderLeftColor: { default: siteTokens.border, [STACKED]: "transparent" },
    borderLeftStyle: "solid",
    borderLeftWidth: 1,
    paddingLeft: { default: 28, [STACKED]: 0 },
  },
  agent: {
    alignItems: "center",
    color: siteTokens.text,
    display: "flex",
    fontSize: 17,
    fontWeight: 650,
    gap: 9,
    letterSpacing: "-.01em",
    margin: 0,
  },
  agentLine: { color: siteTokens.muted, fontSize: 14, lineHeight: 1.5, margin: 0 },
  budget: { color: siteTokens.muted, fontSize: 13, fontVariantNumeric: "tabular-nums", margin: 0 },
  budgetFigure: { color: siteTokens.text, fontWeight: 650 },
  meter: { display: "flex", gap: 2, height: 8 },
  meterPart: { borderRadius: 2, minWidth: 3 },
  grow: (count: number) => ({ flexBasis: 0, flexGrow: count }),
  fill: (color: string) => ({ backgroundColor: color }),
  outline: { boxShadow: `inset 0 0 0 1px ${siteTokens.muted}` },
  rows: { display: "flex", flexDirection: "column", margin: 0 },
  row2: {
    alignItems: "baseline",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    justifyContent: "space-between",
    paddingBlock: 8,
  },
  term: { alignItems: "center", color: siteTokens.text, display: "flex", fontSize: 14, gap: 8 },
  key: { borderRadius: 2, display: "inline-block", height: 8, width: 8 },
  value: {
    color: siteTokens.text,
    fontSize: 14,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 650,
    margin: 0,
  },
  // Room for four lines on a wide screen, so the note below stays put as the numbers change.
  summary: {
    minHeight: { default: "4.4em", "@media (max-width: 760px)": 0 },
    color: siteTokens.muted,
    fontSize: "clamp(28px, 3.6vw, 50px)",
    fontWeight: 600,
    letterSpacing: "-.035em",
    lineHeight: 1.08,
    margin: "36px 0 0",
    maxWidth: "24ch",
    textWrap: "balance",
  },
  summaryWindow: { color: siteTokens.muted },
  num: { color: siteTokens.text, fontVariantNumeric: "tabular-nums", fontWeight: 700 },
  note: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.5,
    margin: "22px 0 0",
    maxWidth: "68ch",
  },
});
