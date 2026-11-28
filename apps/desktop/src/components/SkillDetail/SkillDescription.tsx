// ============================================================================
// SkillDescription - A skill's description on its page. A long one shows its
// first lines, fading out, with Show more / Show less; a short one shows whole.
// Key it by the text: it measures the text once, then on each resize.
// ============================================================================

import { useId, useLayoutEffect, useRef, useState } from "react";
import { Button, cn } from "@skill-studio/ui";

const COLLAPSED_LINES = 3;

interface SkillDescriptionProps {
  text: string;
}

interface Heights {
  collapsed: number;
  full: number;
}

export function SkillDescription({ text }: SkillDescriptionProps) {
  const ref = useRef<HTMLParagraphElement>(null);
  const id = useId();
  const [expanded, setExpanded] = useState(false);
  // Set only when the text is taller than the collapsed lines.
  const [heights, setHeights] = useState<Heights | null>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const style = getComputedStyle(el);
      const line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.5;
      const collapsed = line * COLLAPSED_LINES;
      const full = el.scrollHeight;
      const next = full > collapsed + 1 ? { collapsed, full } : null;
      // The observer fires on every frame of the open and close transition; keep equal heights.
      setHeights((prev) =>
        prev && next && prev.collapsed === next.collapsed && prev.full === next.full ? prev : next,
      );
    };
    measure();
    // A narrower window wraps the text onto more lines.
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const faded = heights !== null && !expanded;
  return (
    <div className="mt-3">
      <p
        id={id}
        ref={ref}
        className={cn(
          "select-text text-pretty text-body leading-[1.5] text-text-secondary",
          heights &&
            "overflow-hidden motion-safe:transition-[max-height] motion-safe:duration-200 motion-safe:ease-out",
          faded && "[mask-image:linear-gradient(to_bottom,black_calc(100%_-_1.5em),transparent)]",
        )}
        style={heights ? { maxHeight: expanded ? heights.full : heights.collapsed } : undefined}
      >
        {text}
      </p>
      {heights && (
        <Button
          variant="ghost"
          size="sm"
          className="mt-1 -ml-2.5 text-text-secondary"
          aria-expanded={expanded}
          aria-controls={id}
          onClick={() => setExpanded((open) => !open)}
        >
          {expanded ? "Show less" : "Show more"}
        </Button>
      )}
    </div>
  );
}
