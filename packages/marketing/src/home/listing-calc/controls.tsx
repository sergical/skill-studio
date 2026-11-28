// Calculator controls. Keyboard and pointer both work: the radio group moves with arrow keys,
// the slider takes arrows, Page keys, Home and End.
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";
import * as stylex from "@stylexjs/stylex";
import { Info } from "lucide-react";

import { siteTokens } from "../../SiteTheme.stylex";

const HOVER = "@media (hover: hover) and (pointer: fine)";
const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";
const SPRINGY = "cubic-bezier(0.34, 1.56, 0.64, 1)";
const hintPopIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px) scale(0.97)" },
  to: { opacity: 1, transform: "none" },
});
// First open waits; once one tip is open, the next opens at once until the group goes quiet.
const HINT_OPEN_MS = 500;
const HINT_GROUP_IDLE_MS = 300;
// iOS fires an emulated mouse pointerenter right after a touch tap; ignore it within 50ms.
const TOUCH_MOUSE_GUARD_MS = 50;

let hintGroupOpen = false;
let hintGroupIdle: ReturnType<typeof setTimeout> | null = null;
let lastTouchEndAt = 0;

function markHintGroupOpen() {
  hintGroupOpen = true;
  if (hintGroupIdle) {
    clearTimeout(hintGroupIdle);
    hintGroupIdle = null;
  }
}

function markHintGroupClosed() {
  if (hintGroupIdle) clearTimeout(hintGroupIdle);
  hintGroupIdle = setTimeout(() => {
    hintGroupOpen = false;
    hintGroupIdle = null;
  }, HINT_GROUP_IDLE_MS);
}

/** Footnotes stay behind this icon so the panel only shows the control. */
export function InfoHint({
  label,
  align = "center",
  children,
}: {
  label: string;
  align?: "center" | "end";
  children: ReactNode;
}) {
  const id = useId();
  const wrapRef = useRef<HTMLSpanElement>(null);
  const openTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [open, setOpen] = useState(false);
  const [instant, setInstant] = useState(false);

  const show = useCallback((immediate = false) => {
    markHintGroupOpen();
    setInstant(immediate);
    setOpen(true);
  }, []);

  const hide = useCallback(() => {
    setOpen(false);
    setInstant(false);
    markHintGroupClosed();
  }, []);

  useEffect(
    () => () => {
      if (openTimer.current) clearTimeout(openTimer.current);
    },
    [],
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    const onPointer = (event: globalThis.PointerEvent) => {
      if (event.target instanceof Node && wrapRef.current?.contains(event.target)) return;
      hide();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onPointer);
    };
  }, [open, hide]);

  const clearOpenTimer = () => {
    if (!openTimer.current) return;
    clearTimeout(openTimer.current);
    openTimer.current = null;
  };

  const onPointerEnter = (event: PointerEvent<HTMLSpanElement>) => {
    if (event.pointerType !== "mouse" || performance.now() - lastTouchEndAt < TOUCH_MOUSE_GUARD_MS)
      return;
    clearOpenTimer();
    if (hintGroupOpen) {
      show(true);
      return;
    }
    openTimer.current = setTimeout(() => show(false), HINT_OPEN_MS);
  };

  const onPointerLeave = (event: PointerEvent<HTMLSpanElement>) => {
    if (event.pointerType !== "mouse") return;
    clearOpenTimer();
    hide();
  };

  const onPointerUp = (event: PointerEvent<HTMLSpanElement>) => {
    if (event.pointerType === "touch") lastTouchEndAt = performance.now();
  };

  return (
    <span
      ref={wrapRef}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      onPointerUp={onPointerUp}
      {...stylex.props(styles.hintWrap)}
    >
      <button
        type="button"
        aria-label={label}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => (open ? hide() : show(false))}
        {...stylex.props(styles.hintButton)}
      >
        <Info aria-hidden="true" size={14} strokeWidth={2.25} />
      </button>
      {open && (
        <span
          role="tooltip"
          id={id}
          {...stylex.props(
            styles.hintPopup,
            align === "end" ? styles.hintPopupEnd : styles.hintPopupCenter,
            instant && styles.hintPopupInstant,
          )}
        >
          {children}
        </span>
      )}
    </span>
  );
}

