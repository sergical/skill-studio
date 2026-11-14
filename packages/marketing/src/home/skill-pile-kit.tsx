// ============================================================================
// Skill Studio - Skill pile kit
// Stage size, chip poses and flights for the hero pile. Chips and the mascot
// move by inline transforms that the Web Animations API drives, so React never
// re-renders 92 chips for a frame of motion.
// ============================================================================
import { useLayoutEffect, useRef, useState, type ReactNode, type Ref } from "react";
import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";
import {
  PILE,
  KEPT_CHIPS,
  PILE_COUNTS,
  STATE_COLOR,
  isKept,
  type PileChip,
  type PileState,
} from "./skill-pile-data";

const INTRO_KEY_PREFIX = "skill-pile-intro-";
export const KEPT_SLOT = new Map(KEPT_CHIPS.map((chip, slot) => [chip.id, slot]));
const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";

const MESSY_SUMMARY = `${PILE_COUNTS.total} skills in ${PILE_COUNTS.folders} folders. ${PILE_COUNTS.copies} of them are copies.`;
const TIDY_SUMMARY = `${PILE_COUNTS.kept} kept. ${PILE_COUNTS.parked} parked, ${PILE_COUNTS.broken} fixed, ${PILE_COUNTS.outdated} updated.`;

const RESOLVED_LABEL = {
  ok: undefined,
  broken: "fixed",
  outdated: "updated",
  unused: undefined,
  copy: undefined,
} satisfies Record<PileState, string | undefined>;

export interface Point {
  x: number;
  y: number;
}

export interface Pose extends Point {
  r?: number;
  s?: number;
  o?: number;
  // Horizontal anchor in % of the chip width: -50 centers it, 0 pins its left edge.
  ax?: number;
}

export interface Geometry {
  w: number;
  h: number;
  narrow: boolean;
  mascotSize: number;
  mascot: Point;
  drawer: Point;
}

export function geometry(w: number, h: number): Geometry {
  const narrow = w < 520;
  const mascotSize = narrow ? 92 : 132;
  const mascot = narrow ? { x: w / 2, y: 52 } : { x: w * 0.8, y: h * 0.27 };
  const drawer = narrow
    ? { x: w / 2, y: h - 18 }
    : { x: mascot.x, y: mascot.y + mascotSize / 2 + 34 };
  return { w, h, narrow, mascotSize, mascot, drawer };
}

export function slotPose(g: Geometry, slot: number): Pose {
  const perColumn = Math.ceil(KEPT_CHIPS.length / 2);
  const column = slot < perColumn ? 0 : 1;
  const row = slot % perColumn;
  if (g.narrow) return { x: column === 0 ? 0 : g.w / 2 + 4, y: 128 + row * 32, ax: 0 };
  const step = 38;
  const top = (g.h - perColumn * step) / 2 + step / 2;
  return { x: column === 0 ? 0 : g.w * 0.34, y: top + row * step, ax: 0 };
}

// A fixed hash, so every visit builds the same pile.
export function rand(index: number, salt: number) {
  const value = Math.sin(index * 12.9898 + salt * 78.233) * 43758.5453;
  return value - Math.floor(value);
}

// A mound: wide rows at the bottom, narrower rows on top, chips overlapping and tilted.
export function heapPoses(g: Geometry): Pose[] {
  const spacing = g.narrow ? 30 : 46;
  const rise = g.narrow ? 17 : 21;
  const span = (row: number) => g.w * Math.max(0.18, 0.96 - row * 0.075);
  const capacity = (row: number) => Math.max(2, Math.floor(span(row) / spacing));
  const poses: Pose[] = [];
  let row = 0;
  let inRow = 0;
  PILE.forEach((_, index) => {
    if (inRow >= capacity(row)) {
      row += 1;
      inRow = 0;
    }
    const width = span(row);
    const x =
      g.w / 2 -
      width / 2 +
      (inRow + 0.5) * (width / capacity(row)) +
      (rand(index, 1) - 0.5) * spacing * 0.7;
    poses.push({
      x,
      y: g.h - 24 - row * rise + (rand(index, 2) - 0.5) * 8,
      r: (rand(index, 3) - 0.5) * 32,
    });
    inRow += 1;
  });
  return poses;
}

export const poseTransform = ({ x, y, r = 0, s = 1, ax = -50 }: Pose) =>
  `translate(${x}px, ${y}px) translate(${ax}%, -50%) rotate(${r}deg) scale(${s})`;

export const squash = (pose: Pose, sx: number, sy: number) =>
  `${poseTransform(pose)} scale(${sx}, ${sy})`;

export const frame = (pose: Pose, extra: Keyframe = {}): Keyframe => ({
  transform: poseTransform(pose),
  opacity: pose.o ?? 1,
  ...extra,
});

