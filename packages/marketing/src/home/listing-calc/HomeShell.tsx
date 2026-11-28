import type { ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";

import { AgentIcon, type AgentId } from "../../AgentIcon";
import { BenchmarkSection } from "../BenchmarkSection";
import { ClosingFinale } from "../ClosingFinale";
import { finaleCtaMarker } from "../ClosingFinale.stylex";
import { FaqSection } from "../FaqSection";
import { FirstScanSteps } from "../FirstScanSteps";
import { homeSectionStyles } from "../HomeSection.stylex";
import { JobsSection } from "../JobsSection";
import type { ThemeToggleOrigin } from "../../ProductMock";
import { SiteFooter, SiteHeader, siteLayout } from "../../SiteChrome";
import { lightSiteTheme, siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import { HERO_TITLE } from "./hero-copy";
import { DownloadButton, HeaderDownload, TrustLine } from "./HeroParts";

export interface HomePageProps {
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}

const AGENTS = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "opencode", name: "OpenCode" },
  { id: "pi", name: "pi" },
  { id: "cursor", name: "Cursor" },
  { id: "grok", name: "Grok Build" },
] satisfies ReadonlyArray<{ id: AgentId; name: string }>;

const marqueeMove = stylex.keyframes({ to: { transform: "translateX(-50%)" } });

function AgentStrip() {
  return (
    <section {...stylex.props(styles.agentProof)} aria-label="Supported agents">
      <div {...stylex.props(styles.agentProofRow)}>
        <span {...stylex.props(styles.agentProofLabel)}>Works with</span>
        <div {...stylex.props(styles.desktopAgentList)}>
          {AGENTS.map((agent) => (
            <span key={agent.id} {...stylex.props(styles.agentMark)}>
              <AgentIcon agent={agent.id} size={20} />
              {agent.name}
            </span>
          ))}
        </div>
        <div {...stylex.props(styles.marqueeViewport)}>
          <div {...stylex.props(styles.marqueeTrack)} aria-hidden="true">
            {[0, 1].map((copy) => (
              <div key={copy} {...stylex.props(styles.marqueeSet)}>
                {AGENTS.map((agent) => (
                  <span key={`${copy}-${agent.id}`} {...stylex.props(styles.agentMark)}>
                    <AgentIcon agent={agent.id} size={20} />
                    {agent.name}
                  </span>
                ))}
              </div>
            ))}
          </div>
          <span {...stylex.props(styles.visuallyHidden)}>
            Claude Code, Codex, OpenCode, pi, Cursor, and Grok Build
          </span>
        </div>
      </div>
    </section>
  );
}

export function HomeShell({
  theme,
  hero,
  calculator,
  product,
}: HomePageProps & {
  hero: ReactNode;
  calculator: ReactNode;
  product: ReactNode;
}) {
  return (
    <div id="top" {...stylex.props(siteLayout.page, theme === "light" && lightSiteTheme)}>
      <SiteHeader page="home" sticky cta={<HeaderDownload />} />
      <main id="main">
        {hero}

        {product}

        <section
          id="listing"
          {...stylex.props(siteLayout.container, styles.listing, styles.anchor)}
        >
          {calculator}
        </section>

        <AgentStrip />

        <section
          id="jobs"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <JobsSection />
        </section>

        <section
          id="how-it-works"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <FirstScanSteps />
        </section>

        <section
          id="benchmark"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <BenchmarkSection />
        </section>

        <section
          id="faq"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <FaqSection />
        </section>
      </main>

      <ClosingFinale theme={theme}>
        <section
          id="download"
          aria-labelledby="download-title"
          {...stylex.props(siteLayout.container, styles.closingSection, styles.anchor)}
        >
          <img
            src="/skill-studio-logo-512.webp"
            alt=""
            width={96}
            height={96}
            {...stylex.props(styles.closingMascot)}
          />
          <h2 id="download-title" {...stylex.props(styles.closingTitle)}>
            {HERO_TITLE}
          </h2>
          <p {...stylex.props(styles.closingCopy)}>
            See what each session listed and used. Park the skills that never run. Undo any time.
          </p>
          <DownloadButton place="finale" />
          <TrustLine />
        </section>
        <SiteFooter page="home" />
      </ClosingFinale>
    </div>
  );
}

const styles = stylex.create({
  anchor: { scrollMarginTop: 72 },
  listing: {
    paddingBlock: "64px 96px",
    "@media (max-width: 600px)": { paddingBlock: "44px 72px" },
  },
  agentProof: {
    alignItems: "center",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    color: siteTokens.muted,
    display: "flex",
    flexDirection: "column",
    fontSize: 12,
    justifyContent: "center",
    overflow: "hidden",
    padding: "22px 28px",
    "@media (max-width: 600px)": { paddingInline: 20 },
  },
  agentProofRow: {
    alignItems: "center",
    display: "flex",
    gap: 28,
    justifyContent: "center",
    maxWidth: "100%",
    minWidth: 0,
    width: { default: "auto", "@media (max-width: 860px)": "100%" },
    "@media (max-width: 600px)": { gap: 18 },
  },
  agentProofLabel: { color: siteTokens.muted, flexShrink: 0, fontSize: 12, whiteSpace: "nowrap" },
  desktopAgentList: {
    alignItems: "center",
    display: "flex",
    gap: 34,
    justifyContent: "center",
    "@media (max-width: 860px)": { display: "none" },
  },
  marqueeViewport: {
    display: "none",
    maskImage: "linear-gradient(90deg, transparent, black 5%, black 95%, transparent)",
    minWidth: 0,
    overflow: "hidden",
    position: "relative",
    width: "100%",
    "@media (max-width: 860px)": { display: "block" },
  },
  marqueeTrack: {
    alignItems: "center",
    animationDuration: "20s",
    animationIterationCount: "infinite",
    animationName: marqueeMove,
    animationTimingFunction: "linear",
    display: "flex",
    width: "max-content",
    ":hover": { animationPlayState: "paused" },
    "@media (prefers-reduced-motion: reduce)": { animationName: "none" },
  },
  marqueeSet: { alignItems: "center", display: "flex", gap: 40, paddingRight: 40 },
  agentMark: {
    alignItems: "center",
    color: siteTokens.text,
    display: "flex",
    fontSize: 13,
    fontWeight: 560,
    gap: 9,
    whiteSpace: "nowrap",
  },
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
  closingSection: {
    alignItems: "center",
    display: "flex",
    flexDirection: "column",
    paddingBlock: "124px 96px",
    textAlign: "center",
    "@media (max-width: 600px)": { paddingBlock: "86px 64px" },
  },
  closingMascot: {
    filter: "drop-shadow(0 18px 32px oklch(0.45 0.2 293 / .4))",
    height: "auto",
    marginBottom: 28,
    transform: {
      default: "none",
      "@media (hover: hover) and (pointer: fine)": {
        [stylex.when.anySibling(":hover", finaleCtaMarker)]: "translateY(-6px) rotate(-6deg)",
      },
    },
    transition: {
      default: "transform 300ms cubic-bezier(0.34, 1.56, 0.64, 1)",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    width: 96,
    "@media (max-width: 600px)": { width: 80 },
  },
  closingTitle: {
    fontSize: "clamp(42px,5vw,68px)",
    letterSpacing: "-.035em",
    lineHeight: 0.98,
    margin: 0,
    maxWidth: "14ch",
    textWrap: "balance",
  },
  closingCopy: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.6,
    margin: "24px 0 30px",
    maxWidth: "48ch",
    textWrap: "balance",
  },
});
