import type { Pack } from "@skill-studio/lib";

export const plural = (count: number, one: string) => `${count} ${one}${count === 1 ? "" : "s"}`;

/**
 * One line after an upstream check: how many packs it could check, what it
 * found, or that nothing is new. Packs with no Git source or pinned to a
 * commit are not checked, so they never count toward "Up to date".
 */
export function checkResult(packs: Pack[]): string {
  const checked = packs.filter((pack) => pack.remote.state === "checked").length;
  // A moved pack's original carries its own source's error, so a pack
  // counts once however many of its two checks failed.
  const failed = packs.filter(
    (pack) => pack.remote.state === "failed" || pack.original?.remote.state === "failed",
  ).length;
  if (checked === 0 && failed === 0) return "No pack can be checked upstream";
  const behind = packs.reduce((sum, pack) => sum + pack.behind_count, 0);
  const added = packs.reduce((sum, pack) => sum + pack.new_upstream.length, 0);
  const found = [
    behind > 0 && `${behind} behind`,
    added > 0 && `${added} new upstream`,
    failed > 0 && `${plural(failed, "check")} failed`,
  ].filter(Boolean);
  const scope =
    checked === packs.length ? `${checked} checked` : `${checked} of ${packs.length} checked`;
  return [scope, ...(found.length === 0 ? ["Up to date"] : found)].join(" · ");
}
