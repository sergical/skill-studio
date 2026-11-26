// PROTOTYPE (throwaway): "Field" - every skill is a dot, by uses (right, log) and the room its
// description takes in every message (up). Zoom with the wheel or pinch, drag to pan, drag
// with Shift to box-zoom, double-click to reset; a count chip zooms to its skills. Names show
// once few enough dots are in view. With `list`, a panel lists the skills in view.

import { useEffect, useEffectEvent, useState } from "react";
import type { PointerEvent } from "react";
import { formatCount } from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { Button, Input } from "@skill-studio/ui";
import {
  FocusStrip,
  Key,
  Legend,
  Ring,
  SkillCard,
  Swatch,
  UpdateDot,
  matchesFocus,
  useWidth,
} from "./shared";
import type { Focus } from "./shared";
import type { LibrarySkill } from "./library-model";

const H = 420;
const PAD = { top: 24, right: 16, bottom: 28, left: 48 };

interface Domain {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}

function hash(text: string) {
  let h = 0;
  for (const c of text) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h);
}

/** Data x: never-used skills spread over [0, 0.4]; used ones at 0.6 + log10(uses). */
const dataX = (s: LibrarySkill) =>
  s.uses ? 0.6 + Math.log10(s.uses) : ((hash(s.name) % 97) / 97) * 0.4;
const dataY = (s: LibrarySkill) => s.tokens + (((hash(s.name) >> 7) % 9) - 4) * 0.6;

