// ============================================================================
// Skill Studio - Skill pile kit
// Stage size, chip poses and flights for the hero pile. Chips and the mascot
// move by inline transforms that the Web Animations API drives, so React never
// re-renders 92 chips for a frame of motion.
// ============================================================================
import { useLayoutEffect, useRef, useState, type ReactNode, type Ref } from "react";
import * as stylex from "@stylexjs/stylex";

import { productDemoTokens } from "../ProductDemoTokens.stylex";
import { siteTokens } from "../SiteTheme.stylex";
import { ACTIVE_PILE_CHIPS, PILE, PILE_CLEANUP, type PileChip } from "./skill-pile-data";

/** Maps each active skill to its place in the sorted list. */
export const ACTIVE_PILE_SLOTS = new Map(ACTIVE_PILE_CHIPS.map((chip, slot) => [chip.id, slot]));
const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";

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
  // Mascot on top, parking at the bottom: for a stage alone in a centred column.
  stacked: boolean;
  mascotSize: number;
  mascot: Point;
  drawer: Point;
}

export interface StageSize {
  w: number;
  h: number;
  centered: boolean;
}

export function geometry({ w, h, centered }: StageSize): Geometry {
  const narrow = w < 520;
  const stacked = narrow || centered;
  const mascotSize = narrow ? 92 : stacked ? 112 : 132;
  const mascot = stacked ? { x: w / 2, y: mascotSize / 2 + 6 } : { x: w * 0.8, y: h * 0.27 };
  const drawer = stacked
    ? { x: w / 2, y: h - 18 }
    : { x: mascot.x, y: mascot.y + mascotSize / 2 + 34 };
  return { w, h, narrow, stacked, mascotSize, mascot, drawer };
}

export function slotPose(g: Geometry, slot: number): Pose {
  const perColumn = Math.ceil(ACTIVE_PILE_CHIPS.length / 2);
  const column = slot < perColumn ? 0 : 1;
  const row = slot % perColumn;
  if (g.narrow) return { x: column === 0 ? 0 : g.w / 2 + 4, y: 128 + row * 32, ax: 0 };
  if (g.stacked) {
    const top = g.mascot.y + g.mascotSize / 2 + 22;
    const step = Math.min(34, (g.drawer.y - 22 - top) / perColumn);
    return { x: g.w / 2 + (column === 0 ? -172 : -4), y: top + row * step, ax: 0 };
  }
  const step = 38;
  const top = (g.h - perColumn * step) / 2 + step / 2;
  return { x: column === 0 ? 0 : g.w * 0.34, y: top + row * step, ax: 0 };
}

// A fixed hash, so every visit builds the same pile.
export function rand(index: number, salt: number) {
  const value = Math.sin(index * 12.9898 + salt * 78.233) * 43758.5453;
  return value - Math.floor(value);
}

export interface ChipSize {
  w: number;
  h: number;
}

const HEAP_EDGE = 4;

// The heap sits under the mascot and, on stacked stages, above the drawer.
function heapBounds(g: Geometry) {
  const mascotBottom = g.mascot.y + g.mascotSize / 2;
  return g.stacked
    ? { top: mascotBottom + 12, bottom: g.drawer.y - 20 }
    : { top: mascotBottom, bottom: g.h };
}

// Half the width and height of the box that a tilted chip covers.
export function rotatedHalf(size: ChipSize, r = 0) {
  const angle = (Math.abs(r) * Math.PI) / 180;
  return {
    halfW: (size.w * Math.cos(angle) + size.h * Math.sin(angle)) / 2,
    halfH: (size.w * Math.sin(angle) + size.h * Math.cos(angle)) / 2,
  };
}

export const clampX = (g: Geometry, x: number, halfW: number) =>
  Math.min(Math.max(x, halfW + HEAP_EDGE), g.w - halfW - HEAP_EDGE);

function keepInside(g: Geometry, bottom: number, pose: Pose, size: ChipSize): Pose {
  const { halfW, halfH } = rotatedHalf(size, pose.r);
  return { ...pose, x: clampX(g, pose.x, halfW), y: Math.min(pose.y, bottom - halfH) };
}

