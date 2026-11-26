// ============================================================================
// Skill Studio - Share card
// The 1200×630 image that link previews show (public/og.jpg): the skill pile
// heaped on the left, a short tidy list on the right, and the mascot and
// headline in the centre square, which is all that Slack, LinkedIn and
// WhatsApp thumbnails keep. scripts/render-share-card.ts renders it; run it
// after any edit here.
// ============================================================================
import { useEffect, useState, type CSSProperties } from "react";
import { Dithering } from "@paper-design/shaders-react";
import { AbsoluteFill, Img, continueRender, delayRender } from "remotion";

import logo from "../../public/skill-studio-logo-512.webp";
import { DITHER_COLORS } from "../home/dither-colors";
import { SHARE_CARD_CONTENT } from "../share-card";

const { title, host, pile, sorted, cleanup } = SHARE_CARD_CONTENT;

export const OG_CARD_SIZE = { width: 1200, height: 630 };
// Thumbnails crop the middle square: columns 285 to 915 of the 1200px card.
const CENTRE = { left: (OG_CARD_SIZE.width - OG_CARD_SIZE.height) / 2, size: OG_CARD_SIZE.height };

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
const STATUS_DOT = { fine: COLOR.parked, warning: COLOR.warning, broken: COLOR.danger };

const CLEANUP = [
  { label: "parked", count: cleanup.parked, dot: COLOR.parked },
  { label: "fixed", count: cleanup.fixed, dot: COLOR.danger },
  { label: "updated", count: cleanup.updated, dot: COLOR.warning },
];

// A fixed seed, so every render draws the same heap.
function seededRandom(seed: number) {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Densest at the left edge and thinner toward the centre, so the headline stays clear.
const HEAP = (() => {
  const random = seededRandom(31);
  return pile.map((chip) => {
    const depth = Math.pow(random(), 0.6);
    const x = -110 + random() * 330 * (1 - depth * 0.35);
    const y = OG_CARD_SIZE.height - Math.pow(random(), 0.55) * 700;
    const nearCentre = Math.max(0, (x + 40) / 300);
    const roll = random();
    return {
      chip,
      x,
      y,
      rotate: (random() - 0.5) * 70,
      scale: 0.82 + random() * 0.36,
      opacity: 0.55 + random() * 0.45 - nearCentre * 0.25,
      note: chip.inUse ? undefined : roll < 0.45 ? chip.folder : roll < 0.7 ? "unused" : undefined,
    };
  });
})();

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

function Chip({
  name,
  dot,
  note,
  style,
}: {
  name: string;
  dot: string;
  note?: string;
  style?: CSSProperties;
}) {
  return (
    <span style={{ ...styles.chip, ...style }}>
      <span style={{ ...styles.dot, backgroundColor: dot }} />
      {name}
      {note && <span style={styles.note}>{note}</span>}
    </span>
  );
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

      <div style={styles.heap}>
        {HEAP.map(({ chip, x, y, rotate, scale, opacity, note }, index) => (
          <Chip
            key={`${chip.name}@${chip.folder}#${index}`}
            name={chip.name}
            dot={chip.inUse ? COLOR.fine : STATUS_DOT[chip.status]}
            note={note}
            style={{
              left: x,
              opacity,
              position: "absolute",
              top: y,
              transform: `rotate(${rotate}deg) scale(${scale})`,
            }}
          />
        ))}
      </div>

      <div style={styles.sorted}>
        {sorted.map((name) => (
          <Chip key={name} name={name} dot={COLOR.fine} />
        ))}
      </div>

      <div style={styles.centre}>
        <div style={styles.centreShade} />
        <Img src={logo} style={styles.mascot} />
        <h1 style={styles.title}>{title}</h1>
        <div style={styles.cleanup}>
          {CLEANUP.map((pill) => (
            <span key={pill.label} style={styles.cleanupPill}>
              <span style={{ ...styles.dot, backgroundColor: pill.dot }} />
              <b style={styles.cleanupCount}>{pill.count}</b>
              {pill.label}
            </span>
          ))}
        </div>
      </div>

      <div style={styles.host}>{host}</div>
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
  // The hero's dither, rising behind the mascot.
  dither: {
    imageRendering: "pixelated",
    inset: 0,
    maskImage: "radial-gradient(ellipse 50% 70% at 52% 28%, black 10%, transparent 70%)",
    opacity: 0.5,
    position: "absolute",
  },
  heap: {
    height: OG_CARD_SIZE.height,
    left: 0,
    maskImage: "linear-gradient(90deg, black 62%, transparent 100%)",
    overflow: "hidden",
    position: "absolute",
    top: 0,
    width: 430,
  },
  sorted: {
    alignItems: "flex-start",
    display: "flex",
    flexDirection: "column",
    gap: 18,
    height: OG_CARD_SIZE.height,
    justifyContent: "center",
    position: "absolute",
    right: 64,
    top: 0,
    width: 190,
  },
  chip: {
    ...pill,
    color: COLOR.text,
    fontFamily: MONO,
    fontSize: 15,
    gap: 9,
    height: 34,
    paddingInline: "13px 15px",
  },
  dot: { borderRadius: 999, flexShrink: 0, height: 7, width: 7 },
  note: { color: COLOR.muted, fontFamily: SANS, fontSize: 12 },
  centre: {
    alignItems: "center",
    display: "flex",
    flexDirection: "column",
    height: CENTRE.size,
    justifyContent: "center",
    left: CENTRE.left,
    position: "absolute",
    textAlign: "center",
    top: 0,
    width: CENTRE.size,
  },
  // Keeps stray pile chips from showing through behind the headline.
  centreShade: {
    background: "radial-gradient(closest-side, oklch(0.11 0 0 / .92), oklch(0.11 0 0 / 0))",
    inset: "70px 60px",
    position: "absolute",
  },
  mascot: {
    filter: "drop-shadow(0 20px 40px oklch(0.45 0.2 293 / .6))",
    position: "relative",
    width: 168,
  },
  title: {
    fontSize: 86,
    fontWeight: 700,
    letterSpacing: "-.03em",
    lineHeight: 0.98,
    margin: "26px 0",
    maxWidth: 560,
    position: "relative",
    textWrap: "balance",
  },
  cleanup: { display: "flex", gap: 10, position: "relative" },
  cleanupPill: {
    ...pill,
    borderStyle: "dashed",
    color: COLOR.muted,
    fontSize: 16,
    gap: 7,
    height: 35,
    paddingInline: 16,
  },
  cleanupCount: { color: COLOR.text, fontVariantNumeric: "tabular-nums", fontWeight: 640 },
  host: { color: COLOR.muted, fontSize: 18, position: "absolute", right: 36, top: 34 },
} satisfies Record<string, CSSProperties>;
