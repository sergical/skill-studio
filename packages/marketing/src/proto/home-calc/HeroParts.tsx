// PROTO(home-calc): hero pieces copied from variants/CommandCenter.tsx (not exported there),
// plus the dither layer whose glow rises while the download button is hovered or focused.
import type { Ref } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, Copy, Download } from "lucide-react";
import { AGENT_SETUP_PROMPT } from "@skill-studio/lib/agent-setup";

import { Arrow } from "../../MarketingBrand";
import { DITHER_COLORS } from "../../home/ClosingFinale";
import { finaleCtaMarker } from "../../home/ClosingFinale.stylex";
import { homeSectionStyles } from "../../home/HomeSection.stylex";
import { useCopyFlash } from "../../home/useCopyFlash";
import { WarpDither } from "../../home/WarpDither";
import {
  CLI_DOCS_URL,
  DOWNLOAD_INTEL_URL,
  DOWNLOAD_URL,
  MCP_DOCS_URL,
  TRUST_LINE,
} from "../../site-links";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import { heroCtaMarker } from "./hero.stylex";

export const HERO_TITLE = "Fix your skill issues.";
export const HERO_SUBLINE =
  "Find, fix and park the agent skills you use across Claude Code, Codex, Cursor and more.";

const TOUCH_DEVICE = "@media (hover: none) and (pointer: coarse)";
const HOVER = "@media (hover: hover) and (pointer: fine)";
const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";

export function DownloadButton({
  theme,
  place,
  buttonRef,
}: {
  theme: SiteTheme;
  place: "hero" | "finale";
  buttonRef?: Ref<HTMLAnchorElement>;
}) {
  const marker = place === "hero" ? heroCtaMarker : finaleCtaMarker;
  return (
    <>
      <a
        ref={buttonRef}
        href={DOWNLOAD_URL}
        {...stylex.props(marker, styles.primaryButton, styles.pointerOnly)}
      >
        <span>Download for macOS</span>
        <span aria-hidden="true" {...stylex.props(styles.primaryButtonArrow)}>
          <span {...stylex.props(styles.arrowNudge)}>
            <Arrow inverse={theme === "dark"} />
          </span>
        </span>
      </a>
      <a href="#use-it" {...stylex.props(marker, styles.primaryButton, styles.touchOnly)}>
        <span>Get Skill Studio</span>
        <span
          aria-hidden="true"
          {...stylex.props(
            styles.primaryButtonArrow,
            place === "finale" ? styles.arrowUp : styles.arrowDown,
          )}
        >
          <Arrow inverse={theme === "dark"} />
        </span>
      </a>
    </>
  );
}

