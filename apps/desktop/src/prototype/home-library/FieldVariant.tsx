// PROTOTYPE (throwaway): "Field" - every skill is a dot, by uses (right) and the room its
// description takes in every message (up). Problems ring their dots; the counts above
// highlight a group; drag to select, hover or arrow keys to read one, Enter to open it.

import { useState } from "react";
import type { PointerEvent } from "react";
import { formatCount } from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
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

const H = 340;
const TOP = 28;
const BOTTOM = 28;
const LEFT = 56;
const BAND = 64;

function hash(text: string) {
  let h = 0;
  for (const c of text) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h);
}

export function FieldVariant({
  skills,
  history,
  open,
  select,
}: {
  skills: LibrarySkill[];
  history: boolean;
  open: (name: string) => void;
  select: (skills: InstalledSkill[]) => void;
}) {
  const { ref, width } = useWidth<HTMLDivElement>();
  const [focus, setFocus] = useState<Focus>("all");
  const [active, setActive] = useState<number | null>(null);
  const [box, setBox] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const [drag, setDrag] = useState(false);
  const maxUses = Math.max(1, ...skills.map((s) => s.uses));
  const maxTokens = Math.max(1, ...skills.map((s) => s.tokens));
  const usedLeft = LEFT + BAND + 16;
  const x = (s: LibrarySkill) =>
    s.uses
      ? usedLeft +
        (Math.log10(s.uses) / Math.log10(Math.max(10, maxUses))) * (width - usedLeft - 16)
      : LEFT + 6 + (hash(s.name) % (BAND - 12));
  const y = (s: LibrarySkill) => TOP + (1 - s.tokens / maxTokens) * (H - TOP - BOTTOM);
  const placed = skills
    .map((s) => ({ s, x: x(s), y: y(s) }))
    .sort((a, b) => a.x - b.x || a.y - b.y);
  const inBox = (p: { x: number; y: number }) =>
    box !== null &&
    p.x >= Math.min(box.x0, box.x1) &&
    p.x <= Math.max(box.x0, box.x1) &&
    p.y >= Math.min(box.y0, box.y1) &&
    p.y <= Math.max(box.y0, box.y1);
  const picked = box ? placed.filter(inBox) : [];
  const point = (e: PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const nearest = (at: { x: number; y: number }) => {
    let best: number | null = null;
    let dist = 144;
    placed.forEach((p, i) => {
      const d = (p.x - at.x) ** 2 + (p.y - at.y) ** 2;
      if (d < dist) [best, dist] = [i, d];
    });
    return best;
  };
  const usesTicks = [1, 10, 100, 1000, 10000].filter((t) => t <= Math.max(10, maxUses));
  const tokenTicks = [0, Math.round(maxTokens / 2), maxTokens];
  const current = active === null ? null : placed[active];

  return (
    <section aria-label="Your skills by use and size" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FocusStrip skills={skills} history={history} focus={focus} setFocus={setFocus} />
        <Legend>
          <Key mark={<Swatch className="rounded-full bg-data" />}>used</Key>
          <Key mark={<Swatch className="size-2 rounded-full bg-text-tertiary" />}>unused</Key>
          <Key mark={<Ring className="border-error" />}>doesn’t load</Key>
          <Key mark={<Ring className="border-warning" />}>cut</Key>
          <Key mark={<UpdateDot />}>update</Key>
        </Legend>
      </div>
      <div ref={ref} className="relative">
        {width > 0 && (
          <svg
            width={width}
            height={H}
            tabIndex={0}
            data-proto-keys
            role="application"
            aria-label="Skills by uses in 30 days and description size. Arrow keys move, Enter opens."
            className="block touch-none select-none rounded-md outline-offset-2 focus-visible:outline-2 focus-visible:outline-text-tertiary"
            onPointerDown={(e) => {
              const at = point(e);
              const hit = nearest(at);
              if (hit !== null) return open(placed[hit].s.name);
              setBox({ x0: at.x, y0: at.y, x1: at.x, y1: at.y });
              setDrag(true);
              e.currentTarget.setPointerCapture(e.pointerId);
            }}
            onPointerMove={(e) => {
              const at = point(e);
              if (drag) setBox((b) => b && { ...b, x1: at.x, y1: at.y });
              else setActive(nearest(at));
            }}
            onPointerUp={() => setDrag(false)}
            onPointerLeave={() => !drag && setActive(null)}
            onKeyDown={(e) => {
              const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
              if (step) {
                e.preventDefault();
                setActive((i) => Math.min(placed.length - 1, Math.max(0, (i ?? -1) + step)));
              } else if (e.key === "Enter" && current) open(current.s.name);
              else if (e.key === "Escape") setBox(null);
            }}
          >
            <rect
              x={LEFT}
              y={TOP}
              width={BAND}
              height={H - TOP - BOTTOM}
              rx={6}
              className="fill-bg-tertiary/50"
            />
            {tokenTicks.map((t) => {
              const ty = TOP + (1 - t / maxTokens) * (H - TOP - BOTTOM);
              return (
                <g key={t}>
                  <line x1={LEFT} x2={width - 8} y1={ty} y2={ty} className="stroke-border-subtle" />
                  <text
                    x={LEFT - 8}
                    y={ty + 4}
                    textAnchor="end"
                    className="fill-text-tertiary text-[11px] tabular-nums"
                  >
                    {formatCount(t)}
                  </text>
                </g>
              );
            })}
            <text
              x={LEFT + BAND / 2}
              y={H - 8}
              textAnchor="middle"
              className="fill-text-tertiary text-[11px]"
            >
              Never used
            </text>
            {usesTicks.map((t, ti) => {
              const tx =
                usedLeft +
                (Math.log10(t) / Math.log10(Math.max(10, maxUses))) * (width - usedLeft - 16);
              return (
                <text
                  key={t}
                  x={tx}
                  y={H - 8}
                  textAnchor="middle"
                  className="fill-text-tertiary text-[11px] tabular-nums"
                >
                  {formatCount(t)}
                  {ti === usesTicks.length - 1 ? " uses" : ""}
                </text>
              );
            })}
            <text x={LEFT - 8} y={12} textAnchor="end" className="fill-text-tertiary text-[11px]">
              tokens
            </text>
            {placed.map((p, i) => {
              const lit = matchesFocus(p.s, focus);
              return (
                <g key={p.s.name} opacity={lit ? 1 : 0.15}>
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
                    r={i === active ? 6 : 4}
                    className={`${p.s.uses ? "fill-data" : "fill-text-tertiary"} ${inBox(p) ? "stroke-text-primary" : ""}`}
                    strokeWidth={1.5}
                  />
                  {p.s.update && (
                    <circle cx={p.x + 5} cy={p.y - 5} r={2} className="fill-text-primary" />
                  )}
                </g>
              );
            })}
            {box && (
              <rect
                x={Math.min(box.x0, box.x1)}
                y={Math.min(box.y0, box.y1)}
                width={Math.abs(box.x1 - box.x0)}
                height={Math.abs(box.y1 - box.y0)}
                className="fill-text-primary/5 stroke-text-tertiary"
                strokeDasharray="3 3"
              />
            )}
          </svg>
        )}
        {current && (
          <SkillCard
            item={current.s}
            x={Math.min(current.x + 14, width - 264)}
            y={Math.min(current.y + 8, H - 150)}
          />
        )}
        <p aria-live="polite" className="sr-only">
          {current ? `${current.s.name}, ${current.s.uses} uses, ${current.s.tokens} tokens` : ""}
        </p>
      </div>
      <div className="flex min-h-8 items-center gap-3 text-small text-text-secondary">
        {picked.length ? (
          <>
            <span className="tabular-nums">
              {picked.length} selected ·{" "}
              {formatCount(picked.reduce((sum, p) => sum + p.s.tokens, 0))} tokens in every message
            </span>
            <Button size="sm" onClick={() => select(picked.map((p) => p.s.skill))}>
              Select {picked.length}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setBox(null)}>
              Clear
            </Button>
          </>
        ) : null}
      </div>
    </section>
  );
}
