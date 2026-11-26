// ============================================================================
// AgentTabs - The two halves of the Agents place: what the agent sees of your
// skills (its skill list) and what it used (its recorded sessions). The tab
// keeps the chosen agent, so switching never resets the agent menu.
// ============================================================================

import { useEffect, useRef } from "react";
import { Tabs, TabsList, TabsTrigger } from "@skill-studio/ui";
import { useAppStore } from "../../store/appStore";

const AGENT_TABS = ["listing", "sessions"] as const;
type AgentTab = (typeof AGENT_TABS)[number];

/** No padding, so the first tab's label starts on the page's left edge, like the title (the
 * list's `-ml-px` takes back each tab's 1px transparent border). The prefix matches the line
 * variant's own `px-4`, which a bare `px-0` would lose to. `pb-1` puts the label's middle on
 * the toolbar's other controls' middle, which sit 2px above the row's centre. */
const TRIGGER =
  "group-data-[variant=line]/tabs-list:px-0 pb-1 text-body font-medium text-text-tertiary after:bg-text-primary data-active:text-text-primary hover:text-text-secondary";

/** Each tab is its own page, so a switch unmounts the selected tab. The next page's tabs take
 * focus back when the switch came from here, and only then. */
let switchedHere = false;

export function AgentTabs({ tab, harness }: { tab: AgentTab; harness: string }) {
  const navigate = useAppStore((s) => s.setActiveView);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!switchedHere) return;
    // A frame later: Base UI moves its arrow-key highlight to the selected tab only while focus
    // is outside the list, and it registers the tabs after this effect runs.
    const frame = requestAnimationFrame(() => {
      switchedHere = false;
      list.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  return (
    // The tabs fill the toolbar row down to its line (`-mb-1` undoes the row's `pb-1`), and the
    // list draws no line of its own: the row's line is the one line under the header, and the
    // selected tab's underline sits on it.
    <Tabs
      className="-mb-1 self-stretch"
      value={tab}
      onValueChange={(value) => {
        const kind = AGENT_TABS.find((item) => item === value);
        if (!kind || kind === tab) return;
        switchedHere = true;
        navigate({ kind, harness: harness || undefined });
      }}
    >
      <TabsList
        ref={list}
        variant="line"
        aria-label="Agent report"
        className="-ml-px h-full gap-5 border-b-0 group-data-horizontal/tabs:h-full"
      >
        <TabsTrigger value="listing" className={TRIGGER}>
          What it sees
        </TabsTrigger>
        <TabsTrigger value="sessions" className={TRIGGER}>
          What it used
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
}