// A mound: wide rows at the bottom, narrower rows on top, chips overlapping and tilted.
// A small stage packs chips closer so the mound stays as low as the free band allows.
export function heapPoses(g: Geometry, sizes: ChipSize[]): Pose[] {
  const { top, bottom } = heapBounds(g);
  const rise = g.narrow ? 17 : 21;
  const maxRows = Math.max(1, Math.floor((bottom - top - 40) / rise));
  const span = (row: number) => g.w * Math.max(0.18, 0.96 - row * 0.075);
  let spacing = g.narrow ? 30 : 46;
  const capacity = (row: number) => Math.max(2, Math.floor(span(row) / spacing));
  const rowsNeeded = () => {
    let rows = 0;
    for (let left = PILE.length; left > 0; rows += 1) left -= capacity(rows);
    return rows;
  };
  while (spacing > 12 && rowsNeeded() > maxRows) spacing -= 2;
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
    const pose = {
      x,
      y: bottom - 16 - row * rise + (rand(index, 2) - 0.5) * 8,
      r: (rand(index, 3) - 0.5) * 32,
    };
    poses.push(keepInside(g, bottom, pose, sizes[index] ?? { w: 120, h: 28 }));
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

// Flies the element through `path` to `to`. It starts from wherever the element is now, so a
// flight can interrupt another one.
export function fly(
  element: HTMLElement | null,
  path: Keyframe[],
  to: Pose,
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
  const animation = element.animate([start, ...path, frame(to)], {
    fill: "forwards",
    ...options,
  });
  animation.finished.then(
    () => {
      // Not `commitStyles`: at the end of a flight WebKit can commit the first keyframe, and
      // the chip snaps back to where it started.
      place(element, to);
      onDone?.();
    },
    () => undefined,
  );
}

export function usePileStage() {
  const stageRef = useRef<HTMLDivElement>(null);
  const chipRefs = useRef<Array<HTMLSpanElement | null>>([]);
  const mascotRef = useRef<HTMLButtonElement>(null);
  const [size, setSize] = useState<StageSize | null>(null);

  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const measure = () => {
      const { left, width, height } = stage.getBoundingClientRect();
      const centered = Math.abs(left + width / 2 - window.innerWidth / 2) < 24;
      setSize((current) =>
        current && current.w === width && current.h === height && current.centered === centered
          ? current
          : { w: width, h: height, centered },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, []);

  return { stageRef, chipRefs, mascotRef, size };
}

export function PileChipView({
  chip,
  sorted,
  narrow,
  chipRef,
}: {
  chip: PileChip;
  sorted: boolean;
  narrow: boolean;
  chipRef: Ref<HTMLSpanElement>;
}) {
  // In the sorted list the folder is noise: the list answers which skills, not where.
  const showFolder = !narrow && !(sorted && chip.inUse);
  const status = sorted ? "fine" : chip.status;
  return (
    <span
      ref={chipRef}
      aria-hidden="true"
      {...stylex.props(styles.chip, narrow && styles.chipNarrow)}
    >
      <span {...stylex.props(styles.dot, styles[status])} />
      {chip.name}
      {showFolder && <span {...stylex.props(styles.meta)}>{chip.folder}</span>}
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

/** Gives the skill pile a caption and announces when the mascot finishes sorting. */
export function PileFrame({
  stageRef,
  ready,
  sorted,
  children,
}: {
  stageRef: Ref<HTMLDivElement>;
  ready: boolean;
  sorted: boolean;
  children: ReactNode;
}) {
  return (
    <div {...stylex.props(styles.frame)}>
      <div ref={stageRef} {...stylex.props(styles.stage, !ready && styles.stageHidden)}>
        {children}
      </div>
      <p aria-live="polite" {...stylex.props(styles.cleanupResult)}>
        {sorted &&
          `${PILE_CLEANUP.parked} parked · ${PILE_CLEANUP.fixed} fixed · ${PILE_CLEANUP.updated} updated`}
      </p>
    </div>
  );
}

const styles = stylex.create({
  cleanupResult: {
    color: siteTokens.text,
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    lineHeight: 1.5,
    margin: "14px 0 0",
    minHeight: 18,
    paddingInline: 12,
    textAlign: { default: "left", "@media (max-width: 1080px)": "center" },
  },
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
    transition: {
      default: "background-color 240ms ease-out 360ms",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    width: 7,
  },
  fine: { backgroundColor: "oklch(0.696 0.149 162.48)" },
  warning: { backgroundColor: productDemoTokens.warning },
  broken: { backgroundColor: productDemoTokens.danger },
  meta: {
    color: siteTokens.muted,
    fontFamily: "-apple-system, BlinkMacSystemFont, sans-serif",
    fontSize: 11,
  },
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
});
