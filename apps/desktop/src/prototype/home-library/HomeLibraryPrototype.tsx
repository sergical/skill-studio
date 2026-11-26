// PROTOTYPE (throwaway, branch prototype/project-skill-set): round 2, riffing on Field and Map
// (Current and Lanes were rejected). Switched with the picker at the bottom (1-3 or the arrow
// keys, ?v= keeps the choice). Mounted from HomeView by one
// dev-only block fenced with "PROTOTYPE home-library"; delete this folder and that block.

import type { ReactNode } from "react";
import type { SkillSnapshot } from "@skill-studio/lib";
import { useClientIds } from "../../hooks/useClientRows";
import { useAppStore } from "../../store/appStore";
import { useHomeListing } from "../../components/Home/useHomeListing";
import type { HomeGroups } from "../../components/Home/home-inbox-data";
import { buildLibrary } from "./library-model";
import { Picker, usePicker } from "./shared";
import { FieldVariant } from "./FieldVariant";
import { MapVariant } from "./MapVariant";

const NAMES = ["Field", "Field + list", "Map"];

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
  const toggle = useAppStore((s) => s.toggleSkillSelection);
  const clear = useAppStore((s) => s.clearSkillSelection);
  const listings = (load?.data?.listings ?? []).filter(
    (row) => row.scope.scope === "global" && (!agents || agents.includes(row.harness)),
  );
  const { skills, history } = buildLibrary(snapshot, groups, listings, agents);
  // On in the native dev app, or in the browser harness with ?v=; otherwise today's Home, so the
  // harness e2e specs keep testing the real page.
  const on = !("__harness" in window) || new URLSearchParams(location.search).has("v");
  if (!on) return baseline;
  return (
    <>
      {/* With no skills every picture is empty, so each variant shows today's first-run Home. */}
      {skills.length === 0 && baseline}
      {skills.length > 0 && current < 2 && (
        <FieldVariant
          key={current}
          skills={skills}
          history={history}
          open={open}
          list={current === 1}
          select={(picked) => {
            clear();
            for (const skill of picked) {
              const path = skill.deployments[0]?.path;
              if (path) toggle(path);
            }
          }}
        />
      )}
      {skills.length > 0 && current === 2 && (
        <MapVariant skills={skills} history={history} open={open} />
      )}
      {active && <Picker names={NAMES} current={current} set={set} />}
    </>
  );
}
