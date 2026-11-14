import * as stylex from "@stylexjs/stylex";
import { Dithering } from "@paper-design/shaders-react";
import { Check, Copy, Download } from "lucide-react";
import { AGENT_SETUP_PROMPT } from "@skill-studio/lib/agent-setup";

import { AgentIcon, type AgentId } from "../AgentIcon";
import { ProductMock, type ThemeToggleOrigin } from "../ProductMock";
import { Arrow } from "../MarketingBrand";
import { SiteFooter, SiteHeader, siteLayout } from "../SiteChrome";
import { lightSiteTheme, siteTokens, type SiteTheme } from "../SiteTheme.stylex";
import { useReducedMotion } from "../use-reduced-motion";
import { SidecarWalkthrough } from "../walkthrough/SidecarWalkthrough";
import { ClosingFinale, DITHER_COLORS } from "../home/ClosingFinale";
import { finaleCtaMarker } from "../home/ClosingFinale.stylex";
import { FaqSection } from "../home/FaqSection";
import { homeSectionStyles } from "../home/HomeSection.stylex";
import { FirstScanSteps } from "../home/FirstScanSteps";
import { SkillThrow } from "../home/SkillThrow";
import { UsageSection } from "../home/UsageSection";
import { useCopyFlash } from "../home/useCopyFlash";
import {
  CLI_DOCS_URL,
  DOWNLOAD_INTEL_URL,
  DOWNLOAD_URL,
  MCP_DOCS_URL,
  TRUST_LINE,
} from "../site-links";

interface CommandCenterProps {
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}

const supportedAgents = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "opencode", name: "OpenCode" },
  { id: "pi", name: "pi" },
  { id: "cursor", name: "Cursor" },
  { id: "grok", name: "Grok Build" },
] satisfies ReadonlyArray<{ id: AgentId; name: string }>;

const marqueeMove = stylex.keyframes({
  to: { transform: "translateX(-50%)" },
});

// Phones and tablets cannot run the macOS installer, so they get the CLI and MCP console instead.
const TOUCH_DEVICE = "@media (hover: none) and (pointer: coarse)";

function DownloadButton({ theme, inFinale = false }: { theme: SiteTheme; inFinale?: boolean }) {
  return (
    <>
      <a
        href={DOWNLOAD_URL}
        {...stylex.props(inFinale && finaleCtaMarker, styles.primaryButton, styles.pointerOnly)}
      >
        <span>Download for macOS</span>
        <span aria-hidden="true" {...stylex.props(styles.primaryButtonArrow)}>
          <Arrow inverse={theme === "dark"} />
        </span>
      </a>
      <a
        href="#use-it"
        {...stylex.props(inFinale && finaleCtaMarker, styles.primaryButton, styles.touchOnly)}
      >
        <span>Get Skill Studio</span>
        <span
          aria-hidden="true"
          {...stylex.props(styles.primaryButtonArrow, inFinale ? styles.arrowUp : styles.arrowDown)}
        >
          <Arrow inverse={theme === "dark"} />
        </span>
      </a>
    </>
  );
}

function TrustLine() {
  return (
    <>
      <p {...stylex.props(styles.fineprint, styles.pointerOnlyBlock)}>
        Also as a{" "}
        <a href={CLI_DOCS_URL} {...stylex.props(homeSectionStyles.textLink, styles.inlineLink)}>
          CLI
        </a>{" "}
        ·{" "}
        <a href={MCP_DOCS_URL} {...stylex.props(homeSectionStyles.textLink, styles.inlineLink)}>
          MCP server
        </a>
        <br />
        {TRUST_LINE} ·{" "}
        <a
          href={DOWNLOAD_INTEL_URL}
          {...stylex.props(homeSectionStyles.textLink, styles.inlineLink)}
        >
          Intel build
        </a>
      </p>
      <p {...stylex.props(styles.fineprint, styles.touchOnlyBlock)}>
        Mac app, CLI and MCP server. Free and open source.
        <br />
        Download the Mac app on your computer.
      </p>
    </>
  );
}

function HeaderDownload() {
  return (
    <>
      <a href={DOWNLOAD_URL} {...stylex.props(styles.headerCta, styles.pointerOnly)}>
        <Download aria-hidden="true" size={15} />
        Download
      </a>
      <a href="#use-it" {...stylex.props(styles.headerCta, styles.touchOnly)}>
        Get it
      </a>
    </>
  );
}

