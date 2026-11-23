import { useDetectedHarnesses } from "./useDetectedHarnesses";

export function useClientIds(): string[] | undefined {
  const { choice, report } = useDetectedHarnesses();
  return choice?.kept ?? report?.harnesses.flatMap((h) => (h.state === "not_found" ? [] : [h.id]));
}

/** Explicitly keeping no clients is different from unavailable detection information. */
export function useClientRows<T extends { harness: string }>(rows: T[]): T[] {
  const ids = useClientIds();
  if (!ids) return rows;
  const selected = new Set(ids);
  return rows.filter((row) => selected.has(row.harness));
}
