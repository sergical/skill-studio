import { useState, type ComponentProps } from "react";
import { ChevronDown } from "lucide-react";
import type { HomeView } from "../../components/Home/HomeView";
import { PageShell } from "../../components/Shell/PageShell";
import {
  FirstScanState,
  type FirstScanStateProps,
} from "../../components/SkillList/FirstScanState";
import { MenuControl, MenuRadioGroup, MenuRadioItem } from "../../components/ui/MenuControl";
import { ClientDirectory, RecentWork, RepairQueue } from "./HomeSurfaces";
import { LibraryOverview } from "./LibraryOverview";

const designs = ["Library overview", "Repairs", "Clients", "Recent work"];

export function HomeDirections(props: ComponentProps<typeof HomeView> & FirstScanStateProps) {
  const [variant, setVariant] = useState(() => {
    const index = Number(new URLSearchParams(location.search).get("v")) - 1;
    return designs[index] ? index : 0;
  });
  const { snapshot, onSelectSkill, active, usageRefreshing } = props;
  const picker = (
    <MenuControl
      align="end"
      triggerClassName="inline-flex items-center gap-1.5 rounded-sm px-2 py-1 text-small text-text-secondary hover:bg-bg-hover focus-visible:outline-accent"
      trigger={
        <>
          Compare designs <ChevronDown size={13} />
        </>
      }
    >
      <MenuRadioGroup
        value={designs[variant]}
        onValueChange={(value) => {
          const index = designs.indexOf(String(value));
          if (index < 0) return;
          setVariant(index);
          const url = new URL(location.href);
          url.searchParams.set("v", String(index + 1));
          history.replaceState(history.state, "", url);
        }}
      >
        {designs.map((name) => (
          <MenuRadioItem key={name} value={name} closeOnClick>
            {name}
          </MenuRadioItem>
        ))}
      </MenuRadioGroup>
    </MenuControl>
  );
  return (
    <PageShell title="Home" actions={picker}>
      {!snapshot ? (
        <FirstScanState isLoading={props.isLoading} error={props.error} onRetry={props.onRetry} />
      ) : variant === 1 ? (
        <RepairQueue snapshot={snapshot} open={onSelectSkill} />
      ) : variant === 2 ? (
        <ClientDirectory />
      ) : variant === 3 ? (
        <RecentWork snapshot={snapshot} open={onSelectSkill} />
      ) : (
        <LibraryOverview
          snapshot={snapshot}
          active={active}
          usageRefreshing={usageRefreshing}
          open={onSelectSkill}
        />
      )}
    </PageShell>
  );
}