function TopDither({ theme }: { theme: SiteTheme }) {
  const reducedMotion = useReducedMotion();
  return (
    <div aria-hidden="true" {...stylex.props(styles.topDither)}>
      <Dithering
        width="100%"
        height="100%"
        colorBack="#00000000"
        colorFront={DITHER_COLORS[theme].front}
        // oxlint-disable-next-line anti-slop/no-shape-in-symbol-names -- Paper Shaders' own prop name
        shape="warp"
        type="4x4"
        size={3}
        scale={0.8}
        speed={reducedMotion ? 0 : 0.35}
      />
    </div>
  );
}

// Copies the prompt that has the visitor's own agent add the MCP server and check their skills.
function CopyAgentPromptButton() {
  const { copied, copy } = useCopyFlash();
  return (
    <button
      type="button"
      onClick={() => copy(AGENT_SETUP_PROMPT)}
      {...stylex.props(styles.sourceLink)}
    >
      {copied ? <Check aria-hidden="true" size={18} /> : <Copy aria-hidden="true" size={18} />}
      <span aria-live="polite">
        {copied ? "Copied. Paste it into your agent" : "Copy agent prompt"}
      </span>
    </button>
  );
}

export function CommandCenter({ theme, onToggleTheme }: CommandCenterProps) {
  return (
    <div id="top" {...stylex.props(siteLayout.page, theme === "light" && lightSiteTheme)}>
      <SiteHeader page="home" sticky cta={<HeaderDownload />} />

      <main>
        <div {...stylex.props(styles.heroWrap)}>
          <TopDither theme={theme} />
          <section {...stylex.props(siteLayout.container, styles.hero)}>
            <div {...stylex.props(styles.copy)}>
              <h1 {...stylex.props(styles.title)}>Find every skill your agents load.</h1>
              <p {...stylex.props(styles.lede)}>
                Skills pile up in your home folder, in every project and inside plugins. See them in
                one list, fix the broken ones and park the rest, with undo.
              </p>
              <div {...stylex.props(styles.actions)}>
                <DownloadButton theme={theme} />
                <CopyAgentPromptButton />
              </div>
              <TrustLine />
            </div>
            <div {...stylex.props(styles.visual)}>
              <SkillThrow />
            </div>
          </section>
        </div>

        <section
          id="product"
          {...stylex.props(siteLayout.container, styles.product, styles.anchor)}
        >
          <ProductMock theme={theme} onToggleTheme={onToggleTheme} />
        </section>

        <section {...stylex.props(styles.agentProof)} aria-label="Supported agents">
          <span {...stylex.props(styles.agentProofLabel)}>Works with</span>
          <div {...stylex.props(styles.desktopAgentList)}>
            {supportedAgents.map((agent) => (
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
                  {supportedAgents.map((agent) => (
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
        </section>

        <section
          id="how-it-works"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <FirstScanSteps />
        </section>

        <section {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}>
          <SidecarWalkthrough theme={theme} title="Then, day to day." />
        </section>

        <section
          id="use-it"
          {...stylex.props(siteLayout.container, homeSectionStyles.section, styles.anchor)}
        >
          <UsageSection />
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
            See what your agents load.
          </h2>
          <p {...stylex.props(styles.closingCopy)}>
            Keep the skills your agents use and park the rest. Undo any time.
          </p>
          <DownloadButton theme={theme} inFinale />
          <TrustLine />
        </section>
        <SiteFooter page="home" />
      </ClosingFinale>
    </div>
  );
}

const styles = stylex.create({
  anchor: { scrollMarginTop: 72 },
  heroWrap: { isolation: "isolate", position: "relative" },
  topDither: {
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
    maxWidth: "46ch",
    textWrap: "pretty",
    "@media (max-width: 600px)": { fontSize: 16 },
  },
  visual: { minWidth: 0, position: "relative", width: "100%" },
  product: {
    paddingBottom: 96,
    position: "relative",
    zIndex: 1,
    "@media (max-width: 600px)": { paddingBottom: 64 },
  },
  headerCta: {
    alignItems: "center",
    backgroundColor: { default: siteTokens.accent, ":hover": siteTokens.accentHover },
    borderRadius: 8,
    color: siteTokens.accentText,
    fontSize: 13,
    fontWeight: 640,
    gap: 7,
    height: 36,
    paddingInline: 14,
    position: "relative",
    textDecoration: "none",
    transition: "background-color 150ms ease-out, transform 150ms ease-out",
    ":active": { transform: "scale(.96)" },
    "::before": { content: '""', inset: "-4px 0", position: "absolute" },
  },
  actions: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 14,
    "@media (max-width: 1080px)": { justifyContent: "center", width: "100%" },
  },
  fineprint: {
    color: siteTokens.muted,
    fontSize: 12,
    lineHeight: 1.5,
    margin: "14px 0 0",
    textWrap: "balance",
  },
  inlineLink: { fontSize: "inherit", minHeight: "auto" },
  pointerOnly: { display: { default: "inline-flex", [TOUCH_DEVICE]: "none" } },
  touchOnly: { display: { default: "none", [TOUCH_DEVICE]: "inline-flex" } },
  pointerOnlyBlock: { display: { default: "block", [TOUCH_DEVICE]: "none" } },
  touchOnlyBlock: { display: { default: "none", [TOUCH_DEVICE]: "block" } },
  arrowDown: { transform: "rotate(90deg)" },
  arrowUp: { transform: "rotate(-90deg)" },
  primaryButton: {
    textDecoration: "none",
    alignItems: "center",
    backgroundColor: siteTokens.accent,
    border: 0,
    borderColor: "oklch(1 0 0 / .28)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 1px 0 oklch(1 0 0 / .3) inset, 0 10px 30px oklch(0 0 0 / .3)",
    color: siteTokens.accentText,
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 15,
    fontWeight: 680,
    gap: 18,
    justifyContent: "space-between",
    minHeight: 54,
    minWidth: 248,
    padding: "7px 8px 7px 18px",
    transition:
      "background-color 150ms ease-out, box-shadow 150ms ease-out, transform 150ms ease-out",
    "@media (hover: hover) and (pointer: fine)": {
      ":hover": {
        backgroundColor: siteTokens.accentHover,
        boxShadow: "0 1px 0 oklch(1 0 0 / .35) inset, 0 14px 38px oklch(0 0 0 / .38)",
        transform: "translateY(-1px)",
      },
      ":hover:active": {
        boxShadow: "0 1px 0 oklch(1 0 0 / .18) inset, 0 4px 12px oklch(0 0 0 / .24)",
        transform: "translateY(2px) scale(.96)",
      },
    },
    ":active": {
      boxShadow: "0 1px 0 oklch(1 0 0 / .18) inset, 0 4px 12px oklch(0 0 0 / .24)",
      transform: "translateY(2px) scale(.96)",
    },
    "@media (max-width: 600px)": { fontSize: 16, minHeight: 56, width: "100%" },
  },
  primaryButtonArrow: {
    alignItems: "center",
    backgroundColor: siteTokens.accentText,
    borderRadius: 7,
    color: siteTokens.text,
    display: "flex",
    height: 38,
    justifyContent: "center",
    overflow: "hidden",
    position: "relative",
    width: 38,
  },
  sourceLink: {
    alignItems: "center",
    backgroundColor: "transparent",
    cursor: "pointer",
    fontFamily: "inherit",
    borderColor: siteTokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    display: "inline-flex",
    fontSize: 15,
    fontWeight: 620,
    gap: 9,
    justifyContent: "center",
    minHeight: 54,
    paddingInline: 16,
    textDecoration: "none",
    transition:
      "background-color 150ms ease-out, border-color 150ms ease-out, transform 150ms ease-out",
    ":hover": { backgroundColor: siteTokens.surface, borderColor: siteTokens.muted },
    ":active": { transform: "scale(.96)" },
    "@media (max-width: 600px)": { minHeight: 48 },
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
    fontSize: 12,
    gap: 28,
    justifyContent: "center",
    minHeight: 76,
    overflow: "hidden",
    padding: "0 28px",
    "@media (max-width: 600px)": { gap: 18, minHeight: 68, paddingInline: 20 },
  },
  agentProofLabel: {
    color: siteTokens.muted,
    flexShrink: 0,
    fontSize: 12,
    whiteSpace: "nowrap",
  },
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
  marqueeSet: {
    alignItems: "center",
    display: "flex",
    gap: 40,
    paddingRight: 40,
  },
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
    // The mascot perks up while the download button below it is hovered.
    transform: {
      default: "none",
      "@media (hover: hover) and (pointer: fine)": {
        [stylex.when.anySibling(":hover", finaleCtaMarker)]: "translateY(-6px) rotate(-6deg)",
      },
    },
    transition: "transform 300ms cubic-bezier(0.34, 1.56, 0.64, 1)",
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
