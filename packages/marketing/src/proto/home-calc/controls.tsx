// PROTO(home-calc): throwaway calculator controls. Keyboard and pointer both work: the radio
// group moves with arrow keys, the slider takes arrows, Page keys, Home and End.
import {
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

import { siteTokens } from "../../SiteTheme.stylex";

const SETTLE = "cubic-bezier(0.32, 0.72, 0, 1)";
const SPRINGY = "cubic-bezier(0.34, 1.56, 0.64, 1)";

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
}) {
  const labelId = useId();
  const frame = useRef(0);
  const bounds = useRef({ left: 0, width: 1 });
  const pending = useRef(value);
  const [dragging, setDragging] = useState(false);
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
    slider.focus({ preventScroll: true });
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
    if (offset === undefined) update(event.key === "Home" ? min : max);
    else update(round(value + offset));
  };

  return (
    <div>
      <div {...stylex.props(styles.sliderHeading, hideHeading && styles.visuallyHidden)}>
        <span id={labelId}>{label}</span>
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
        onKeyDown={onKeyDown}
        onPointerDown={startDrag}
        onPointerMove={moveDrag}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        {...stylex.props(styles.slider)}
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
            <span {...stylex.props(styles.thumb, dragging && styles.thumbPressed)} />
          </span>
        </span>
      </div>
      {marks && (
        // An empty mark is a tick without a label. The labelled stops must be evenly spaced,
        // because the labels are spread across the row, not placed under their ticks.
        <div aria-hidden="true" {...stylex.props(styles.marks)}>
          {marks.map(
            (mark, i) =>
              mark && (
                <span
                  key={i}
                  {...stylex.props(styles.mark, i === Math.round(value - min) && styles.markOn)}
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
  checked,
  onChange,
}: {
  label: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
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
      <span>{label}</span>
    </button>
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
  groupEqual: { display: "flex", width: "100%" },
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
  optionEqual: { flex: 1, minWidth: 0 },
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
    paddingInline: 11,
    touchAction: "pan-y",
    userSelect: "none",
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
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
    transform: "translateY(-50%)",
    transition: {
      default: `transform 160ms ${SETTLE}`,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    width: 20,
  },
  thumbPressed: { transform: "translateY(-50%) scale(1.15)" },
  marks: {
    color: siteTokens.muted,
    display: "flex",
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    justifyContent: "space-between",
    paddingInline: 2,
  },
  mark: { minWidth: 32, textAlign: "center", transition: "color 150ms ease-out" },
  markOn: { color: siteTokens.text, fontWeight: 600 },
  switchButton: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderWidth: 0,
    color: siteTokens.text,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 14,
    fontWeight: 600,
    gap: 10,
    minHeight: 44,
    padding: 0,
    touchAction: "manipulation",
    ":focus-visible": {
      outline: `2px solid ${siteTokens.muted}`,
      outlineOffset: 4,
      borderRadius: 6,
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
