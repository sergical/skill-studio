// ============================================================================
// HarnessMark - one reading harness's mark: a fixed-size box that shows the
// harness's brand icon, badged with a small link glyph anchored to the icon
// when its copy links to the Universal folder. Every mark has a one-line
// tooltip naming the harness and how it reaches the skill, since the 11px
// badge is too small to carry that alone.
// ============================================================================

import { Link2, Link2Off } from "lucide-react";
import { HarnessIcon } from "../ui/HarnessIcon";
import { RichTooltip } from "../ui/RichTooltip";
import type { HarnessReach } from "./skill-row-state";

export { HarnessIcon as HarnessGlyph } from "../ui/HarnessIcon";

function harnessMarkAriaLabel(reach: HarnessReach): string {
  const how =
    reach.how === "linked"
      ? "link to the Universal folder"
      : reach.how === "broken"
        ? "broken link"
        : reach.how === "universal"
          ? "source in the Universal folder"
          : "own copy";
  return reach.disabled ? `${reach.label} · ${how} · disabled` : `${reach.label} · ${how}`;
}

/** One line per reached harness: `<glyph> <label> · <how>[ · <state>]` - the
 * words the 11px badge cannot carry on its own. The how-word (linked /
 * source / own copy) always shows; a broken link replaces it in the error
 * tone, and disabled follows it in the warning tone. */
function harnessMarkTooltip(reach: HarnessReach) {
  const how =
    reach.how === "broken"
      ? { text: "link broken", tone: "text-error" }
      : reach.how === "linked"
        ? { text: "linked to Universal folder", tone: "text-text-tertiary" }
        : reach.how === "universal"
          ? { text: "source in Universal folder", tone: "text-text-tertiary" }
          : { text: "own copy", tone: "text-text-tertiary" };
  return (
    <span className="flex items-center gap-1.5 text-small">
      <HarnessIcon harness={reach.harness} size={14} />
      <span>{reach.label}</span>
      <span className={how.tone}>· {how.text}</span>
      {reach.disabled && <span className="text-warning">· disabled</span>}
    </span>
  );
}

/** One reading harness's mark: the brand icon at full tone, muted when disabled, badged with a
 * small link (or broken-link) glyph anchored to the icon itself (not the slot) when its copy is a
 * link, so every harness's badge lands at the same offset from its icon's corner. */
export function HarnessMark({ reach, size: sizeProp }: { reach: HarnessReach; size?: number }) {
  const size = sizeProp ?? 13;
  const badge = reach.how === "linked" || reach.how === "broken" ? reach.how : null;
  const mark = (
    <span
      role="img"
      className="group relative inline-flex size-5 items-center justify-center"
      aria-label={harnessMarkAriaLabel(reach)}
    >
      <span className="relative inline-flex" style={{ width: size, height: size }}>
        <HarnessIcon harness={reach.harness} size={size} muted={reach.disabled} />
        {badge && (
          <span className="absolute -right-1 -bottom-1 inline-flex size-2.5 items-center justify-center rounded-full bg-bg-primary group-hover:bg-bg-secondary">
            {badge === "linked" ? (
              <Link2 size={7} className="text-text-secondary" aria-hidden />
            ) : (
              <Link2Off size={7} className="text-error" aria-hidden />
            )}
          </span>
        )}
      </span>
    </span>
  );
  return <RichTooltip content={harnessMarkTooltip(reach)}>{mark}</RichTooltip>;
}
