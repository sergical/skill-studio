// PROTOTYPE (throwaway, branch prototype/project-skill-set): round 3, three new directions
// drawn from Mobbin (Screen Time recaps, storage managers, status pages). Rejected so far:
// Current, Lanes, Field, Field + list, Map. Switched with the picker at the bottom (1-3 or the arrow
// keys, ?v= keeps the choice). Mounted from HomeView by one
// dev-only block fenced with "PROTOTYPE home-library"; delete this folder and that block.

import type { ReactNode } from "react";
import type { HarnessListing, SkillSnapshot } from "@skill-studio/lib";
import { useClientIds } from "../../hooks/useClientRows";
import { useHomeListing } from "../../components/Home/useHomeListing";
import type { HomeGroups } from "../../components/Home/home-inbox-data";
import { buildLibrary } from "./library-model";
import { Picker, usePicker } from "./shared";
import { RecapVariant } from "./RecapVariant";
import { AskLineVariant, AskVariant, OneLineVariant, SinceVariant } from "./SimpleVariants";
import type { SimpleProps } from "./SimpleVariants";

const NAMES = ["Recap", "Since last time", "One line", "Ask", "Ask in one line"];

export function HomeLibraryPrototype({
  snapshot,
  groups,
  active,
  baseline,
  open,
}: {
  snapshot: SkillSnapshot;
  groups: HomeGroups;
  active: boolean;
  baseline: ReactNode;
  open: (name: string) => void;
}) {
  const { current, set } = usePicker(NAMES);
  const agents = useClientIds();
  const { load } = useHomeListing(snapshot.revision, active && Boolean(agents?.length));
  const listings = (load?.data?.listings ?? []).filter(
    (row) => row.scope.scope === "global" && (!agents || agents.includes(row.harness)),
  );
  // Plugin skills and history count too: a user with only plugins still has a week to report.
  const anything = snapshot.skills.length > 0 || snapshot.invocations.length > 0;
  const { skills } = buildLibrary(snapshot, groups, listings, agents);
  // On in the native dev app, or in the browser harness with ?v=; otherwise today's Home, so the
  // harness e2e specs keep testing the real page.
  const on = !("__harness" in window) || new URLSearchParams(location.search).has("v");
  if (!on) return baseline;
  return (
    <>
      {/* With no skills every picture is empty, so each variant shows today's first-run Home. */}
      {!anything && baseline}
      {anything && current === 0 && (
        <RecapVariant
          snapshot={snapshot}
          skills={skills}
          agents={agents}
          listings={listings}
          counts={{
            broken: [...new Set(groups.broken.map((issue) => issue.skill.name))],
            warnings: [...new Set(groups.warnings.map((issue) => issue.skill.name))],
            updates: groups.updates.map((skill) => skill.name),
            unused: groups.unused.map((skill) => skill.name),
          }}
          open={open}
        />
      )}
      {anything && current > 0 && (
        <SimpleHome
          index={current}
          listings={listings}
          props={{
            snapshot,
            skills,
            agents,
            counts: {
              broken: [...new Set(groups.broken.map((issue) => issue.skill.name))],
              warnings: [...new Set(groups.warnings.map((issue) => issue.skill.name))],
              updates: groups.updates.map((skill) => skill.name),
              unused: groups.unused.map((skill) => skill.name),
            },
            open,
          }}
        />
      )}
      {active && <Picker names={NAMES} current={current} set={set} />}
    </>
  );
}

function SimpleHome({
  index,
  props,
  listings,
}: {
  index: number;
  props: SimpleProps;
  listings: HarnessListing[];
}) {
  if (index === 1) return <SinceVariant {...props} />;
  if (index === 2) return <OneLineVariant {...props} />;
  if (index === 3) return <AskVariant {...props} />;
  return <AskLineVariant {...props} listings={listings} />;
}
