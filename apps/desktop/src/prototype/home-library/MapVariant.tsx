// PROTOTYPE (throwaway): "Map" - a treemap of every skill, grouped by where it came from.
// A tile's area is the room its description takes in every message; its brightness is how
// much it was used. Problems mark their tiles. Click a source to zoom in, a tile to open.

import { useState } from "react";
import { formatCount } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import {
  FocusStrip,
  Key,
  Legend,
  Ramp,
  Ring,
  SkillCard,
  Swatch,
  UpdateDot,
  matchesFocus,
  useWidth,
} from "./shared";
import type { Focus } from "./shared";
import type { LibrarySkill } from "./library-model";

const H = 380;

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Squarified treemap: lays `values` out in `rect`, rows of near-square cells. */
function squarify(values: number[], rect: Rect): Rect[] {
  const total = values.reduce((a, b) => a + b, 0) || 1;
  const scale = (rect.w * rect.h) / total;
  const out: Rect[] = [];
  let { x, y, w, h } = rect;
  let row: number[] = [];
  const worst = (r: number[], side: number) => {
    const sum = r.reduce((a, b) => a + b, 0) * scale;
    const max = Math.max(...r) * scale;
    const min = Math.min(...r) * scale;
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };
  const flush = () => {
    const sum = row.reduce((a, b) => a + b, 0) * scale;
    if (w >= h) {
      const rw = sum / h;
      let cy = y;
      for (const v of row) {
        const ch = (v * scale) / rw;
        out.push({ x, y: cy, w: rw, h: ch });
        cy += ch;
      }
      x += rw;
      w -= rw;
    } else {
      const rh = sum / w;
      let cx = x;
      for (const v of row) {
        const cw = (v * scale) / rh;
        out.push({ x: cx, y, w: cw, h: rh });
        cx += cw;
      }
      y += rh;
      h -= rh;
    }
    row = [];
  };
  for (const v of values) {
    const side = Math.min(w, h);
    if (row.length && worst([...row, v], side) > worst(row, side)) flush();
    row.push(v);
  }
  if (row.length) flush();
  return out;
}

