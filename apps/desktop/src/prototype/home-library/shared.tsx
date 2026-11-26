// PROTOTYPE (throwaway, branch prototype/project-skill-set): pieces every Home library variant
// shares - the skill card, the width hook, the summary strip and the variant picker from the
// emil-prototype skill's PICKER.md.

import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { deploymentLabelFromAgentId, formatCount } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { STATE_WORDS } from "./library-model";
import type { LibrarySkill } from "./library-model";

export function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.floor(entry.contentRect.width)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** The details for one skill, shown next to whatever the pointer or the keyboard is on. */
export function SkillCard({ item, x, y }: { item: LibrarySkill; x: number; y: number }) {
  const agents = Object.entries(item.listed);
  return (
    <div
      className="pointer-events-none absolute z-10 w-64 rounded-lg border border-border bg-bg-elevated p-3 text-small shadow-lg"
      style={{ left: x, top: y }}
    >
      <p className="truncate text-body font-semibold">{item.name}</p>
      <p className="text-text-secondary">{item.source}</p>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 tabular-nums">
        <dt className="text-text-tertiary">Uses, 30 days</dt>
        <dd>{formatCount(item.uses)}</dd>
        <dt className="text-text-tertiary">Description</dt>
        <dd>{formatCount(item.tokens)} tokens per message</dd>
        {agents.map(([agent, state]) => (
          <Fragment key={agent}>
            <dt className="text-text-tertiary">{deploymentLabelFromAgentId(agent)}</dt>
            <dd className={state === "name_only" || state === "omitted" ? "text-warning" : ""}>
              {STATE_WORDS[state]}
            </dd>
          </Fragment>
        ))}
      </dl>
      {(item.broken || item.update || item.warning) && (
        <p className="mt-2 flex flex-wrap gap-1.5">
          {item.broken && <Tag tone="text-error">Doesn’t load</Tag>}
          {item.warning && <Tag tone="text-warning">Needs a look</Tag>}
          {item.update && <Tag tone="text-text-primary">Update available</Tag>}
        </p>
      )}
    </div>
  );
}

function Tag({ tone, children }: { tone: string; children: ReactNode }) {
  return <span className={`rounded-full bg-bg-tertiary px-2 py-0.5 ${tone}`}>{children}</span>;
}

export type Focus = "all" | "broken" | "cut" | "update" | "unused";

/** Counts across the top; each one highlights its skills in the picture below. */
export function FocusStrip({
  skills,
  history,
  focus,
  setFocus,
}: {
  skills: LibrarySkill[];
  history: boolean;
  focus: Focus;
  setFocus: (focus: Focus) => void;
}) {
  const items: [Focus, string, number, string][] = [
    ["all", "skills", skills.length, "text-text-primary"],
    ["broken", "don’t load", skills.filter((s) => s.broken).length, "text-error"],
    ["cut", "cut from a skill listing", skills.filter((s) => s.cut).length, "text-warning"],
    ["update", "updates", skills.filter((s) => s.update).length, "text-text-primary"],
  ];
  if (history)
    items.push([
      "unused",
      "unused in 30 days",
      skills.filter((s) => !s.uses).length,
      "text-text-secondary",
    ]);
  return (
    <div role="radiogroup" aria-label="Highlight" className="flex flex-wrap gap-1.5">
      {items.map(([id, label, count, tone]) => (
        <Button
          key={id}
          variant="ghost"
          role="radio"
          aria-checked={focus === id}
          onClick={() => setFocus(focus === id && id !== "all" ? "all" : id)}
          className={`h-8 rounded-md border px-2.5 text-small font-normal tabular-nums transition-colors duration-150 ${
            focus === id
              ? "border-text-tertiary bg-bg-tertiary"
              : "border-border-subtle hover:border-border"
          }`}
        >
          <span className={`font-semibold ${tone}`}>{formatCount(count)}</span>{" "}
          <span className="text-text-secondary">{label}</span>
        </Button>
      ))}
    </div>
  );
}

export function matchesFocus(item: LibrarySkill, focus: Focus): boolean {
  if (focus === "broken") return item.broken;
  if (focus === "cut") return item.cut;
  if (focus === "update") return item.update;
  if (focus === "unused") return item.uses === 0;
  return true;
}

