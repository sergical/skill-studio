import { ChevronDown } from "lucide-react";

/**
 * The "Open" menu trigger's label. The prerendered page and the mounted menu both draw it,
 * so the trigger looks the same before and after the menu script loads.
 */
export function DocsAgentTriggerContent() {
  return (
    <>
      <span>Open</span>
      <ChevronDown aria-hidden="true" size={14} className="docs-chevron" />
    </>
  );
}
