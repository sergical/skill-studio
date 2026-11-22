import { useEffect, useId, useRef, type KeyboardEvent, type PointerEvent } from "react";
import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";

interface BudgetSliderProps {
  label: string;
  value: number;
  valueText: string;
  min: number;
  max: number;
  step?: number;
  curve?: number;
  onChange: (value: number) => void;
}

/** A keyboard and pointer slider for the marketing budget calculator. */
export function BudgetSlider({
  label,
  value,
  valueText,
  min,
  max,
  step = 1,
  curve = 1,
  onChange,
}: BudgetSliderProps) {
  const labelId = useId();
  const frame = useRef(0);
  const bounds = useRef({ left: 0, width: 1 });
  const pending = useRef(value);
  const position = ((value - min) / (max - min)) ** (1 / curve);
  const round = (next: number) =>
    Math.min(max, Math.max(min, Number((Math.round(next / step) * step).toFixed(4))));
  const update = (next: number) => {
    if (next !== value) onChange(next);
  };

  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  function pointerValue(clientX: number) {
    const { left, width } = bounds.current;
    const proportion = Math.max(0, Math.min(1, (clientX - left) / width));
    return round(min + (max - min) * proportion ** curve);
  }

  function startDrag(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    const slider = event.currentTarget;
    const box = slider.getBoundingClientRect();
    bounds.current = { left: box.left + 9, width: Math.max(1, box.width - 18) };
    slider.focus({ preventScroll: true });
    slider.setPointerCapture(event.pointerId);
    update(pointerValue(event.clientX));
  }

  function moveDrag(event: PointerEvent<HTMLDivElement>) {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    pending.current = pointerValue(event.clientX);
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      onChange(pending.current);
    });
  }

  function endDrag() {
    if (!frame.current) return;
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    update(pending.current);
  }

  function changeByKey(event: KeyboardEvent<HTMLDivElement>) {
    const values = new Map([
      ["ArrowRight", value + step],
      ["ArrowUp", value + step],
      ["ArrowLeft", value - step],
      ["ArrowDown", value - step],
      ["PageUp", value + step * 10],
      ["PageDown", value - step * 10],
      ["Home", min],
      ["End", max],
    ]);
    const next = values.get(event.key);
    if (next === undefined) return;
    event.preventDefault();
    update(round(next));
  }

  return (
    <div>
      <div {...stylex.props(styles.heading)}>
        <span id={labelId}>{label}</span>
        <span {...stylex.props(styles.value)}>{valueText}</span>
      </div>
      <div
        role="slider"
        tabIndex={0}
        aria-labelledby={labelId}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={valueText}
        onKeyDown={changeByKey}
        onPointerDown={startDrag}
        onPointerMove={moveDrag}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        {...stylex.props(styles.slider)}
      >
        <span {...stylex.props(styles.track)}>
          <span {...stylex.props(styles.fill, styles.scale(position))} />
          <span {...stylex.props(styles.thumbLocation, styles.position(position))}>
            <span {...stylex.props(styles.thumb)} />
          </span>
        </span>
      </div>
    </div>
  );
}

const styles = stylex.create({
  heading: {
    alignItems: "baseline",
    display: "flex",
    fontSize: 14,
    fontWeight: 590,
    gap: 8,
    justifyContent: "space-between",
  },
  value: { color: siteTokens.muted, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" },
  slider: {
    alignItems: "center",
    cursor: "pointer",
    display: "flex",
    minHeight: 44,
    paddingInline: 9,
    touchAction: "pan-y",
    userSelect: "none",
    ":focus-visible": {
      borderRadius: 6,
      outline: `2px solid ${siteTokens.accent}`,
      outlineOffset: 2,
    },
  },
  track: {
    backgroundColor: siteTokens.border,
    borderRadius: 999,
    height: 5,
    position: "relative",
    width: "100%",
  },
  fill: {
    backgroundColor: siteTokens.accent,
    borderRadius: 999,
    inset: 0,
    position: "absolute",
    transformOrigin: "left",
  },
  scale: (position: number) => ({ transform: `scaleX(${position})` }),
  thumbLocation: { inset: 0, position: "absolute" },
  position: (position: number) => ({ transform: `translateX(${position * 100}%)` }),
  thumb: {
    backgroundColor: siteTokens.surface,
    border: `2px solid ${siteTokens.accent}`,
    borderRadius: "50%",
    height: 18,
    left: -9,
    position: "absolute",
    top: "50%",
    transform: "translateY(-50%)",
    width: 18,
  },
});