export const lerpPose = (from: Pose, to: Pose, t: number, extra: Partial<Pose> = {}): Pose => ({
  x: from.x + (to.x - from.x) * t,
  y: from.y + (to.y - from.y) * t,
  r: (from.r ?? 0) + ((to.r ?? 0) - (from.r ?? 0)) * t,
  s: (from.s ?? 1) + ((to.s ?? 1) - (from.s ?? 1)) * t,
  o: 1,
  ax: (from.ax ?? -50) + ((to.ax ?? -50) - (from.ax ?? -50)) * t,
  ...extra,
});

export function settleAnimations(element: HTMLElement) {
  for (const animation of element.getAnimations()) {
    animation.commitStyles();
    animation.cancel();
  }
  element.style.translate = "";
  element.style.scale = "";
}

export function setStyle(
  element: HTMLElement | null | undefined,
  values: Partial<
    Pick<
      CSSStyleDeclaration,
      | "translate"
      | "zIndex"
      | "transition"
      | "transform"
      | "opacity"
      | "pointerEvents"
      | "cursor"
      | "touchAction"
    >
  >,
) {
  if (element) Object.assign(element.style, values);
}

export function place(element: HTMLElement | null, pose: Pose) {
  if (!element) return;
  settleAnimations(element);
  element.style.transform = poseTransform(pose);
  element.style.opacity = String(pose.o ?? 1);
}

// Starts from wherever the element is now, so a flight can interrupt another one.
export function fly(
  element: HTMLElement | null,
  frames: Keyframe[],
  { startEasing = SETTLE, ...options }: KeyframeAnimationOptions & { startEasing?: string },
  onDone?: () => void,
) {
  if (!element) return;
  settleAnimations(element);
  const start: Keyframe = {
    transform: element.style.transform,
    opacity: element.style.opacity || "1",
    offset: 0,
    easing: startEasing,
  };
  const animation = element.animate([start, ...frames], { fill: "forwards", ...options });
  animation.finished.then(
    () => {
      animation.commitStyles();
      animation.cancel();
      onDone?.();
    },
    () => undefined,
  );
}

// A short shove on the separate `translate` property, so it adds to a running flight.
export function pulse(element: HTMLElement | null, amount = 1.08) {
  element?.animate([{ scale: "1" }, { scale: String(amount) }, { scale: "1" }], {
    duration: 200,
    easing: "ease-out",
  });
}

export function introSeen(key: string) {
  try {
    return sessionStorage.getItem(INTRO_KEY_PREFIX + key) === "1";
  } catch {
    return false;
  }
}

export function markIntroSeen(key: string) {
  try {
    sessionStorage.setItem(INTRO_KEY_PREFIX + key, "1");
  } catch {
    // Without storage the intro plays on every visit, which is harmless.
  }
}

export function usePileStage() {
  const stageRef = useRef<HTMLDivElement>(null);
  const chipRefs = useRef<Array<HTMLSpanElement | null>>([]);
  const mascotRef = useRef<HTMLButtonElement>(null);
  const drawerRef = useRef<HTMLSpanElement>(null);
  const countRef = useRef<HTMLSpanElement>(null);
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);

  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const measure = () => {
      const { width, height } = stage.getBoundingClientRect();
      setSize((current) =>
        current && current.w === width && current.h === height ? current : { w: width, h: height },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, []);

  return { stageRef, chipRefs, mascotRef, drawerRef, countRef, size };
}

export function setCount(countRef: { current: HTMLSpanElement | null }, value: number) {
  if (countRef.current) countRef.current.textContent = String(value);
}

export function PileChipView({
  chip,
  tidy,
  narrow,
  badge,
  chipRef,
}: {
  chip: PileChip;
  tidy: boolean;
  narrow: boolean;
  badge?: string;
  chipRef: Ref<HTMLSpanElement>;
}) {
  const kept = isKept(chip);
  const resolved = tidy && kept ? RESOLVED_LABEL[chip.state] : undefined;
  const meta = tidy && kept ? resolved : chip.folder;
  return (
    <span
      ref={chipRef}
      aria-hidden="true"
      {...stylex.props(styles.chip, narrow && styles.chipNarrow)}
    >
      <span
        {...stylex.props(
          styles.dot,
          styles.dotColor(resolved ? STATE_COLOR.ok : STATE_COLOR[chip.state]),
        )}
      />
      {chip.name}
      {badge && <span {...stylex.props(styles.badge)}>{badge}</span>}
      {meta && !narrow && (
        <span {...stylex.props(styles.meta, resolved !== undefined && styles.metaResolved)}>
          {meta}
        </span>
      )}
    </span>
  );
}

export function PileMascot({
  mascotRef,
  size,
  label,
  pressed,
  onClick,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  grabbable = false,
}: {
  mascotRef: Ref<HTMLButtonElement>;
  size: number;
  label: string;
  pressed: boolean;
  onClick: () => void;
  onPointerDown?: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onPointerMove?: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onPointerUp?: (event: React.PointerEvent<HTMLButtonElement>) => void;
  grabbable?: boolean;
}) {
  return (
    <button
      ref={mascotRef}
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      onClick={onClick}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      {...stylex.props(styles.mascot, grabbable && styles.mascotGrab)}
    >
      <img
        src="/skill-studio-logo-512.webp"
        alt=""
        width={size}
        height={size}
        draggable={false}
        {...stylex.props(styles.mascotImage, styles.mascotSize(size))}
      />
    </button>
  );
}

