// ============================================================================
// Skill Studio - Share card
// The 1200×630 image that link previews show (public/og.png): the homepage hero
// in its sorted state. scripts/render-share-card.ts renders it.
// ============================================================================
import { useEffect, useState, type CSSProperties } from "react";
import { Dithering } from "@paper-design/shaders-react";
import { AbsoluteFill, Img, continueRender, delayRender } from "remotion";

import logo from "../../public/skill-studio-logo-512.webp";
import { DITHER_COLORS } from "../home/dither-colors";
import { SHARE_CARD_CONTENT } from "../share-card";

const { title, subline, host, surfaces, skills, cleanup } = SHARE_CARD_CONTENT;

export const OG_CARD_SIZE = { width: 1200, height: 630 };

const SANS = '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif';
const MONO = "ui-monospace, SFMono-Regular, Menlo, monospace";
const COLOR = {
  background: "oklch(0.11 0 0)",
  surface: "oklch(0.15 0 0)",
  border: "oklch(0.24 0 0)",
  text: "oklch(0.97 0 0)",
  muted: "oklch(0.72 0 0)",
  fine: "oklch(0.696 0.149 162.48)",
  parked: "oklch(0.6 0.02 290)",
  danger: "oklch(0.645 0.208 25.331)",
  warning: "oklch(0.769 0.165 70.08)",
};

const CLEANUP = [
  { label: "parked", count: cleanup.parked, dot: COLOR.parked },
  { label: "fixed", count: cleanup.fixed, dot: COLOR.danger },
  { label: "updated", count: cleanup.updated, dot: COLOR.warning },
];

// The shader paints on an animation frame after it mounts, so the still waits for two.
function useShaderPaint() {
  // oxlint-disable-next-line react/hook-use-state -- Remotion's pattern: one handle per mount, taken before the first paint
  const [handle] = useState(() => delayRender("Paint the dither"));
  useEffect(() => {
    let frameId = requestAnimationFrame(() => {
      frameId = requestAnimationFrame(() => continueRender(handle));
    });
    return () => cancelAnimationFrame(frameId);
  }, [handle]);
}

function Dot({ color }: { color: string }) {
  return <span style={{ ...styles.dot, backgroundColor: color }} />;
}

export function OgCard() {
  useShaderPaint();
  return (
    <AbsoluteFill style={styles.card}>
      <div style={styles.dither}>
        <Dithering
          width="100%"
          height="100%"
          colorBack="#00000000"
          colorFront={DITHER_COLORS.dark.front}
          // oxlint-disable-next-line anti-slop/no-shape-in-symbol-names -- Paper Shaders' own prop name
          shape="warp"
          type="4x4"
          size={3}
          scale={0.8}
          speed={0}
          minPixelRatio={1}
          maxPixelCount={OG_CARD_SIZE.width * OG_CARD_SIZE.height}
        />
      </div>

      <div style={styles.copy}>
        <div style={styles.brand}>
          <Img src={logo} style={styles.brandLogo} />
          Skill Studio
        </div>
        <h1 style={styles.title}>{title}</h1>
        <p style={styles.lede}>{subline}</p>
        <div style={styles.footer}>
          <span style={styles.host}>{host}</span>
          <span>{surfaces}</span>
        </div>
      </div>

      <div style={styles.chips}>
        {skills.map((name) => (
          <span key={name} style={styles.chip}>
            <Dot color={COLOR.fine} />
            {name}
          </span>
        ))}
      </div>

      <div style={styles.mascot}>
        <Img src={logo} style={styles.mascotImage} />
        {CLEANUP.map((pill) => (
          <span key={pill.label} style={styles.cleanup}>
            <Dot color={pill.dot} />
            <b style={styles.cleanupCount}>{pill.count}</b>
            {pill.label}
          </span>
        ))}
      </div>
    </AbsoluteFill>
  );
}

const pill: CSSProperties = {
  alignItems: "center",
  backgroundColor: COLOR.surface,
  borderColor: COLOR.border,
  borderRadius: 999,
  borderStyle: "solid",
  borderWidth: 1,
  display: "inline-flex",
  whiteSpace: "nowrap",
};

const styles = {
  card: {
    backgroundColor: COLOR.background,
    color: COLOR.text,
    fontFamily: SANS,
    WebkitFontSmoothing: "antialiased",
  },
  // The hero's dither, masked to the right so the copy sits on plain background.
  dither: {
    imageRendering: "pixelated",
    inset: 0,
    maskImage: "radial-gradient(ellipse 62% 105% at 92% 18%, black 12%, transparent 74%)",
    opacity: 0.6,
    position: "absolute",
  },
  copy: {
    bottom: 60,
    display: "flex",
    flexDirection: "column",
    left: 72,
    position: "absolute",
    top: 64,
    width: 560,
  },
  brand: { alignItems: "center", display: "flex", fontSize: 22, fontWeight: 650, gap: 12 },
  brandLogo: { height: 40, width: 40 },
  title: {
    fontSize: 92,
    fontWeight: 700,
    letterSpacing: "-.03em",
    lineHeight: 0.98,
    margin: "auto 0 0",
    textWrap: "balance",
  },
  lede: {
    color: COLOR.muted,
    fontSize: 23,
    lineHeight: 1.45,
    margin: "22px 0 0",
    maxWidth: "30ch",
    textWrap: "pretty",
  },
  footer: {
    color: COLOR.muted,
    display: "flex",
    flexDirection: "column",
    fontSize: 18,
    gap: 6,
    marginTop: "auto",
    paddingTop: 40,
  },
  host: { color: COLOR.text, fontSize: 20, fontWeight: 600 },
  chips: {
    display: "grid",
    fontFamily: MONO,
    gap: "13px 14px",
    gridAutoFlow: "column",
    gridTemplateRows: `repeat(${Math.ceil(skills.length / 2)}, auto)`,
    justifyItems: "start",
    left: 640,
    position: "absolute",
    top: 112,
  },
  chip: { ...pill, fontSize: 15, gap: 9, height: 34, paddingInline: "13px 15px" },
  dot: { borderRadius: 999, flexShrink: 0, height: 7, width: 7 },
  mascot: {
    alignItems: "center",
    display: "flex",
    flexDirection: "column",
    gap: 10,
    position: "absolute",
    right: 48,
    top: 150,
  },
  mascotImage: {
    filter: "drop-shadow(0 20px 40px oklch(0.45 0.2 293 / .55))",
    marginBottom: 10,
    width: 150,
  },
  cleanup: {
    ...pill,
    borderStyle: "dashed",
    color: COLOR.muted,
    fontSize: 14,
    gap: 7,
    height: 30,
    paddingInline: 13,
  },
  cleanupCount: { color: COLOR.text, fontVariantNumeric: "tabular-nums", fontWeight: 640 },
} satisfies Record<string, CSSProperties>;