export function FieldVariant({
  skills,
  history,
  open,
  select,
  list = false,
}: {
  skills: LibrarySkill[];
  history: boolean;
  open: (name: string) => void;
  select: (skills: InstalledSkill[]) => void;
  list?: boolean;
}) {
  const { ref, width } = useWidth<HTMLDivElement>();
  const [focus, setFocus] = useState<Focus>("all");
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const full: Domain = {
    x0: -0.05,
    x1: 0.6 + Math.log10(Math.max(10, ...skills.map((s) => s.uses))) + 0.1,
    y0: 0,
    y1: Math.max(10, ...skills.map((s) => s.tokens)) * 1.08,
  };
  const [domain, setDomain] = useState<Domain>(full);
  const [gesture, setGesture] = useState<
    | { kind: "pan"; at: { x: number; y: number }; from: Domain }
    | { kind: "box"; x0: number; y0: number; x1: number; y1: number }
    | null
  >(null);
  const plotW = Math.max(1, width - PAD.left - PAD.right);
  const plotH = H - PAD.top - PAD.bottom;
  const sx = (v: number) => PAD.left + ((v - domain.x0) / (domain.x1 - domain.x0)) * plotW;
  const sy = (v: number) => PAD.top + (1 - (v - domain.y0) / (domain.y1 - domain.y0)) * plotH;
  const ix = (px: number) => domain.x0 + ((px - PAD.left) / plotW) * (domain.x1 - domain.x0);
  const iy = (py: number) => domain.y0 + (1 - (py - PAD.top) / plotH) * (domain.y1 - domain.y0);
  const needle = query.trim().toLowerCase();
  const points = skills.map((s) => ({ s, x: sx(dataX(s)), y: sy(dataY(s)) }));
  const inView = points.filter(
    (p) => p.x >= PAD.left && p.x <= width - PAD.right && p.y >= PAD.top && p.y <= H - PAD.bottom,
  );
  const lit = (s: LibrarySkill) =>
    matchesFocus(s, focus) && (!needle || s.name.toLowerCase().includes(needle));
  const shown = inView.filter((p) => lit(p.s));
  const zoomed =
    domain.x0 !== full.x0 ||
    domain.x1 !== full.x1 ||
    domain.y0 !== full.y0 ||
    domain.y1 !== full.y1;
  const current = points.find((p) => p.s.name === active) ?? null;
  // Names go next to dots greedily, biggest first, skipping any that would overlap one already
  // placed, so a dense cluster shows a few names instead of a smear.
  const labeled = new Set<string>();
  const taken: { x0: number; x1: number; y0: number; y1: number }[] = [];
  if (shown.length <= 80)
    for (const p of [...shown].sort((a, b) => b.s.tokens - a.s.tokens)) {
      const r = { x0: p.x + 8, x1: p.x + 12 + p.s.name.length * 6.2, y0: p.y - 7, y1: p.y + 7 };
      if (r.x1 > width - PAD.right) continue;
      if (taken.some((t) => r.x0 < t.x1 && r.x1 > t.x0 && r.y0 < t.y1 && r.y1 > t.y0)) continue;
      if (shown.some((q) => q !== p && q.x > r.x0 && q.x < r.x1 && Math.abs(q.y - p.y) < 6))
        continue;
      taken.push(r);
      labeled.add(p.s.name);
    }

  /** Zooms to the box around `items`, with a margin, or back out when there are none. */
  const fit = (items: LibrarySkill[]) => {
    if (!items.length) return setDomain(full);
    const xs = items.map(dataX);
    const ys = items.map(dataY);
    const mx = Math.max(0.15, (Math.max(...xs) - Math.min(...xs)) * 0.15);
    const my = Math.max(4, (Math.max(...ys) - Math.min(...ys)) * 0.15);
    setDomain({
      x0: Math.min(...xs) - mx,
      x1: Math.max(...xs) + mx,
      y0: Math.max(0, Math.min(...ys) - my),
      y1: Math.max(...ys) + my,
    });
  };
  const point = (e: PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const nearest = (at: { x: number; y: number }) => {
    let best: string | null = null;
    let dist = 100;
    for (const p of inView) {
      const d = (p.x - at.x) ** 2 + (p.y - at.y) ** 2;
      if (d < dist) [best, dist] = [p.s.name, d];
    }
    return best;
  };
  /** Zooms by `factor` around the pixel `at`, never past the full view or below a few skills. */
  const zoomAt = (at: { x: number; y: number }, factor: number) => {
    const fx = (at.x - PAD.left) / plotW;
    const fy = 1 - (at.y - PAD.top) / plotH;
    setDomain((d) => {
      const w = Math.min(full.x1 - full.x0, Math.max(0.05, (d.x1 - d.x0) * factor));
      const h = Math.min(full.y1 - full.y0, Math.max(2, (d.y1 - d.y0) * factor));
      const cx = d.x0 + fx * (d.x1 - d.x0);
      const cy = d.y0 + fy * (d.y1 - d.y0);
      const y0 = Math.max(full.y0, cy - fy * h);
      return { x0: cx - fx * w, x1: cx - fx * w + w, y0, y1: y0 + h };
    });
  };
  // React's wheel listener is passive, so it can't stop the page scrolling; this one can.
  const handleWheel = useEffectEvent((e: WheelEvent) => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    e.preventDefault();
    zoomAt({ x: e.clientX - r.left, y: e.clientY - r.top }, Math.exp(e.deltaY * 0.002));
  });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.addEventListener("wheel", handleWheel, { passive: false });
    return () => el.removeEventListener("wheel", handleWheel);
  }, [ref]);
  const xTicks = [
    { v: 0.2, label: "never" },
    ...[1, 10, 100, 1000, 10000].map((t) => ({ v: 0.6 + Math.log10(t), label: formatCount(t) })),
  ].filter((t) => t.v >= domain.x0 && t.v <= domain.x1);
  const ySpan = domain.y1 - domain.y0;
  const yStep = [1, 2, 5, 10, 20, 50, 100, 200, 500].find((s) => ySpan / s <= 5) ?? 1000;
  const yTicks: number[] = [];
  for (let v = Math.ceil(domain.y0 / yStep) * yStep; v <= domain.y1; v += yStep) yTicks.push(v);

  const chart = (
    <div ref={ref} className="relative min-w-0 flex-1">
      {width > 0 && (
        <svg
          width={width}
          height={H}
          tabIndex={0}
          data-proto-keys
          role="application"
          aria-label="Skills by uses in 30 days and description size. Arrow keys pan, plus and minus zoom, 0 resets, Enter opens the skill under the card."
          className="block cursor-grab touch-none rounded-md outline-offset-2 select-none focus-visible:outline-2 focus-visible:outline-text-tertiary active:cursor-grabbing"
          onDoubleClick={() => setDomain(full)}
          onPointerDown={(e) => {
            const at = point(e);
            const hit = nearest(at);
            if (hit && !e.shiftKey) return open(hit);
            e.currentTarget.setPointerCapture(e.pointerId);
            setGesture(
              e.shiftKey
                ? { kind: "box", x0: at.x, y0: at.y, x1: at.x, y1: at.y }
                : { kind: "pan", at, from: domain },
            );
          }}
          onPointerMove={(e) => {
            const at = point(e);
            if (gesture?.kind === "pan") {
              const dx = ((at.x - gesture.at.x) / plotW) * (gesture.from.x1 - gesture.from.x0);
              const dy = ((at.y - gesture.at.y) / plotH) * (gesture.from.y1 - gesture.from.y0);
              setDomain({
                x0: gesture.from.x0 - dx,
                x1: gesture.from.x1 - dx,
                y0: gesture.from.y0 + dy,
                y1: gesture.from.y1 + dy,
              });
            } else if (gesture?.kind === "box") setGesture({ ...gesture, x1: at.x, y1: at.y });
            else setActive(nearest(at));
          }}
          onPointerUp={() => {
            if (gesture?.kind === "box" && Math.abs(gesture.x1 - gesture.x0) > 8) {
              setDomain({
                x0: ix(Math.min(gesture.x0, gesture.x1)),
                x1: ix(Math.max(gesture.x0, gesture.x1)),
                y0: Math.max(0, iy(Math.max(gesture.y0, gesture.y1))),
                y1: iy(Math.min(gesture.y0, gesture.y1)),
              });
            }
            setGesture(null);
          }}
          onPointerLeave={() => !gesture && setActive(null)}
          onKeyDown={(e) => {
            const panX = (domain.x1 - domain.x0) * 0.1;
            const panY = (domain.y1 - domain.y0) * 0.1;
            const move =
              e.key === "ArrowLeft"
                ? { x0: domain.x0 - panX, x1: domain.x1 - panX }
                : e.key === "ArrowRight"
                  ? { x0: domain.x0 + panX, x1: domain.x1 + panX }
                  : e.key === "ArrowUp"
                    ? { y0: domain.y0 + panY, y1: domain.y1 + panY }
                    : e.key === "ArrowDown"
                      ? { y0: Math.max(0, domain.y0 - panY), y1: domain.y1 - panY }
                      : null;
            const center = { x: PAD.left + plotW / 2, y: PAD.top + plotH / 2 };
            if (move) setDomain({ ...domain, ...move });
            else if (e.key === "+" || e.key === "=") zoomAt(center, 0.7);
            else if (e.key === "-") zoomAt(center, 1.4);
            else if (e.key === "0") setDomain(full);
            else if (e.key === "Enter" && current) open(current.s.name);
            else return;
            e.preventDefault();
          }}
        >
          <defs>
            <clipPath id="field-plot">
              <rect x={PAD.left} y={PAD.top} width={plotW} height={plotH} />
            </clipPath>
          </defs>
          {domain.x0 < 0.5 && (
            <rect
              x={sx(Math.max(domain.x0, -0.05))}
              y={PAD.top}
              width={Math.max(0, sx(Math.min(0.45, domain.x1)) - sx(Math.max(domain.x0, -0.05)))}
              height={plotH}
              className="fill-bg-tertiary/50"
              clipPath="url(#field-plot)"
            />
          )}
          {yTicks.map((t) => (
            <g key={t}>
              <line
                x1={PAD.left}
                x2={width - PAD.right}
                y1={sy(t)}
                y2={sy(t)}
                className="stroke-border-subtle"
              />
              <text
                x={PAD.left - 8}
                y={sy(t) + 4}
                textAnchor="end"
                className="fill-text-tertiary text-[11px] tabular-nums"
              >
                {formatCount(t)}
              </text>
            </g>
          ))}
          <text x={PAD.left - 8} y={12} textAnchor="end" className="fill-text-tertiary text-[11px]">
            tokens
          </text>
          {xTicks.map((t, i) => (
            <text
              key={t.label}
              x={sx(t.v)}
              y={H - 8}
              textAnchor="middle"
              className="fill-text-tertiary text-[11px] tabular-nums"
            >
              {t.label}
              {i === xTicks.length - 1 && t.label !== "never" ? " uses" : ""}
            </text>
          ))}
          <g clipPath="url(#field-plot)">
            {points.map((p) => {
              const on = lit(p.s);
              const hot = p.s.name === active;
              return (
                <g key={p.s.name} opacity={on ? 1 : 0.12}>
                  {(p.s.broken || p.s.cut) && (
                    <circle
                      cx={p.x}
                      cy={p.y}
                      r={7.5}
                      fill="none"
                      strokeWidth={1.5}
                      className={p.s.broken ? "stroke-error" : "stroke-warning"}
                    />
                  )}
                  <circle
                    cx={p.x}
                    cy={p.y}
                    r={hot ? 6 : 4}
                    className={p.s.uses ? "fill-data" : "fill-text-tertiary"}
                  />
                  {p.s.update && (
                    <circle cx={p.x + 5} cy={p.y - 5} r={2} className="fill-text-primary" />
                  )}
                  {on && labeled.has(p.s.name) && (
                    <text
                      x={p.x + 10}
                      y={p.y + 4}
                      className="pointer-events-none fill-text-secondary text-[11px]"
                    >
                      {p.s.name}
                    </text>
                  )}
                </g>
              );
            })}
          </g>
          {gesture?.kind === "box" && (
            <rect
              x={Math.min(gesture.x0, gesture.x1)}
              y={Math.min(gesture.y0, gesture.y1)}
              width={Math.abs(gesture.x1 - gesture.x0)}
              height={Math.abs(gesture.y1 - gesture.y0)}
              className="fill-text-primary/5 stroke-text-tertiary"
              strokeDasharray="3 3"
            />
          )}
        </svg>
      )}
      {current && !gesture && (
        <SkillCard
          item={current.s}
          x={Math.min(current.x + 14, width - 264)}
          y={Math.min(current.y + 8, H - 160)}
        />
      )}
      <p aria-live="polite" className="sr-only">
        {`${shown.length} skills in view`}
      </p>
    </div>
  );

  return (
    <section aria-label="Your skills by use and size" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FocusStrip
          skills={skills}
          history={history}
          focus={focus}
          setFocus={(next) => {
            setFocus(next);
            fit(next === "all" ? [] : skills.filter((s) => matchesFocus(s, next)));
          }}
        />
        <Legend>
          <Key mark={<Swatch className="rounded-full bg-data" />}>used</Key>
          <Key mark={<Swatch className="size-2 rounded-full bg-text-tertiary" />}>unused</Key>
          <Key mark={<Ring className="border-error" />}>doesn’t load</Key>
          <Key mark={<Ring className="border-warning" />}>cut</Key>
          <Key mark={<UpdateDot />}>update</Key>
        </Legend>
      </div>
      <div className="flex items-center gap-2">
        <Input
          type="search"
          placeholder="Find a skill"
          aria-label="Find a skill"
          value={query}
          className="h-8 w-56"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") fit(skills.filter((s) => s.name.toLowerCase().includes(needle)));
          }}
        />
        <span className="text-small text-text-secondary tabular-nums">
          {formatCount(shown.length)} in view
        </span>
        <span className="flex-1" />
        {shown.length > 0 && shown.length < skills.length && (
          <Button size="sm" variant="ghost" onClick={() => select(shown.map((p) => p.s.skill))}>
            Select {formatCount(shown.length)}
          </Button>
        )}
        <Button size="sm" variant="outline" disabled={!zoomed} onClick={() => setDomain(full)}>
          Reset zoom
        </Button>
      </div>
      {list ? (
        <div className="flex gap-4">
          {chart}
          <div className="flex h-[420px] w-72 shrink-0 flex-col border-l border-border-subtle pl-3">
            <p className="flex justify-between px-2 pb-1 text-caption text-text-tertiary">
              <span>skill</span>
              <span>uses · tokens</span>
            </p>
            <ul aria-label="Skills in view" className="min-h-0 flex-1 overflow-y-auto">
              {[...shown]
                .sort((a, b) => b.s.tokens - a.s.tokens)
                .map((p) => (
                  <li key={p.s.name}>
                    <Button
                      variant="ghost"
                      className={`grid h-8 w-full grid-cols-[minmax(0,1fr)_auto] gap-2 px-2 font-normal ${p.s.name === active ? "bg-bg-tertiary" : ""}`}
                      onPointerEnter={() => setActive(p.s.name)}
                      onFocus={() => setActive(p.s.name)}
                      onClick={() => open(p.s.name)}
                    >
                      <span
                        className={`truncate text-left ${p.s.broken ? "text-error" : p.s.cut ? "text-warning" : ""}`}
                      >
                        {p.s.name}
                      </span>
                      <span className="text-small text-text-secondary tabular-nums">
                        {formatCount(p.s.uses)} · {formatCount(p.s.tokens)}
                      </span>
                    </Button>
                  </li>
                ))}
            </ul>
          </div>
        </div>
      ) : (
        chart
      )}
    </section>
  );
}
