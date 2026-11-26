// PROTOTYPE (throwaway, branch prototype/project-skill-set): four ways to show the whole
// library at the top of Home, switched with the picker at the bottom (1-4 or the arrow keys,
// ?v= keeps the choice). "Current" is today's Home, unchanged. Mounted from HomeView by one
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
import { LanesVariant } from "./LanesVariant";
import { MapVariant } from "./MapVariant";

const NAMES = ["Current", "Field", "Map", "Lanes"];

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
  return (
    <>
      {/* With no skills every picture is empty, so each variant shows today's first-run Home. */}
      {(current === 0 || skills.length === 0) && baseline}
      {skills.length > 0 && current === 1 && (
        <FieldVariant
          skills={skills}
          history={history}
          open={open}
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
      {skills.length > 0 && current === 3 && (
        <LanesVariant skills={skills} history={history} listings={listings} open={open} />
      )}
      {active && <Picker names={NAMES} current={current} set={set} />}
    </>
  );
}
