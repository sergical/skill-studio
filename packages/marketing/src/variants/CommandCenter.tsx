import * as stylex from "@stylexjs/stylex";
import { AGENT_SETUP_PROMPT } from "@skill-studio/lib/agent-setup";

import { productDemoLightTheme } from "../ProductDemoTokens.stylex";
import type { ThemeToggleOrigin } from "../ProductMock";
import { SiteFooter, SiteHeader, siteLayout } from "../SiteChrome";
import { lightSiteTheme, siteTokens, type SiteTheme } from "../SiteTheme.stylex";
import { FaqSection } from "../home/FaqSection";
import { homeSectionStyles } from "../home/HomeSection.stylex";
import { CleanupOverview } from "../home/CleanupOverview";
import { RepairProof } from "../home/RepairProof";
import { SkillThrow } from "../home/SkillThrow";
import { TrustSetupSection } from "../home/TrustSetupSection";
import { useCopyFlash } from "../home/useCopyFlash";
import { WarpDither } from "../home/WarpDither";
import { DOWNLOAD_URL } from "../site-links";

interface CommandCenterProps {
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}

function HeroActions() {
  const { copied, copy } = useCopyFlash();
  return (
    <>
      <div {...stylex.props(styles.actions)}>
        <a href={DOWNLOAD_URL} {...stylex.props(styles.action, styles.download)}>
          Download for macOS
        </a>
        <button
          type="button"
          onClick={() => copy(AGENT_SETUP_PROMPT)}
          {...stylex.props(styles.action, styles.agentSetup)}
        >
          <span aria-live="polite">
            {copied ? "Copied. Paste into your agent" : "Set up with your agent"}
          </span>
        </button>
      </div>
      <p {...stylex.props(styles.facts)}>
        macOS 13+ · Free · Open source (MIT) · No account to scan
      </p>
    </>
  );
}

/** The homepage leads from what Skill Studio does to one repair example and setup. */
export function CommandCenter({ theme, onToggleTheme }: CommandCenterProps) {
  return (
    <div id="top" {...stylex.props(siteLayout.page, theme === "light" && lightSiteTheme)}>
      <SiteHeader page="home" sticky showThemeToggle onToggleTheme={onToggleTheme} />
      <main>
        <div {...stylex.props(styles.heroWrap)}>
          <div aria-hidden="true" {...stylex.props(styles.dither)}>
            <WarpDither colorBack="#00000000" colorFront="#5a45c4" />
          </div>
          <section
            id="download"
            {...stylex.props(siteLayout.container, styles.hero, styles.anchor)}
          >
            <div {...stylex.props(styles.copy)}>
              <h1 {...stylex.props(styles.title)}>Fix your skill issues</h1>
              <p {...stylex.props(styles.lede)}>
                Find, fix, and organize agent skills across Claude Code, Codex, Cursor, and more.
              </p>
              <HeroActions />
            </div>
            <div {...stylex.props(styles.visual, theme === "light" && productDemoLightTheme)}>
              <SkillThrow />
            </div>
          </section>
        </div>
        <section
          id="jobs"
          aria-label="Find and clean up your skills"
          {...stylex.props(siteLayout.container, styles.jobs, styles.anchor)}
        >
          <CleanupOverview theme={theme} />
        </section>
        <section
          id="how-it-works"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <RepairProof theme={theme} />
        </section>
        <section
          id="use-it"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <TrustSetupSection />
        </section>
        <section
          id="faq"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <FaqSection />
        </section>
      </main>
      <SiteFooter page="home" />
    </div>
  );
}

const styles = stylex.create({
  anchor: { scrollMarginTop: 72 },
  heroWrap: { isolation: "isolate", position: "relative" },
  dither: {
    height: 640,
    insetInline: 0,
    maskImage: "radial-gradient(ellipse 70% 80% at 70% 0%, black 10%, transparent 72%)",
    opacity: 0.45,
    pointerEvents: "none",
    position: "absolute",
    top: -64,
    zIndex: -1,
  },
  hero: {
    alignItems: "center",
    display: "grid",
    gap: "clamp(32px,4vw,64px)",
    minHeight: 460,
    gridTemplateColumns: "minmax(0,.9fr) minmax(0,1.1fr)",
    paddingBlock: "36px 48px",
    "@media (max-width: 1080px)": { gridTemplateColumns: "1fr", justifyItems: "center" },
    "@media (max-width: 600px)": { gap: 28, paddingBlock: "28px 40px" },
  },
  copy: {
    maxWidth: 560,
    "@media (max-width: 1080px)": { textAlign: "center", width: "100%" },
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
    maxWidth: "46ch",
    textWrap: "pretty",
    "@media (max-width: 1080px)": { marginInline: "auto" },
    "@media (max-width: 600px)": { fontSize: 16 },
  },
  actions: {
    display: "flex",
    gap: 10,
    "@media (max-width: 1080px)": { justifyContent: "center" },
    "@media (max-width: 600px)": {
      display: "grid",
      gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    },
  },
  action: {
    alignItems: "center",
    border: `1px solid ${siteTokens.border}`,
    borderRadius: 10,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 14,
    fontWeight: 640,
    justifyContent: "center",
    minHeight: 48,
    padding: "8px 15px",
    textAlign: "center",
    textDecoration: "none",
    "@media (max-width: 600px)": { paddingInline: 8 },
  },
  download: {
    backgroundColor: { default: siteTokens.accent, ":hover": siteTokens.accentHover },
    color: siteTokens.accentText,
  },
  agentSetup: {
    backgroundColor: { default: "transparent", ":hover": siteTokens.surface },
    color: siteTokens.text,
  },
  facts: { color: siteTokens.muted, fontSize: 12, lineHeight: 1.5, margin: "12px 0 0" },
  visual: {
    minWidth: 0,
    position: "relative",
    width: "100%",
    "@media (max-width: 1080px)": { maxWidth: 680 },
  },
  jobs: { paddingTop: 12 },
});