export function CopyAgentPromptButton() {
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

export function TrustLine() {
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

export function HeaderDownload() {
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

/**
 * The hero's warp dither. The outer layer reads the glow variables the hero sets while its
 * download button is hovered; the inner layer is the one a pointer engine may move or brighten.
 */
export function HeroDither({
  theme,
  layerRef,
}: {
  theme: SiteTheme;
  layerRef?: Ref<HTMLDivElement>;
}) {
  return (
    <div aria-hidden="true" {...stylex.props(styles.dither)}>
      <div ref={layerRef} {...stylex.props(styles.ditherLayer)}>
        <WarpDither colorBack="#00000000" colorFront={DITHER_COLORS[theme].front} />
      </div>
    </div>
  );
}

// The glow rises slowly toward the button and retreats faster once the pointer leaves it.
const glowIn = `--hero-glow-alpha 500ms ${SETTLE}, --hero-glow-reach 600ms ${SETTLE}, --hero-glow-x 600ms ${SETTLE}`;
const glowOut = `--hero-glow-alpha 300ms ${SETTLE}, --hero-glow-reach 350ms ${SETTLE}, --hero-glow-x 350ms ${SETTLE}`;

/** How far the dither glows at rest and toward the hovered download button, per variant. */
export const heroGlow = stylex.create({
  baseline: {
    "--hero-glow-alpha": {
      default: 0.45,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: 0.85,
      [HOVER]: { default: 0.45, [stylex.when.descendant(":hover", heroCtaMarker)]: 0.85 },
    },
    "--hero-glow-reach": {
      default: "70%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "130%",
      [HOVER]: { default: "70%", [stylex.when.descendant(":hover", heroCtaMarker)]: "130%" },
    },
    "--hero-glow-x": {
      default: "70%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "30%",
      [HOVER]: { default: "70%", [stylex.when.descendant(":hover", heroCtaMarker)]: "30%" },
    },
    transition: {
      default: glowOut,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: glowIn,
      [HOVER]: { default: glowOut, [stylex.when.descendant(":hover", heroCtaMarker)]: glowIn },
    },
  },
  kinetic: {
    "--hero-glow-alpha": {
      default: 0.55,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: 1,
      [HOVER]: { default: 0.55, [stylex.when.descendant(":hover", heroCtaMarker)]: 1 },
    },
    "--hero-glow-reach": {
      default: "80%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "150%",
      [HOVER]: { default: "80%", [stylex.when.descendant(":hover", heroCtaMarker)]: "150%" },
    },
    "--hero-glow-x": {
      default: "70%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "25%",
      [HOVER]: { default: "70%", [stylex.when.descendant(":hover", heroCtaMarker)]: "25%" },
    },
    transition: {
      default: glowOut,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: glowIn,
      [HOVER]: { default: glowOut, [stylex.when.descendant(":hover", heroCtaMarker)]: glowIn },
    },
  },
  editorial: {
    "--hero-glow-alpha": {
      default: 0.3,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: 0.8,
      [HOVER]: { default: 0.3, [stylex.when.descendant(":hover", heroCtaMarker)]: 0.8 },
    },
    "--hero-glow-reach": {
      default: "60%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "120%",
      [HOVER]: { default: "60%", [stylex.when.descendant(":hover", heroCtaMarker)]: "120%" },
    },
    "--hero-glow-x": {
      default: "75%",
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: "20%",
      [HOVER]: { default: "75%", [stylex.when.descendant(":hover", heroCtaMarker)]: "20%" },
    },
    transition: {
      default: glowOut,
      [stylex.when.descendant(":focus-visible", heroCtaMarker)]: glowIn,
      [HOVER]: { default: glowOut, [stylex.when.descendant(":hover", heroCtaMarker)]: glowIn },
    },
  },
});

export const heroPartStyles = stylex.create({
  actions: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 14,
    "@media (max-width: 1080px)": { justifyContent: "center", width: "100%" },
  },
});

const styles = stylex.create({
  dither: {
    height: 680,
    insetInline: 0,
    maskImage:
      "radial-gradient(ellipse var(--hero-glow-reach) calc(var(--hero-glow-reach) + 10%) at var(--hero-glow-x) 0%, black 10%, transparent 72%)",
    opacity: "var(--hero-glow-alpha)",
    overflow: "hidden",
    pointerEvents: "none",
    position: "absolute",
    top: -64,
    zIndex: -1,
  },
  // Bigger than its mask, so a pointer engine can shift it without showing an edge.
  ditherLayer: {
    inset: -24,
    position: "absolute",
    translate: "calc(var(--hero-px, 0) * 18px) calc(var(--hero-py, 0) * 12px)",
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
  fineprint: {
    color: siteTokens.muted,
    fontSize: 12,
    lineHeight: 1.5,
    margin: "14px 0 0",
    textWrap: "balance",
  },
  inlineLink: {
    fontSize: "inherit",
    minHeight: "auto",
    textDecorationColor: siteTokens.border,
    textDecorationLine: "underline",
    textDecorationThickness: 1,
    textUnderlineOffset: 3,
    transition: "color 150ms ease-out, text-decoration-color 150ms ease-out",
    ":hover": { color: siteTokens.accent, textDecorationColor: "currentColor" },
  },
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
    [HOVER]: {
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
  // The arrow leans toward where the button goes while it is hovered.
  arrowNudge: {
    display: "flex",
    transform: {
      default: "none",
      [HOVER]: { [stylex.when.ancestor(":hover", heroCtaMarker)]: "translateX(3px)" },
    },
    transition: `transform 200ms ${SETTLE}`,
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
    "@media (max-width: 600px)": { minHeight: 48, width: "100%" },
  },
});