const PICKER_CSS = `
.proto-picker { position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%); z-index: 2147483647; display: flex; align-items: center; gap: 2px; padding: 4px; border-radius: 999px; background: rgba(10, 10, 10, 0.82); -webkit-backdrop-filter: blur(12px) saturate(1.4); backdrop-filter: blur(12px) saturate(1.4); box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.08) inset, 0 8px 24px rgba(0, 0, 0, 0.24), 0 2px 6px rgba(0, 0, 0, 0.12); font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; font-size: 13px; line-height: 1; -webkit-font-smoothing: antialiased; user-select: none; -webkit-user-select: none; }
.proto-picker-highlight { position: absolute; top: 4px; left: 0; height: 28px; border-radius: 999px; background: rgba(255, 255, 255, 0.12); will-change: transform; }
.proto-picker[data-ready] .proto-picker-highlight { transition: transform 250ms cubic-bezier(0.23, 1, 0.32, 1), width 250ms cubic-bezier(0.23, 1, 0.32, 1); }
@media (prefers-reduced-motion: reduce) { .proto-picker[data-ready] .proto-picker-highlight { transition: none; } }
.proto-picker-item { position: relative; display: flex; align-items: center; height: 28px; padding: 0 12px; border: 0; border-radius: 999px; background: transparent; color: rgba(255, 255, 255, 0.55); font: inherit; cursor: pointer; transition: color 150ms ease-out; }
.proto-picker-item:hover { color: rgba(255, 255, 255, 0.85); }
.proto-picker-item:active { transform: scale(0.97); }
.proto-picker-item:focus-visible { outline: 2px solid rgba(255, 255, 255, 0.4); outline-offset: 2px; }
.proto-picker-item[data-active] { color: #fff; }
`;

/** The emil-prototype picker: 1-N and the arrow keys switch, ?v= keeps the choice. */
export function usePicker(names: string[]) {
  const [current, setCurrent] = useState(
    () =>
      Math.min(
        names.length,
        Math.max(1, Number(new URLSearchParams(location.search).get("v")) || 1),
      ) - 1,
  );
  const set = (i: number) => {
    if (i < 0 || i >= names.length) return;
    setCurrent(i);
    const url = new URL(location.href);
    url.searchParams.set("v", String(i + 1));
    history.replaceState(history.state, "", url);
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target instanceof HTMLElement ? e.target : null;
      // The picture and the Home grid use the arrow keys themselves.
      if (
        target?.closest(
          "input, textarea, select, [contenteditable], [role=grid], [data-proto-keys]",
        )
      )
        return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const num = Number.parseInt(e.key, 10);
      if (num >= 1 && num <= names.length) set(num - 1);
      else if (e.key === "ArrowRight") set((current + 1) % names.length);
      else if (e.key === "ArrowLeft") set((current - 1 + names.length) % names.length);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  return { current, set };
}

export function Picker({
  names,
  current,
  set,
}: {
  names: string[];
  current: number;
  set: (i: number) => void;
}) {
  const nav = useRef<HTMLElement>(null);
  const highlight = useRef<HTMLSpanElement>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  useLayoutEffect(() => {
    const el = items.current[current];
    if (!el || !highlight.current) return;
    highlight.current.style.width = `${el.offsetWidth}px`;
    highlight.current.style.transform = `translateX(${el.offsetLeft}px)`;
  });
  useEffect(() => {
    const id = requestAnimationFrame(() =>
      requestAnimationFrame(() => nav.current?.setAttribute("data-ready", "")),
    );
    return () => cancelAnimationFrame(id);
  }, []);
  return (
    <>
      <style>{PICKER_CSS}</style>
      <nav ref={nav} className="proto-picker" aria-label="Prototype variants">
        <span ref={highlight} className="proto-picker-highlight" aria-hidden="true" />
        {names.map((name, i) => (
          <Button
            key={name}
            variant="ghost"
            ref={(el) => {
              items.current[i] = el;
            }}
            className="proto-picker-item"
            data-active={i === current ? "" : undefined}
            aria-current={i === current ? "true" : undefined}
            onClick={() => set(i)}
          >
            {name}
          </Button>
        ))}
      </nav>
    </>
  );
}
