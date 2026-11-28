// ============================================================================
// CommandPalette - ⌘K from anywhere: run an action, jump to a place, or open
// a skill by name. A combobox/listbox pair, not a menu - one text input, one
// scrollable results list, `aria-activedescendant` driving the highlight.
// No animation anywhere in this component: it opens and closes instantly,
// and the highlight moves without a transition.
// ============================================================================

import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from "react";
import { Search } from "lucide-react";
import { Dialog, DialogContent, Input, Kbd } from "@skill-studio/ui";
import type { SkillSnapshot } from "@skill-studio/lib";
import { keyShortcutsFor } from "../../lib/app-shortcuts";
import { useAppStore } from "../../store/appStore";
import { rankItems, SECTION_LABEL } from "./command-palette-items";
import type { PaletteItem, PaletteSection } from "./command-palette-items";
import { usePaletteItems } from "./usePaletteItems";

interface CommandPaletteProps {
  snapshot: SkillSnapshot | undefined;
  requestRescan: () => Promise<void>;
}

/** No transition, no fade, no zoom - only instant open/close and an instant highlight move. */
const NO_ANIMATION =
  "duration-0! animate-none! transition-none! data-open:animate-none! data-closed:animate-none! data-open:duration-0! data-closed:duration-0!";

export function CommandPalette({ snapshot, requestRescan }: CommandPaletteProps) {
  const open = useAppStore((state) => state.commandPaletteOpen);
  const setOpen = useAppStore((state) => state.setCommandPaletteOpen);

  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);
  // Tracked so the query/highlight reset below can happen during rendering, on the render where
  // `open` flips, instead of a follow-up effect setting that state after commit.
  const [prevOpen, setPrevOpen] = useState(open);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  // WebKit fires mousemove at an unchanged position when the list scrolls under a resting
  // pointer; only a real move may take the highlight from the keyboard.
  const pointerRef = useRef<{ x: number; y: number } | null>(null);

  function close() {
    setOpen(false);
  }

  function run(item: PaletteItem) {
    // The item decides where focus goes next (e.g. the filter input); restoring the
    // pre-palette element would take it back.
    restoreFocusRef.current = null;
    close();
    item.run();
  }

  const items = usePaletteItems(snapshot, requestRescan);

  const results = rankItems(items, query);

  function onOptionMouseMove(e: ReactMouseEvent, index: number) {
    const last = pointerRef.current;
    if (last && last.x === e.clientX && last.y === e.clientY) return;
    pointerRef.current = { x: e.clientX, y: e.clientY };
    setHighlight(index);
  }

  function onQueryChange(value: string) {
    setQuery(value);
    setHighlight(0);
  }

  // Resetting query/highlight is derived state (it only depends on the render where `open`
  // flips), so it's set here during rendering rather than in the effect below.
  if (open !== prevOpen) {
    setPrevOpen(open);
    if (open) {
      setQuery("");
      setHighlight(0);
    }
  }

  useEffect(() => {
    if (open) {
      restoreFocusRef.current =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
    } else {
      restoreFocusRef.current?.focus({ preventScroll: true });
      restoreFocusRef.current = null;
    }
  }, [open]);

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-index="${highlight}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [highlight]);

  function onKeyDown(e: ReactKeyboardEvent) {
    if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      setHighlight((h) => Math.min(h + 1, results.length - 1));
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      setHighlight((h) => Math.max(h - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (results[highlight]) run(results[highlight]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }

  const activeId = results[highlight]
    ? `command-palette-option-${results[highlight].id}`
    : undefined;

  // Grouped for rendering, in the order `rankItems` produced, so the highlight moves down the
  // list as drawn.
  const sections: { section: PaletteSection; items: PaletteItem[] }[] = [
    ...new Set(results.map((item) => item.section)),
  ].map((section) => ({ section, items: results.filter((item) => item.section === section) }));

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        showCloseButton={false}
        aria-label="Command palette"
        overlayClassName={NO_ANIMATION}
        className={`top-[15vh] left-1/2 w-[560px] max-w-[calc(100%-2rem)] -translate-x-1/2 translate-y-0 gap-0 rounded-lg border border-border bg-bg-secondary p-0 shadow-lg ring-0 ${NO_ANIMATION}`}
      >
        <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-2.5">
          <Search size={16} aria-hidden className="shrink-0 text-text-tertiary" />
          <Input
            ref={inputRef}
            role="combobox"
            aria-label="Command or skill"
            aria-expanded="true"
            aria-controls="command-palette-listbox"
            aria-activedescendant={activeId}
            autoFocus
            placeholder="Type a command or skill"
            className="h-8 border-none bg-transparent px-0 shadow-none focus-visible:border-none dark:bg-transparent"
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            onKeyDown={onKeyDown}
          />
        </div>
        <div
          ref={listRef}
          id="command-palette-listbox"
          role="listbox"
          aria-label="Command palette results"
          className="max-h-[420px] overflow-y-auto py-1 pl-1 gutter-pr-1"
        >
          {sections.map(({ section, items: sectionItems }) => (
            <div
              key={section}
              role="group"
              aria-label={SECTION_LABEL[section]}
              className="not-first:mt-2"
            >
              <div className="px-2 pt-1.5 pb-1 text-caption text-text-tertiary">
                {SECTION_LABEL[section]}
              </div>
              {sectionItems.map((item) => {
                const index = results.indexOf(item);
                const isHighlighted = index === highlight;
                return (
                  <div
                    key={item.id}
                    id={`command-palette-option-${item.id}`}
                    role="option"
                    aria-selected={isHighlighted}
                    data-index={index}
                    className={`flex h-8 cursor-pointer items-center gap-2 rounded-sm px-2 text-body text-text-secondary ${
                      isHighlighted ? "bg-bg-active text-text-primary" : ""
                    }`}
                    onMouseMove={(e) => onOptionMouseMove(e, index)}
                    onClick={() => run(item)}
                  >
                    {item.icon && (
                      <span className="flex size-3.5 shrink-0 items-center">{item.icon}</span>
                    )}
                    <span className="min-w-0 flex-1 truncate">{item.label}</span>
                    {item.secondary && (
                      <span className="shrink-0 text-caption text-text-tertiary">
                        {item.secondary}
                      </span>
                    )}
                    {item.shortcut && (
                      <span
                        className="flex shrink-0 items-center gap-0.5"
                        aria-keyshortcuts={keyShortcutsFor(item.shortcut)}
                      >
                        {item.shortcut.keys.map((key) => (
                          <Kbd key={key}>{key}</Kbd>
                        ))}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {results.length === 0 && (
            <div className="px-2 py-3 text-center text-small text-text-tertiary">No matches</div>
          )}
        </div>
        <div role="status" aria-live="polite" className="sr-only">
          {results.length} result{results.length === 1 ? "" : "s"}
        </div>
      </DialogContent>
    </Dialog>
  );
}
