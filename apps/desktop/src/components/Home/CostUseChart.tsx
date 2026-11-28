// ============================================================================
// CostUseChart - Every skill as a dot: right is uses in 30 days, up is the
// room its description takes in every message. Dragging a box, or the
// "Select costly, unused" button, picks skills; the bar under the chart says
// what parking them would free and parks them through the shared row action.
// ============================================================================

import { useState } from "react";
import type { PointerEvent } from "react";
import { formatCount } from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";

const W = 560;
const H = 240;
const PAD = 28;

export interface CostUsePoint {
  skill: InstalledSkill;
  uses: number;
  /** Read by an agent with history and no use in 30 days: what "Select costly, unused" picks. */
  unused: boolean;
}

interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function CostUseChart({
  points,
  noHistory,
  open,
  park,
}: {
  points: CostUsePoint[];
  /** Skills left off the chart because no agent that reads them has readable history. */
  noHistory: number;
  open: (name: string) => void;
  park: (skills: InstalledSkill[]) => void;
}) {
  // A dragged box, or every unused skill. A box around the unused dots could catch a used one.
  const [box, setBox] = useState<Box | "unused" | null>(null);
  const [dragging, setDragging] = useState(false);
  const maxUses = Math.max(1, ...points.map((p) => p.uses));
  const maxTokens = Math.max(1, ...points.map((p) => p.skill.description_tokens));
  // Unused skills share x = 0, so each gets a small fixed offset by its index to stay visible.
  const placed = points.map((p, index) => ({
    ...p,
    x:
      PAD +
      (Math.log1p(p.uses) / Math.log1p(maxUses)) * (W - PAD - 8) +
      (p.uses ? 0 : (index % 9) * 4),
    y: H - PAD - (p.skill.description_tokens / maxTokens) * (H - PAD - 8),
  }));
  const inBox = (p: { x: number; y: number; unused: boolean }) =>
    box === "unused"
      ? p.unused
      : box !== null &&
        p.x >= Math.min(box.x0, box.x1) &&
        p.x <= Math.max(box.x0, box.x1) &&
        p.y >= Math.min(box.y0, box.y1) &&
        p.y <= Math.max(box.y0, box.y1);
  const picked = placed.filter(inBox);
  const freed = picked.reduce((sum, p) => sum + p.skill.description_tokens, 0);
  const usedPicked = picked.filter((p) => p.uses > 0).length;

  const toSvg = (event: PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * W,
      y: ((event.clientY - rect.top) / rect.height) * H,
    };
  };

  return (
    <div>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={`${points.length} skills by uses and description size`}
        className="w-full cursor-crosshair touch-none select-none"
        onPointerDown={(event) => {
          const at = toSvg(event);
          setBox({ x0: at.x, y0: at.y, x1: at.x, y1: at.y });
          setDragging(true);
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          if (!dragging) return;
          const at = toSvg(event);
          setBox((current) =>
            current && current !== "unused" ? { ...current, x1: at.x, y1: at.y } : current,
          );
        }}
        onPointerUp={() => setDragging(false)}
      >
        <line x1={PAD} y1={H - PAD} x2={W - 8} y2={H - PAD} className="stroke-border" />
        <line x1={PAD} y1={8} x2={PAD} y2={H - PAD} className="stroke-border" />
        <text x={PAD} y={H - 8} className="fill-text-tertiary text-[11px]">
          No uses
        </text>
        <text x={W - 8} y={H - 8} textAnchor="end" className="fill-text-tertiary text-[11px]">
          {formatCount(maxUses)} uses
        </text>
        <text x={PAD + 4} y={18} className="fill-text-tertiary text-[11px]">
          {formatCount(maxTokens)} tokens per message
        </text>
        {placed.map((p) => (
          <circle
            key={p.skill.name}
            cx={p.x}
            cy={p.y}
            r={p.uses ? 4.5 : 3}
            className={`${p.uses ? "fill-data" : "fill-text-quaternary"} ${inBox(p) ? "stroke-text-primary" : ""}`}
            onClick={() => open(p.skill.name)}
          >
            <title>{`${p.skill.name} · ${p.uses} uses · ${p.skill.description_tokens} tokens`}</title>
          </circle>
        ))}
        {box && box !== "unused" && (
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
      <div className="mt-2 flex min-h-8 flex-wrap items-center gap-3 text-small text-text-secondary">
        <Button size="sm" variant="outline" onClick={() => setBox("unused")}>
          Select costly, unused
        </Button>
        {picked.length ? (
          <>
            <span role="status">
              {picked.length} selected · frees about {formatCount(freed)} tokens in every message
              {usedPicked ? ` · ${usedPicked} of them were used` : ""}
            </span>
            <Button size="sm" onClick={() => park(picked.map((p) => p.skill))}>
              Select {picked.length} to park
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setBox(null)}>
              Clear
            </Button>
          </>
        ) : (
          <span>Drag over the dots to select skills.</span>
        )}
        {noHistory > 0 && (
          <span>
            {noHistory} not shown · No readable history from the agents that read{" "}
            {noHistory === 1 ? "it" : "them"}
          </span>
        )}
      </div>
    </div>
  );
}
