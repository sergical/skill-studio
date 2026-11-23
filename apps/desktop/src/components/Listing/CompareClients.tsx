import { useState } from "react";
import { deploymentLabelFromAgentId, homeRelativePath } from "@skill-studio/lib";
import type { HarnessListing, ListingEntry } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { SelectControl } from "../ui/SelectControl";
import { additionalEntries, entrySource, listedEntries } from "./client-allocation";

function AdditionalCopies({ name, entries }: { name: string; entries: ListingEntry[] }) {
  const [limit, setLimit] = useState(10);
  return (
    <details>
      <summary className="cursor-pointer text-small">
        {entries.length} additional {entries.length === 1 ? "entry" : "entries"} in {name}
      </summary>
      <ul className="my-2 list-none divide-y divide-border-subtle pl-3">
        {entries.slice(0, limit).map((entry) => (
          <li className="flex flex-col py-1.5" key={JSON.stringify([entry.name, entry.path])}>
            <span className="text-small">{entry.name}</span>
            <span className="break-all text-caption text-text-secondary">
              {entry.plugin ?? entrySource(entry)}
              {entry.path && ` · ${homeRelativePath(entry.path)}`}
            </span>
          </li>
        ))}
      </ul>
      {entries.length > limit && (
        <Button variant="ghost" size="sm" onClick={() => setLimit(limit + 10)}>
          Show more ({limit} of {entries.length})
        </Button>
      )}
    </details>
  );
}

export function CompareClients({
  row,
  clients,
}: {
  row: HarnessListing;
  clients: HarnessListing[];
}) {
  const [choice, setChoice] = useState("");
  const others = clients.filter((r) => r.harness !== row.harness);
  const other = others.find((r) => r.harness === choice) ?? others[0];
  if (!other) return null;
  const sides = [row, other].map((r) => ({
    name: deploymentLabelFromAgentId(r.harness),
    entries: listedEntries(r),
    scanned: r.evidence !== "preview",
  }));
  const extra = sides.map((s, i) => additionalEntries(s.entries, sides[1 - i].entries));
  return (
    <details className="border-t border-border-subtle py-2">
      <summary className="cursor-pointer text-small">Compare clients</summary>
      <div className="mt-3 flex flex-wrap items-center gap-2 text-small">
        <span>{sides[0].name} and</span>
        <SelectControl
          ariaLabel="Compare with"
          value={other.harness}
          onValueChange={setChoice}
          items={others.map((r) => ({
            value: r.harness,
            label: deploymentLabelFromAgentId(r.harness),
          }))}
        />
      </div>
      <p className="my-2 text-small">
        {sides[0].entries.length - extra[0].length} entries match by name. Each copy counts once.
      </p>
      {sides.map((s, i) => (
        <div key={s.name} className="my-2">
          <p className="m-0 text-small text-text-secondary">
            <strong>{s.name}</strong> · {s.entries.length}{" "}
            {s.scanned ? "found in folders" : "in the estimated listing"}
          </p>
          {extra[i].length > 0 && (
            <AdditionalCopies key={other.harness} name={s.name} entries={extra[i]} />
          )}
        </div>
      ))}
    </details>
  );
}