interface Option<T> {
  value: T;
  label: ReactNode;
}

const STEP_KEYS = new Map([
  ["ArrowRight", 1],
  ["ArrowDown", 1],
  ["ArrowLeft", -1],
  ["ArrowUp", -1],
]);

export function Segmented<T extends string | number>({
  label,
  options,
  value,
  onChange,
  sliding = false,
  size = "regular",
  equalWidth = false,
}: {
  label: string;
  options: ReadonlyArray<Option<T>>;
  value: T;
  onChange: (value: T) => void;
  sliding?: boolean;
  size?: "regular" | "compact";
  /** Give every option the same width (Claude Code / Codex agent switch). */
  equalWidth?: boolean;
}) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const highlight = useRef<HTMLSpanElement>(null);
  const viaKeyboard = useRef(false);
  const [ready, setReady] = useState(false);
  const index = options.findIndex((option) => option.value === value);

  useLayoutEffect(() => {
    const el = refs.current[index];
    const mark = highlight.current;
    if (!sliding || !el || !mark) return;
    // A pointer choice slides; an arrow key lands at once.
    mark.style.transition = viaKeyboard.current ? "none" : "";
    viaKeyboard.current = false;
    mark.style.width = `${el.offsetWidth}px`;
    mark.style.transform = `translateX(${el.offsetLeft}px)`;
  });

  useEffect(() => {
    if (!sliding) return;
    const frame = requestAnimationFrame(() => setReady(true));
    return () => cancelAnimationFrame(frame);
  }, [sliding]);

  const onKeyDown = (event: KeyboardEvent) => {
    const step = STEP_KEYS.get(event.key);
    if (!step) return;
    event.preventDefault();
    viaKeyboard.current = true;
    const next = (index + step + options.length) % options.length;
    onChange(options[next].value);
    refs.current[next]?.focus();
  };

  return (
    <div
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      {...stylex.props(
        styles.group,
        size === "compact" && styles.groupCompact,
        equalWidth && styles.groupEqual,
      )}
    >
      {sliding && (
        <span
          ref={highlight}
          aria-hidden="true"
          {...stylex.props(styles.highlight, ready && styles.highlightReady)}
        />
      )}
      {options.map((option, i) => (
        <button
          key={String(option.value)}
          ref={(el) => {
            refs.current[i] = el;
          }}
          type="button"
          role="radio"
          aria-checked={i === index}
          tabIndex={i === index ? 0 : -1}
          onClick={() => onChange(option.value)}
          {...stylex.props(
            styles.option,
            size === "compact" && styles.optionCompact,
            equalWidth && styles.optionEqual,
            i === index && (sliding ? styles.optionOnText : styles.optionOn),
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Slider({
  label,
  value,
  valueText,
  min,
  max,
  step = 1,
  onChange,
  springy = false,
  marks,
  hideHeading = false,
  hint,
}: {
  label: string;
  value: number;
  valueText: string;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  /** Detent sliders let the thumb spring to each stop. */
  springy?: boolean;
  marks?: ReadonlyArray<string>;
  hideHeading?: boolean;
  /** Optional footnote icon next to the label (limits, unit math). */
  hint?: ReactNode;
}) {
  const labelId = useId();
  const frame = useRef(0);
  const bounds = useRef({ left: 0, width: 1 });
  const pending = useRef(value);
  const pointerDown = useRef(false);
  const [dragging, setDragging] = useState(false);
  const [keyboardFocus, setKeyboardFocus] = useState(false);
  // Detents spring under the pointer; a key press lands at once.
  const [byPointer, setByPointer] = useState(false);
  const spring = springy && byPointer;
  const position = (value - min) / (max - min);
  const round = (next: number) =>
    Math.min(max, Math.max(min, Math.round((next - min) / step) * step + min));
  const valueAt = (proportion: number) =>
    round(min + (max - min) * Math.max(0, Math.min(1, proportion)));
  const update = (next: number) => {
    if (next !== value) onChange(next);
  };

  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const pointerValue = (clientX: number) => {
    const { left, width } = bounds.current;
    return valueAt((clientX - left) / width);
  };

  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const slider = event.currentTarget;
    const box = slider.getBoundingClientRect();
    bounds.current = { left: box.left + 11, width: Math.max(1, box.width - 22) };
    // Keep keyboard focus on the slider so arrows work right after a click, without showing a
    // keyboard focus ring during pointer drags.
    pointerDown.current = true;
    slider.focus({ preventScroll: true });
    pointerDown.current = false;
    setKeyboardFocus(false);
    slider.setPointerCapture(event.pointerId);
    setDragging(true);
    setByPointer(true);
    update(pointerValue(event.clientX));
  };

  const moveDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    pending.current = pointerValue(event.clientX);
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      onChange(pending.current);
    });
  };

  const endDrag = () => {
    setDragging(false);
    if (!frame.current) return;
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    update(pending.current);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const keys = new Map([
      ["ArrowRight", step],
      ["ArrowUp", step],
      ["ArrowLeft", -step],
      ["ArrowDown", -step],
      ["PageUp", Math.max(step, (max - min) / 10)],
      ["PageDown", -Math.max(step, (max - min) / 10)],
    ]);
    const offset = keys.get(event.key);
    if (offset === undefined && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    setByPointer(false);
    setKeyboardFocus(true);
    if (offset === undefined) update(event.key === "Home" ? min : max);
    else update(round(value + offset));
  };

  return (
    <div>
      <div {...stylex.props(styles.sliderHeading, hideHeading && styles.visuallyHidden)}>
        <span {...stylex.props(styles.sliderLabel)}>
          <span id={labelId}>{label}</span>
          {hint}
        </span>
        <span {...stylex.props(styles.sliderValue)}>{valueText}</span>
      </div>
      <div
        role="slider"
        tabIndex={0}
        aria-labelledby={labelId}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={valueText}
        onFocus={() => {
          if (!pointerDown.current) setKeyboardFocus(true);
        }}
        onBlur={() => setKeyboardFocus(false)}
        onKeyDown={onKeyDown}
        onPointerDown={startDrag}
        onPointerMove={moveDrag}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        {...stylex.props(stylex.defaultMarker(), styles.slider)}
      >
        <span {...stylex.props(styles.track)}>
          <span
            {...stylex.props(styles.fill, spring && styles.springy, styles.fillScale(position))}
          />
          {marks?.map((_, i) => (
            <span
              key={i}
              aria-hidden="true"
              {...stylex.props(styles.tick, styles.tickAt((i / (marks.length - 1)) * 100))}
            />
          ))}
          <span
            {...stylex.props(
              styles.thumbLocation,
              spring && styles.springy,
              styles.thumbAt(position),
            )}
          >
            <span
              {...stylex.props(
                styles.thumb,
                dragging && styles.thumbPressed,
                keyboardFocus && styles.thumbFocused,
              )}
            />
          </span>
        </span>
      </div>
      {marks && (
        <div aria-hidden="true" {...stylex.props(styles.marks)}>
          {marks.map(
            (mark, i) =>
              mark && (
                <span
                  key={i}
                  {...stylex.props(
                    styles.mark,
                    styles.markAt((i / (marks.length - 1)) * 100),
                    i === 0
                      ? styles.markStart
                      : i === marks.length - 1
                        ? styles.markEnd
                        : styles.markMid,
                    i === Math.round(value - min) && styles.markOn,
                  )}
                >
                  {mark}
                </span>
              ),
          )}
        </div>
      )}
    </div>
  );
}

export function Switch({
  label,
  hint,
  checked,
  onChange,
}: {
  label: ReactNode;
  hint?: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const id = useId();
  return (
    <div {...stylex.props(styles.switchRow)}>
      <span {...stylex.props(styles.sliderLabel)}>
        <label htmlFor={id} {...stylex.props(styles.switchLabel)}>
          {label}
        </label>
        {hint}
      </span>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        {...stylex.props(styles.switchButton)}
      >
        <span
          aria-hidden="true"
          {...stylex.props(styles.switchTrack, checked && styles.switchTrackOn)}
        >
          <span {...stylex.props(styles.switchKnob, checked && styles.switchKnobOn)} />
        </span>
      </button>
    </div>
  );
}

const styles = stylex.create({
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
  hintWrap: { display: "inline-flex", position: "relative", verticalAlign: "middle" },
  hintButton: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 999,
    borderWidth: 0,
    color: siteTokens.muted,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    height: 24,
    justifyContent: "center",
    margin: 0,
    padding: 0,
    position: "relative",
    touchAction: "manipulation",
    transition: "color 150ms ease-out",
    width: 24,
    // 24px box meets WCAG 2.5.8 on its own; the hit area extends to 40px (emil-ui-polish).
    "::before": { content: '""', inset: -8, position: "absolute" },
    ":hover": { color: siteTokens.text },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  hintPopup: {
    animationDuration: "140ms",
    animationName: { default: hintPopIn, "@media (prefers-reduced-motion: reduce)": "none" },
    animationTimingFunction: "ease-out",
    backgroundColor: siteTokens.background,
    borderRadius: 10,
    bottom: "calc(100% + 8px)",
    boxShadow: `0 0 0 1px ${siteTokens.border}, 0 12px 32px oklch(0 0 0 / .32)`,
    color: siteTokens.muted,
    display: "flex",
    flexDirection: "column",
    fontSize: 12,
    fontWeight: 500,
    gap: 6,
    lineHeight: 1.45,
    maxWidth: 248,
    padding: "10px 12px",
    position: "absolute",
    whiteSpace: "normal",
    width: "max-content",
    zIndex: 10,
  },
  hintPopupCenter: { left: "50%", transformOrigin: "bottom center", translate: "-50% 0" },
  hintPopupEnd: { right: -8, transformOrigin: "bottom right" },
  hintPopupInstant: { animationName: "none" },
  sliderLabel: {
    alignItems: "center",
    display: "inline-flex",
    gap: 4,
    minWidth: 0,
  },
  group: {
    backgroundColor: siteTokens.background,
    borderColor: siteTokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "inline-flex",
    gap: 2,
    padding: 3,
    position: "relative",
  },
  // Grid, not flex: label length ("Claude Code" vs "Codex") must not change column width.
  groupEqual: {
    display: "grid",
    gridTemplateColumns: "1fr 1fr",
    width: "100%",
  },
  groupCompact: { borderRadius: 9 },
  highlight: {
    backgroundColor: siteTokens.text,
    borderRadius: 7,
    bottom: 3,
    left: 0,
    position: "absolute",
    top: 3,
  },
  highlightReady: {
    transition: {
      default: `transform 260ms ${SETTLE}, width 260ms ${SETTLE}`,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  option: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 7,
    borderWidth: 0,
    color: siteTokens.muted,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 14,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 600,
    gap: 8,
    justifyContent: "center",
    minHeight: { default: 38, "@media (pointer: coarse)": 44 },
    paddingInline: 14,
    position: "relative",
    touchAction: "manipulation",
    transition: "color 150ms ease-out, background-color 150ms ease-out",
    whiteSpace: "nowrap",
    ":hover": { color: siteTokens.text },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  optionCompact: {
    fontSize: 13,
    minHeight: { default: 34, "@media (pointer: coarse)": 44 },
    paddingInline: 11,
  },
  optionEqual: { minWidth: 0, width: "100%", paddingInline: 8 },
  optionOn: {
    backgroundColor: siteTokens.text,
    color: siteTokens.background,
    ":hover": { color: siteTokens.background },
  },
  optionOnText: { color: siteTokens.background, ":hover": { color: siteTokens.background } },
  sliderHeading: {
    alignItems: "baseline",
    color: siteTokens.text,
    display: "flex",
    fontSize: 14,
    fontWeight: 600,
    gap: 8,
    justifyContent: "space-between",
  },
  sliderValue: {
    color: siteTokens.muted,
    fontVariantNumeric: "tabular-nums",
    whiteSpace: "nowrap",
  },
  slider: {
    alignItems: "center",
    borderRadius: 8,
    cursor: "pointer",
    display: "flex",
    minHeight: 44,
    outline: "none",
    paddingInline: 11,
    touchAction: "pan-y",
    userSelect: "none",
    // Keyboard focus renders as a circular ring on the thumb itself so the 44px touch box never
    // overlaps the slider heading above.
    ":focus-visible": { outline: "none" },
  },
  track: {
    backgroundColor: siteTokens.border,
    borderRadius: 999,
    height: 6,
    position: "relative",
    width: "100%",
  },
  fill: {
    backgroundColor: siteTokens.text,
    borderRadius: 999,
    inset: 0,
    position: "absolute",
    transformOrigin: "left",
  },
  fillScale: (position: number) => ({ transform: `scaleX(${position})` }),
  springy: {
    transition: {
      default: `transform 320ms ${SPRINGY}`,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  tick: {
    backgroundColor: siteTokens.muted,
    borderRadius: 999,
    height: 6,
    marginLeft: -1,
    opacity: 0.5,
    position: "absolute",
    top: 0,
    width: 2,
  },
  tickAt: (percent: number) => ({ left: `${percent}%` }),
  thumbLocation: { inset: 0, pointerEvents: "none", position: "absolute" },
  thumbAt: (position: number) => ({ transform: `translateX(${position * 100}%)` }),
  thumb: {
    backgroundColor: siteTokens.background,
    borderColor: siteTokens.text,
    borderRadius: "50%",
    borderStyle: "solid",
    borderWidth: 2,
    boxShadow: "0 1px 3px oklch(0 0 0 / .3)",
    height: 20,
    left: -10,
    position: "absolute",
    top: "50%",
    transform: {
      default: "translateY(-50%)",
      [HOVER]: {
        [stylex.when.ancestor(":hover")]: "translateY(-50%) scale(1.08)",
      },
    },
    transition: {
      default: `transform 160ms ${SETTLE}, box-shadow 150ms ease-out`,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    width: 20,
  },
  thumbPressed: {
    transform: "translateY(-50%) scale(1.15)",
    transition: {
      default: "box-shadow 150ms ease-out",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  thumbFocused: {
    boxShadow: `0 0 0 2px ${siteTokens.background}, 0 0 0 4px ${siteTokens.text}`,
  },
  marks: {
    color: siteTokens.muted,
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    height: 18,
    marginInline: 11,
    marginTop: -6,
    position: "relative",
  },
  mark: {
    fontWeight: 500,
    position: "absolute",
    top: 0,
    transition: "color 150ms ease-out",
    whiteSpace: "nowrap",
  },
  markAt: (percent: number) => ({ left: `${percent}%` }),
  markStart: { transform: "none" },
  markMid: { transform: "translateX(-50%)" },
  markEnd: { transform: "translateX(-100%)" },
  // Constant font-weight on selection avoids reflow; color alone signals active stop (emil-ui-polish).
  markOn: { color: siteTokens.text },
  switchRow: {
    alignItems: "center",
    display: "flex",
    gap: 12,
    justifyContent: "space-between",
    minHeight: 36,
    width: "100%",
  },
  switchLabel: {
    color: siteTokens.text,
    cursor: "pointer",
    fontSize: 14,
    fontWeight: 600,
    userSelect: "none",
  },
  switchButton: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 999,
    borderWidth: 0,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    justifyContent: "center",
    padding: 0,
    position: "relative",
    touchAction: "manipulation",
    // 38x22 visual track with >=44px vertical touch target (emil-ui-polish).
    "::before": { content: '""', inset: "-11px -4px", position: "absolute" },
    ":focus-visible": {
      outline: `2px solid ${siteTokens.muted}`,
      outlineOffset: 3,
    },
  },
  switchTrack: {
    backgroundColor: siteTokens.border,
    borderRadius: 999,
    display: "inline-flex",
    flexShrink: 0,
    height: 22,
    padding: 2,
    transition: "background-color 150ms ease-out",
    width: 38,
  },
  switchTrackOn: { backgroundColor: siteTokens.text },
  switchKnob: {
    backgroundColor: siteTokens.background,
    borderRadius: 999,
    height: 18,
    transition: {
      default: `transform 180ms ${SETTLE}`,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    width: 18,
  },
  switchKnobOn: { transform: "translateX(16px)" },
});