export function PileDrawer({
  drawerRef,
  countRef,
  at,
  open,
}: {
  drawerRef: Ref<HTMLSpanElement>;
  countRef: Ref<HTMLSpanElement>;
  at: Point;
  open: boolean;
}) {
  return (
    <span
      ref={drawerRef}
      aria-hidden="true"
      style={{ transform: `translate(${at.x}px, ${at.y}px) translate(-50%, -50%)` }}
      {...stylex.props(styles.drawer, !open && styles.drawerClosed)}
    >
      Parked{" "}
      <span ref={countRef} {...stylex.props(styles.drawerCount)}>
        0
      </span>
    </span>
  );
}

export function PileFrame({
  stageRef,
  ready,
  tidy,
  children,
}: {
  stageRef: Ref<HTMLDivElement>;
  ready: boolean;
  tidy: boolean;
  children: ReactNode;
}) {
  return (
    <div {...stylex.props(styles.frame)}>
      <div ref={stageRef} {...stylex.props(styles.stage, !ready && styles.stageHidden)}>
        {children}
      </div>
      <p {...stylex.props(styles.visuallyHidden)} aria-live="polite">
        {tidy ? TIDY_SUMMARY : MESSY_SUMMARY}
      </p>
    </div>
  );
}

const styles = stylex.create({
  frame: { textAlign: "center", width: "100%" },
  stage: {
    height: { default: 480, "@media (max-width: 600px)": 440 },
    position: "relative",
    width: "100%",
  },
  stageHidden: { visibility: "hidden" },
  chip: {
    alignItems: "center",
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 1px 2px oklch(0 0 0 / .12), 0 6px 18px oklch(0 0 0 / .14)",
    color: siteTokens.text,
    display: "inline-flex",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    gap: 7,
    height: 30,
    left: 0,
    paddingInline: "11px 12px",
    pointerEvents: "none",
    position: "absolute",
    top: 0,
    userSelect: "none",
    whiteSpace: "nowrap",
    willChange: "transform",
  },
  chipNarrow: { fontSize: 11, gap: 6, height: 26, paddingInline: "9px 10px" },
  dot: {
    borderRadius: 999,
    flexShrink: 0,
    height: 7,
    transition: "background-color 240ms ease-out 360ms",
    width: 7,
  },
  dotColor: (color: string) => ({ backgroundColor: color }),
  badge: {
    backgroundColor: "oklch(0.8 0.15 80 / .18)",
    borderRadius: 999,
    color: "oklch(0.8 0.15 80)",
    fontSize: 10,
    fontWeight: 650,
    paddingInline: 6,
  },
  meta: {
    color: siteTokens.muted,
    fontFamily: "-apple-system, BlinkMacSystemFont, sans-serif",
    fontSize: 11,
  },
  metaResolved: { color: "oklch(0.75 0.17 150)" },
  mascot: {
    backgroundColor: "transparent",
    borderRadius: 32,
    borderWidth: 0,
    cursor: "pointer",
    left: 0,
    padding: 0,
    position: "absolute",
    top: 0,
    transformOrigin: "50% 100%",
    WebkitTapHighlightColor: "transparent",
    willChange: "transform",
    zIndex: 3,
    "::before": {
      backgroundImage: "radial-gradient(closest-side, oklch(0.45 0.2 293 / .45), transparent)",
      content: '""',
      inset: "-20% -30% -40%",
      pointerEvents: "none",
      position: "absolute",
      transform: "translateY(12%)",
    },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 4 },
  },
  mascotGrab: { cursor: { default: "grab", ":active": "grabbing" }, touchAction: "none" },
  mascotImage: {
    display: "block",
    height: "auto",
    position: "relative",
    transition: `transform 220ms ${SETTLE}`,
    "@media (hover: hover) and (pointer: fine)": {
      ":hover": { transform: "rotate(-5deg) scale(1.04)" },
    },
    "@media (prefers-reduced-motion: reduce)": { transition: "none" },
  },
  mascotSize: (size: number) => ({ width: size }),
  drawer: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "dashed",
    borderWidth: 1,
    color: siteTokens.muted,
    fontSize: 12,
    left: 0,
    opacity: 1,
    paddingBlock: 6,
    paddingInline: 14,
    pointerEvents: "none",
    position: "absolute",
    top: 0,
    transition: "opacity 200ms ease-out",
    whiteSpace: "nowrap",
    zIndex: 2,
  },
  drawerClosed: { opacity: 0 },
  drawerCount: {
    color: siteTokens.text,
    display: "inline-block",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 640,
    minWidth: "2ch",
    textAlign: "left",
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
});