export function MapVariant({
  skills,
  history,
  open,
}: {
  skills: LibrarySkill[];
  history: boolean;
  open: (name: string) => void;
}) {
  const { ref, width } = useWidth<HTMLDivElement>();
  const [focus, setFocus] = useState<Focus>("all");
  const [zoom, setZoom] = useState<string | null>(null);
  const [hover, setHover] = useState<{ item: LibrarySkill; x: number; y: number } | null>(null);
  const maxUses = Math.max(1, ...skills.map((s) => s.uses));
  const size = (s: LibrarySkill) => Math.max(6, s.tokens);
  const bySource = new Map<string, LibrarySkill[]>();
  for (const s of skills) bySource.set(s.source, [...(bySource.get(s.source) ?? []), s]);
  const groups = [...bySource.entries()]
    .filter(([source]) => zoom === null || source === zoom)
    .map(([source, items]) => ({ source, items: items.sort((a, b) => size(b) - size(a)) }))
    .sort(
      (a, b) => b.items.reduce((t, s) => t + size(s), 0) - a.items.reduce((t, s) => t + size(s), 0),
    );
  const groupRects = width
    ? squarify(
        groups.map((g) => g.items.reduce((t, s) => t + size(s), 0)),
        { x: 0, y: 0, w: width, h: H },
      )
    : [];

  return (
    <section aria-label="Your skills by source" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FocusStrip skills={skills} history={history} focus={focus} setFocus={setFocus} />
        <Legend>
          <Key mark={<Ramp max={maxUses} />}>uses</Key>
          <Key
            mark={
              <span className="inline-flex items-end gap-0.5">
                <Swatch className="size-1.5 bg-text-quaternary" />
                <Swatch className="size-3 bg-text-quaternary" />
              </span>
            }
          >
            tokens
          </Key>
          <Key mark={<Ring className="rounded-[3px] border-error" />}>doesn’t load</Key>
          <Key mark={<Ring className="rounded-[3px] border-warning" />}>cut</Key>
          <Key mark={<UpdateDot />}>update</Key>
        </Legend>
        {zoom && (
          <Button size="sm" variant="ghost" onClick={() => setZoom(null)}>
            ← All sources
          </Button>
        )}
      </div>
      <div
        ref={ref}
        className="relative"
        style={{ height: H }}
        onPointerLeave={() => setHover(null)}
      >
        {groups.map((group, gi) => {
          const g = groupRects[gi];
          if (!g) return null;
          const header = g.h > 40 && g.w > 80 ? 22 : 0;
          const tiles = squarify(group.items.map(size), {
            x: 0,
            y: header,
            w: Math.max(0, g.w - 2),
            h: Math.max(0, g.h - header - 2),
          });
          return (
            <div
              key={group.source}
              className="absolute overflow-hidden rounded-md bg-bg-secondary"
              style={{ left: g.x + 1, top: g.y + 1, width: g.w - 2, height: g.h - 2 }}
            >
              {header > 0 && (
                <Button
                  variant="ghost"
                  className="flex h-[22px] w-full items-center justify-between gap-2 rounded-none px-2 text-left text-caption font-normal text-text-secondary hover:text-text-primary"
                  onClick={() => setZoom(zoom ? null : group.source)}
                >
                  <span className="truncate font-medium">{group.source}</span>
                  <span className="tabular-nums">{group.items.length}</span>
                </Button>
              )}
              {group.items.map((item, i) => {
                const t = tiles[i];
                if (!t) return null;
                const heat = item.uses
                  ? 0.25 + 0.75 * (Math.log1p(item.uses) / Math.log1p(maxUses))
                  : 0;
                const lit = matchesFocus(item, focus);
                return (
                  <Button
                    key={item.name}
                    variant="ghost"
                    aria-label={`${item.name}, ${item.uses} uses, ${item.tokens} tokens`}
                    className={`absolute block h-auto overflow-hidden rounded-[3px] p-0 text-left font-normal focus-visible:outline-2 focus-visible:outline-text-primary ${
                      item.broken
                        ? "ring-2 ring-error ring-inset"
                        : item.cut
                          ? "ring-1 ring-warning ring-inset"
                          : ""
                    }`}
                    style={{
                      left: t.x + 1,
                      top: t.y + 1,
                      width: Math.max(0, t.w - 2),
                      height: Math.max(0, t.h - 2),
                      background: heat
                        ? `oklch(0.668 0.176 293.133 / ${heat})`
                        : "var(--color-bg-tertiary)",
                      opacity: lit ? 1 : 0.15,
                    }}
                    onPointerEnter={(e) => {
                      const r =
                        e.currentTarget.parentElement?.parentElement?.getBoundingClientRect();
                      const tr = e.currentTarget.getBoundingClientRect();
                      if (r) setHover({ item, x: tr.right - r.left + 6, y: tr.top - r.top });
                    }}
                    onFocus={(e) => {
                      const r =
                        e.currentTarget.parentElement?.parentElement?.getBoundingClientRect();
                      const tr = e.currentTarget.getBoundingClientRect();
                      if (r) setHover({ item, x: tr.right - r.left + 6, y: tr.top - r.top });
                    }}
                    onClick={() => open(item.name)}
                  >
                    {t.w > 70 && t.h > 22 && (
                      <span className="block truncate px-1.5 pt-1 text-caption text-text-primary">
                        {item.name}
                      </span>
                    )}
                    {t.w > 70 && t.h > 38 && (
                      <span className="block px-1.5 text-caption text-text-secondary tabular-nums">
                        {formatCount(item.uses)} uses
                      </span>
                    )}
                    {item.update && (
                      <span className="absolute top-1 right-1 size-1.5 rounded-full bg-text-primary" />
                    )}
                  </Button>
                );
              })}
            </div>
          );
        })}
        {hover && (
          <SkillCard
            item={hover.item}
            x={Math.min(hover.x, width - 264)}
            y={Math.min(Math.max(0, hover.y), H - 150)}
          />
        )}
      </div>
    </section>
  );
}
